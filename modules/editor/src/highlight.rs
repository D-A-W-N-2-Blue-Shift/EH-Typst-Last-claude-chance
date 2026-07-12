// ============================================================================
// modules/editor/src/highlight.rs — coloration locale légère
//
// Objectif du sweep :
//   - retirer syntect et sa chaîne de dépendances ;
//   - garder une coloration lisible et stable ;
//   - préserver l'API attendue par viewport.rs / lib.rs.
//
// Le rendu reste volontairement simple :
//   - titres Typst `=` ;
//   - tableaux `#table(` / `#figure(` ;
//   - frontmatter YAML ;
//   - commentaires configurés ;
//   - overlays wikilinks / #tags.
// ============================================================================

use ropey::Rope;

use crate::config::{Config, Theme};
use crate::wikilinks;

/// Lignes traitées au maximum par frame pour rattraper le cache.
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

/// Le type de syntaxe n'est plus externalisé ; on garde un placeholder pour
/// préserver la signature attendue par le reste du module.
pub struct Syntaxes;

impl Syntaxes {
    pub fn load() -> Self {
        Self
    }
}

/// État minimal de ligne pour le cache.
#[derive(Clone, Copy, Default)]
struct LineState {
    in_code_block: bool,
}

/// Cache de coloration d'UN buffer vu par UNE fenêtre.
pub struct HighlightCache {
    /// states[i] = état à la fin de la ligne i. len = lignes déjà traitées.
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
    pub fn ensure(&mut self, rope: &Rope, _syn: &Syntaxes, target_line: usize) -> bool {
        let target = target_line.min(rope.len_lines());
        let mut budget = CATCHUP_BUDGET;
        while self.states.len() < target {
            if budget == 0 {
                return false;
            }
            budget -= 1;
            let i = self.states.len();
            let prev = self.states.last().copied().unwrap_or_default();
            let line = line_without_newline(rope, i);
            let trimmed = line.trim_start();
            let mut state = prev;
            if i == 0 || (i == self.frontmatter_end && self.frontmatter_end > 0) {
                state.in_code_block = false;
            }
            if is_code_fence(trimmed) {
                state.in_code_block = !state.in_code_block;
            }
            self.states.push(state);
        }
        true
    }

    /// Tronçons stylés d'une ligne.
    pub fn line_spans(
        &mut self,
        rope: &Rope,
        _syn: &Syntaxes,
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
        let trimmed = line.trim_start();
        let in_code_block = self
            .states
            .get(line_idx)
            .copied()
            .unwrap_or_default()
            .in_code_block;

        // Délimiteurs --- du frontmatter : couleur dédiée.
        if in_frontmatter && (line_idx == 0 || line_idx + 1 == self.frontmatter_end) {
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(theme.markers),
            });
            return out;
        }

        // Ligne de fence : on la rend lisible, le reste des lignes du bloc
        // est détecté par l'état cache.
        if is_code_fence(trimmed) {
            out.in_code_block = true;
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(theme.markers),
            });
            return out;
        }

        // Commentaire configuré : ligne entière en discret.
        let prefix = &cfg.vomi.comment_prefix;
        if !in_frontmatter && !prefix.is_empty() && trimmed.starts_with(prefix.as_str()) {
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(theme.comment),
            });
            return out;
        }

        let base = if in_frontmatter {
            theme.frontmatter
        } else {
            theme.foreground
        };

        if in_code_block {
            out.in_code_block = true;
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(theme.code_block_fg),
            });
            return out;
        }

        // Frontmatter YAML : la clé est distinguée, la valeur reste lisible.
        if in_frontmatter {
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(base),
            });
            apply_yaml_keys(&mut out, &line, theme);
        } else {
            out.spans.push(Span {
                start: 0,
                end: line.len(),
                style: SpanStyle::plain(base),
            });
            apply_typst_tables(&mut out, &line, theme);
            apply_typst_headings(&mut out, &line, theme);
            if trimmed.starts_with('>') {
                out.is_blockquote = true;
            }
        }

        // Overlays regex par ligne visible : wikilinks, #tags.
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

/// Tableaux Typst (fallback) : repère visuellement les débuts de bloc
/// `#table(` et `#figure(`.
fn apply_typst_tables(out: &mut LineSpans, line: &str, theme: &Theme) {
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

/// Frontmatter YAML : colore la clé `clé:` en accent et le `:` en markers.
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
        return;
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

/// #tag inline : # suivi de lettres, précédé d'un espace ou en début de ligne.
fn find_tags(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let at_start = i == 0 || bytes[i - 1].is_ascii_whitespace();
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

fn is_code_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
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
        assert_eq!(find_tags("#tag au début"), vec![(0, 4)]);
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
