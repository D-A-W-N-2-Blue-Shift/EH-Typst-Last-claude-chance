// ============================================================================
// modules/editor/src/highlight.rs — Coloration Typst Kate-like
//
// Le Typst reste VISIBLE : les `*` du gras, les `=` des titres, tout est à
// l'écran, coloré. Pas de WYSIWYG.
//
// Mécanique (contrainte critique du brief) :
//   - syntect tokenise (syntaxes Typst + YAML du set par défaut), les
//     COULEURS viennent du thème RON — jamais des thèmes syntect.
//   - Cache d'états par ligne : state_cache[i] = (ParseState, ScopeStack)
//     exacts à la FIN de la ligne i. Scroller à la ligne 80 d'un frontmatter
//     de 150 lignes part de state_cache[79] → O(1) + O(visible).
//   - Frappe → invalidation des états en AVAL de la ligne touchée seulement.
//   - Rattrapage budgété : au plus CATCHUP_BUDGET lignes parsées par frame.
//     Au-delà, les lignes s'affichent en couleur de base et la frame
//     suivante continue — jamais de freeze UI.
//   - Frontmatter YAML : la zone --- … --- en tête est parsée avec la
//     syntaxe YAML (couleur dédiée), le corps avec Typst. Les wikilinks
//     sont détectés dans les DEUX (overlay regex, O(visible)).
// ============================================================================

use ropey::Rope;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

use crate::config::{Config, Theme};
use crate::wikilinks;

/// Lignes parsées au maximum par frame pour rattraper le cache.
const CATCHUP_BUDGET: usize = 2000;
/// Garde-fou : un frontmatter ne sera pas cherché au-delà de cette ligne.
const FRONTMATTER_MAX_LINES: usize = 500;

/// Un style de tronçon, prêt à devenir un TextFormat egui.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanStyle {
    pub color: egui::Color32,
    pub background: Option<egui::Color32>,
    pub italics: bool,
    pub bold: bool,
    pub underline: bool,
    /// Échelle de police (titres H1-H6). 1.0 partout ailleurs.
    pub scale: f32,
}

impl SpanStyle {
    fn plain(color: egui::Color32) -> Self {
        Self {
            color,
            background: None,
            italics: false,
            bold: false,
            underline: false,
            scale: 1.0,
        }
    }
}

/// Un tronçon de ligne [start, end) en OCTETS dans la ligne.
#[derive(Debug, Clone)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: SpanStyle,
}

/// Ce que le viewport demande pour une ligne : les tronçons + le contexte
/// de bloc (fond code block, barre de citation, échelle titre).
#[derive(Debug, Clone, Default)]
pub struct LineSpans {
    pub spans: Vec<Span>,
    /// La ligne entière est dans un bloc de code ``` (fond dédié).
    pub in_code_block: bool,
    /// La ligne est une citation (barre verticale à gauche).
    pub is_blockquote: bool,
    /// Échelle de la ligne si titre (sinon 1.0).
    pub scale: f32,
    /// Wikilinks de la ligne : (octets début/fin du LIEN complet, nom).
    pub wikilinks: Vec<(usize, usize, String)>,
}

/// Les syntaxes, chargées une fois pour tout le module.
pub struct Syntaxes {
    set: SyntaxSet,
    typst: usize,
    yaml: usize,
}

impl Syntaxes {
    pub fn load() -> Self {
        let set = SyntaxSet::load_defaults_newlines();
        // Le set par défaut contient toujours ≥ 1 syntaxe : à défaut de Typst
        // ou YAML (jamais observé), on retombe sur l'index 0 (coloration
        // dégradée) plutôt que de paniquer. Doctrine §5/§6 : zéro expect().
        let typst = set
            .syntaxes()
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case("Typst"))
            .or_else(|| {
                set.syntaxes()
                    .iter()
                    .position(|s| s.name.eq_ignore_ascii_case("Markdown"))
            })
            .unwrap_or(0);
        let yaml = set
            .syntaxes()
            .iter()
            .position(|s| s.name == "YAML")
            .unwrap_or(0);
        Self { set, typst, yaml }
    }
}

/// L'état (ParseState, ScopeStack) à la fin d'une ligne.
#[derive(Clone)]
struct LineState {
    parse: ParseState,
    scopes: ScopeStack,
}

/// Cache de coloration d'UN buffer vu par UNE fenêtre.
/// (Deux fenêtres sur le même buffer = deux caches : mémoire négligeable,
/// et chacune invalide selon sa propre version vue.)
pub struct HighlightCache {
    /// states[i] = état à la fin de la ligne i. len = lignes déjà parsées.
    states: Vec<LineState>,
    /// Version du buffer pour laquelle ce cache est valide.
    version: u64,
    /// Fin (exclusive) de la zone frontmatter : lignes 0..fm_end, 0 = aucune.
    pub frontmatter_end: usize,
}

impl Default for HighlightCache {
    fn default() -> Self {
        Self {
            states: Vec::new(),
            version: u64::MAX,
            frontmatter_end: 0,
        }
    }
}

/// Scopes pré-compilés pour la classification (alloués une fois).
struct Scopes {
    heading: Scope,
    headings: [Scope; 6],
    bold: Scope,
    italic: Scope,
    raw_inline: Scope,
    raw_block: Scope,
    quote: Scope,
    punctuation: Scope,
    link: Scope,
    yaml_key: Scope,
    yaml_string: Scope,
}

impl Scopes {
    fn new() -> Self {
        // Scopes constants : un scope invalide (jamais observé) retombe sur le
        // scope vide (matche rien) au lieu de paniquer. Doctrine §5/§6.
        let s = |n: &str| Scope::new(n).unwrap_or_default();
        Self {
            heading: s("markup.heading"),
            headings: [
                s("markup.heading.1"),
                s("markup.heading.2"),
                s("markup.heading.3"),
                s("markup.heading.4"),
                s("markup.heading.5"),
                s("markup.heading.6"),
            ],
            bold: s("markup.bold"),
            italic: s("markup.italic"),
            raw_inline: s("markup.raw.inline"),
            raw_block: s("markup.raw.block"),
            quote: s("markup.quote"),
            punctuation: s("punctuation.definition"),
            link: s("markup.underline.link"),
            yaml_key: s("entity.name.tag.yaml"),
            yaml_string: s("string"),
        }
    }
}

thread_local! {
    static SCOPES: Scopes = Scopes::new();
}

impl HighlightCache {
    /// Synchronise avec le buffer : invalide l'aval de la première ligne
    /// modifiée (ou tout, si le journal d'éditions ne remonte pas assez loin).
    pub fn sync(&mut self, buf: &crate::buffer::Buffer) {
        if buf.version == self.version {
            return;
        }
        match buf.min_line_since(self.version) {
            Some(line) if self.version != u64::MAX => {
                // Le frontmatter peut s'ouvrir/fermer n'importe où au-dessus :
                // si l'édition le touche, on repart de zéro (zone bornée).
                let line = if line < FRONTMATTER_MAX_LINES
                    && (line <= self.frontmatter_end || self.frontmatter_end == 0)
                {
                    0
                } else {
                    line
                };
                self.states.truncate(line);
            }
            _ => self.states.clear(),
        }
        if self.states.is_empty() {
            self.frontmatter_end = detect_frontmatter(&buf.rope);
        }
        self.version = buf.version;
    }

    /// Garantit que les états couvrent [0, target_line), budget par frame.
    /// Retourne false si le budget est épuisé (re-peindre à la frame suivante).
    pub fn ensure(&mut self, rope: &Rope, syn: &Syntaxes, target_line: usize) -> bool {
        let target = target_line.min(rope.len_lines());
        let mut budget = CATCHUP_BUDGET;
        while self.states.len() < target {
            if budget == 0 {
                return false;
            }
            budget -= 1;
            let i = self.states.len();
            let mut state = if i == 0 {
                self.initial_state(syn, 0)
            } else if i == self.frontmatter_end && self.frontmatter_end > 0 {
                // Sortie du frontmatter : le corps repart sur du Typst frais.
                self.initial_state(syn, i)
            } else {
                self.states[i - 1].clone()
            };
            let line = line_with_newline(rope, i);
            if let Ok(ops) = state.parse.parse_line(&line, &syn.set) {
                for (_, op) in &ops {
                    let _ = state.scopes.apply(op);
                }
            }
            self.states.push(state);
        }
        true
    }

    /// L'état de DÉPART pour la ligne `line` (frontière YAML/Typst gérée).
    fn initial_state(&self, syn: &Syntaxes, line: usize) -> LineState {
        let in_fm = line < self.frontmatter_end;
        let syntax = if in_fm {
            &syn.set.syntaxes()[syn.yaml]
        } else {
            &syn.set.syntaxes()[syn.typst]
        };
        LineState {
            parse: ParseState::new(syntax),
            scopes: ScopeStack::new(),
        }
    }

    /// Tronçons stylés d'une ligne. À appeler après ensure() — si la ligne
    /// n'est pas couverte, rend du texte brut (le rattrapage suit son cours).
    pub fn line_spans(
        &mut self,
        rope: &Rope,
        syn: &Syntaxes,
        line_idx: usize,
        theme: &Theme,
        cfg: &Config,
        mut wikilink_valid: impl FnMut(&str) -> bool,
    ) -> LineSpans {
        let line = line_without_newline(rope, line_idx);
        let mut out = LineSpans {
            scale: 1.0,
            ..Default::default()
        };

        let in_frontmatter = line_idx < self.frontmatter_end;

        // Délimiteurs --- du frontmatter : couleur dédiée, pas de YAML.
        if in_frontmatter && (line_idx == 0 || line_idx + 1 == self.frontmatter_end) {
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(theme.markers),
            });
            return out;
        }

        // Commentaire % : ligne entière en discret, rien d'autre.
        let prefix = &cfg.vomi.comment_prefix;
        if !in_frontmatter && !prefix.is_empty() && line.trim_start().starts_with(prefix.as_str()) {
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(theme.comment),
            });
            return out;
        }

        // Tokenisation syntect depuis l'état de fin de la ligne précédente
        // (le cache, donc O(1) d'accès + O(longueur de ligne) de parse).
        let base = if in_frontmatter {
            theme.frontmatter
        } else {
            theme.foreground
        };
        let covered = line_idx < self.states.len();
        if covered {
            let fresh_start =
                line_idx == 0 || (line_idx == self.frontmatter_end && self.frontmatter_end > 0);
            let mut state = if fresh_start {
                self.initial_state(syn, line_idx)
            } else {
                self.states[line_idx - 1].clone()
            };
            // Pile de DÉBUT de ligne = pile de fin de la ligne précédente.
            let mut stack = if fresh_start {
                ScopeStack::new()
            } else {
                self.states[line_idx - 1].scopes.clone()
            };
            let with_nl = line_with_newline(rope, line_idx);
            let ops = state
                .parse
                .parse_line(&with_nl, &syn.set)
                .unwrap_or_default();
            let mut at = 0usize;
            let mut flags = LineFlags::default();
            for (off, op) in ops {
                let off = off.min(line.len());
                if off > at {
                    let style = classify(&stack, theme, base, in_frontmatter, &mut flags);
                    out.push_span(at, off, style);
                }
                let _ = stack.apply(&op);
                at = at.max(off);
            }
            if at < line.len() {
                let style = classify(&stack, theme, base, in_frontmatter, &mut flags);
                out.push_span(at, line.len(), style);
            }
            out.in_code_block = flags.in_code_block;
            out.is_blockquote = flags.is_blockquote;
            out.scale = flags.scale;
        } else {
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(base),
            });
        }

        // Fallbacks de lisibilité (pas de rendu riche : archi ligne-par-ligne) :
        //  - frontmatter : on distingue la clé de la valeur ;
        //  - tableaux Typst : on souligne `#table(` pour repérer les blocs.
        if in_frontmatter {
            apply_yaml_keys(&mut out, &line, theme);
        } else {
            apply_typst_tables(&mut out, &line, theme);
            apply_typst_headings(&mut out, &line, theme);
        }

        // Overlays O(longueur de ligne) : wikilinks, #tags.
        apply_overlays(
            &mut out,
            &line,
            theme,
            cfg,
            &mut wikilink_valid,
            in_frontmatter,
        );
        out
    }
}

/// Flags de ligne accumulés pendant la classification des tronçons.
struct LineFlags {
    in_code_block: bool,
    is_blockquote: bool,
    scale: f32,
}

impl Default for LineFlags {
    fn default() -> Self {
        Self {
            in_code_block: false,
            is_blockquote: false,
            scale: 1.0,
        }
    }
}

impl LineSpans {
    fn push_span(&mut self, start: usize, end: usize, style: SpanStyle) {
        if start >= end {
            return;
        }
        // Fusion avec le tronçon précédent si style identique (moins de
        // sections dans le LayoutJob).
        if let Some(last) = self.spans.last_mut() {
            if last.end == start && last.style == style {
                last.end = end;
                return;
            }
        }
        self.spans.push(Span { start, end, style });
    }
}

/// La pile de scopes → un style. Priorité : code block > code inline >
/// titre > gras/italique > citation > lien > ponctuation > base.
fn classify(
    stack: &ScopeStack,
    theme: &Theme,
    base: egui::Color32,
    in_frontmatter: bool,
    flags: &mut LineFlags,
) -> SpanStyle {
    SCOPES.with(|sc| {
        let mut style = SpanStyle::plain(base);
        let mut heading: Option<usize> = None;
        let (mut raw_block, mut raw_inline, mut bold, mut italic) = (false, false, false, false);
        let (mut quote, mut punct, mut link) = (false, false, false);
        let (mut yaml_key, mut yaml_string) = (false, false);

        for scope in stack.as_slice() {
            if sc.raw_block.is_prefix_of(*scope) {
                raw_block = true;
            } else if sc.raw_inline.is_prefix_of(*scope) {
                raw_inline = true;
            } else if sc.heading.is_prefix_of(*scope) {
                let lvl = sc
                    .headings
                    .iter()
                    .position(|h| h.is_prefix_of(*scope))
                    .unwrap_or(0);
                heading = Some(heading.map_or(lvl, |h: usize| h.min(lvl)));
            } else if sc.bold.is_prefix_of(*scope) {
                bold = true;
            } else if sc.italic.is_prefix_of(*scope) {
                italic = true;
            } else if sc.quote.is_prefix_of(*scope) {
                quote = true;
            } else if sc.punctuation.is_prefix_of(*scope) {
                punct = true;
            } else if sc.link.is_prefix_of(*scope) {
                link = true;
            } else if sc.yaml_key.is_prefix_of(*scope) {
                yaml_key = true;
            } else if sc.yaml_string.is_prefix_of(*scope) {
                yaml_string = true;
            }
        }

        if raw_block {
            flags.in_code_block = true;
            style.color = theme.code_block_fg;
            return style;
        }
        if quote {
            flags.is_blockquote = true;
            style.color = theme.blockquote;
        }
        if raw_inline {
            style.color = theme.inline_code_fg;
            style.background = Some(theme.inline_code_bg);
            return style;
        }
        if let Some(lvl) = heading {
            let lvl = lvl.min(5);
            flags.scale = theme.heading_scales[lvl];
            style.scale = theme.heading_scales[lvl];
            style.color = if punct {
                theme.markers
            } else {
                theme.headings[lvl]
            };
            style.bold = true;
            return style;
        }
        if in_frontmatter {
            if yaml_key {
                style.color = theme.frontmatter;
                style.bold = true;
            } else if yaml_string {
                style.color = theme.foreground;
            }
        }
        if bold {
            style.bold = true;
            if punct {
                style.color = theme.markers;
            }
        }
        if italic {
            style.italics = true;
            if punct {
                style.color = theme.markers;
            }
        }
        if link && !punct {
            style.color = theme.wikilink_valid;
            style.underline = true;
        }
        if punct && !bold && !italic {
            style.color = theme.markers;
        }
        style
    })
}

/// Tableaux Typst (fallback) : repère visuellement les débuts de bloc
/// `#table(` pour garder le brut lisible. Le rendu riche reste volontairement
/// ligne-par-ligne.
fn apply_typst_tables(out: &mut LineSpans, line: &str, theme: &Theme) {
    if out.in_code_block {
        return;
    }
    let trimmed = line.trim_start();
    if !trimmed.starts_with("#table(") && !trimmed.starts_with("#figure(") {
        return;
    }
    let offset = line.len() - trimmed.len();
    override_range(out, offset, offset + 1, SpanStyle::plain(theme.markers));
    override_range(
        out,
        offset + 1,
        offset + trimmed.len().min(6),
        SpanStyle {
            bold: true,
            ..SpanStyle::plain(theme.tag)
        },
    );
}

/// Titres Typst de niveau 1..=5. Les `=` restent visibles en markers, le
/// reste de la ligne reçoit la couleur du niveau.
fn apply_typst_headings(out: &mut LineSpans, line: &str, theme: &Theme) {
    if out.in_code_block {
        return;
    }
    let indent = line.len() - line.trim_start().len();
    let t = &line[indent..];
    let level = t.bytes().take_while(|&b| b == b'=').count();
    if !(1..=5).contains(&level) {
        return;
    }
    let rest = &t[level..];
    if !rest.starts_with(' ') {
        return;
    }
    let color = theme.headings[level - 1];
    let scale = theme.heading_scales[level - 1];
    override_range(out, indent, indent + level, SpanStyle::plain(theme.markers));
    override_range(
        out,
        indent + level,
        line.len(),
        SpanStyle {
            color,
            bold: true,
            scale,
            ..SpanStyle::plain(color)
        },
    );
    out.scale = scale;
}

/// Frontmatter YAML (fallback) : colore la clé `clé:` en accent (theme.tag) et le
/// `:` en markers, pour rendre lisible une fiche à 40+ champs. La valeur garde la
/// couleur de base ; les [[wikilinks]] y restent cliquables (overlays ensuite).
/// L'affichage « propriétés » structuré + édition inline = session dédiée.
fn apply_yaml_keys(out: &mut LineSpans, line: &str, theme: &Theme) {
    let indent = line.len() - line.trim_start().len();
    let content = &line[indent..];
    let Some(colon) = content.find(':') else {
        return;
    };
    let key = &content[..colon];
    if key.is_empty() || key.contains(' ') || key.starts_with('-') {
        return;
    }
    override_range(out, indent, indent + colon, SpanStyle::plain(theme.tag));
    override_range(
        out,
        indent + colon,
        indent + colon + 1,
        SpanStyle::plain(theme.markers),
    );
}

/// Overlays regex par ligne visible : [[wikilinks]] et #tags.
fn apply_overlays(
    out: &mut LineSpans,
    line: &str,
    theme: &Theme,
    cfg: &Config,
    wikilink_valid: &mut impl FnMut(&str) -> bool,
    in_frontmatter: bool,
) {
    if out.in_code_block {
        return; // pas de wikilinks dans le code.
    }
    for (start, end, name) in wikilinks::find_in_line(line) {
        let valid = wikilink_valid(&name);
        let color = if valid {
            theme.wikilink_valid
        } else {
            theme.wikilink_orphan
        };
        let opacity = cfg.vomi.wikilink_bracket_opacity.min(100) as f32 / 100.0;
        let bracket = egui::Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            (255.0 * opacity) as u8,
        );
        // [[ et ]] quasi invisibles, le nom souligné dans la couleur du lien.
        override_range(out, start, start + 2, SpanStyle::plain(bracket));
        override_range(
            out,
            start + 2,
            end - 2,
            SpanStyle {
                underline: true,
                ..SpanStyle::plain(color)
            },
        );
        override_range(out, end - 2, end, SpanStyle::plain(bracket));
        out.wikilinks.push((start, end, name));
    }
    if !in_frontmatter {
        for (start, end) in find_tags(line) {
            override_range(out, start, end, SpanStyle::plain(theme.tag));
        }
    }
}

/// #tag inline : # suivi de lettres, précédé d'un espace ou en début de
/// ligne — mais PAS les expressions Typst `#let` / `#table` ni les titres `=`.
fn find_tags(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let at_start = i != 0 && bytes[i - 1].is_ascii_whitespace();
            let mut j = i + 1;
            while j < bytes.len() {
                let Some(c) = line[j..].chars().next() else {
                    break;
                };
                if c.is_alphanumeric() || c == '_' || c == '-' || c == '/' {
                    j += c.len_utf8();
                } else {
                    break;
                }
            }
            if at_start && j > i + 1 {
                out.push((i, j));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Remplace le style sur [start, end) en redécoupant les tronçons existants.
fn override_range(out: &mut LineSpans, start: usize, end: usize, style: SpanStyle) {
    if start >= end {
        return;
    }
    let mut next = Vec::with_capacity(out.spans.len() + 2);
    for span in out.spans.drain(..) {
        if span.end <= start || span.start >= end {
            next.push(span);
            continue;
        }
        if span.start < start {
            next.push(Span {
                start: span.start,
                end: start,
                style: span.style,
            });
        }
        if span.end > end {
            next.push(Span {
                start: end,
                end: span.end,
                style: span.style,
            });
        }
    }
    next.push(Span { start, end, style });
    next.sort_by_key(|s| s.start);
    out.spans = next;
}

// ---------------------------------------------------------------------------
// Helpers rope → lignes (O(log N) par accès, O(longueur de ligne) en copie).
// ---------------------------------------------------------------------------

pub fn line_with_newline(rope: &Rope, idx: usize) -> String {
    if idx >= rope.len_lines() {
        return "\n".into();
    }
    let mut s: String = rope.line(idx).to_string();
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

pub fn line_without_newline(rope: &Rope, idx: usize) -> String {
    if idx >= rope.len_lines() {
        return String::new();
    }
    let mut s: String = rope.line(idx).to_string();
    while s.ends_with('\n') || s.ends_with('\r') {
        s.pop();
    }
    s
}

/// Détecte la zone frontmatter : `---` en ligne 0, ou bloc Typst `/*` + `---`
/// en lignes 0/1. Fermeture `---` ou `...` plus bas, puis `*/` optionnel.
/// Retourne la fin EXCLUSIVE (ligne après la fermeture), 0 si pas de bloc.
pub fn detect_frontmatter(rope: &Rope) -> usize {
    let max = rope.len_lines().min(FRONTMATTER_MAX_LINES);
    if max < 2 {
        return 0;
    }
    let first = line_without_newline(rope, 0).trim_end().to_string();
    let mut start = 1usize;
    let comment_wrapped = first == "/*";
    if !comment_wrapped && first != "---" {
        return 0;
    }
    if comment_wrapped {
        if line_without_newline(rope, 1).trim_end() != "---" {
            return 0;
        }
        start = 2;
    }
    for i in start..max {
        let t = line_without_newline(rope, i).trim_end().to_string();
        if t == "---" || t == "..." {
            let mut end = i + 1;
            if comment_wrapped && end < max && line_without_newline(rope, end).trim_end() == "*/" {
                end += 1;
            }
            return end;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_detection() {
        let r = Rope::from_str("/*\n---\ngoal: 2000\ntags:\n  - scene\n---\n*/\n\n= Titre\n");
        assert_eq!(detect_frontmatter(&r), 7);
        let r = Rope::from_str("# Pas de frontmatter\n---\n");
        assert_eq!(detect_frontmatter(&r), 0);
        let r = Rope::from_str("");
        assert_eq!(detect_frontmatter(&r), 0);
    }

    #[test]
    fn tags_not_headings() {
        assert_eq!(find_tags("= Titre"), vec![]);
        assert_eq!(
            find_tags("du texte avec #neige et #froid/hiver"),
            vec![(14, 20), (24, 36)]
        );
        assert_eq!(find_tags("pas#de tag collé"), vec![]);
    }

    #[test]
    fn override_splits_spans() {
        let mut out = LineSpans::default();
        out.spans.push(Span {
            start: 0,
            end: 20,
            style: SpanStyle::plain(egui::Color32::WHITE),
        });
        override_range(&mut out, 5, 10, SpanStyle::plain(egui::Color32::RED));
        let ranges: Vec<(usize, usize)> = out.spans.iter().map(|s| (s.start, s.end)).collect();
        assert_eq!(ranges, vec![(0, 5), (5, 10), (10, 20)]);
    }

    /// Le cas du brief : scroller au milieu d'un frontmatter YAML long.
    /// L'état de la ligne 80 doit venir du cache, pas d'un parse à froid.
    #[test]
    fn parse_state_cache_covers_long_frontmatter() {
        let mut text = String::from("---\n");
        for i in 0..150 {
            text.push_str(&format!("cle_{i}: valeur {i}\n"));
        }
        text.push_str("---\ncorps du texte\n");
        let rope = Rope::from_str(&text);
        let syn = Syntaxes::load();
        let mut cache = HighlightCache {
            frontmatter_end: detect_frontmatter(&rope),
            version: 0,
            ..HighlightCache::default()
        };
        assert_eq!(cache.frontmatter_end, 152);
        assert!(cache.ensure(&rope, &syn, 153));
        assert_eq!(cache.states.len(), 153);
    }
}
