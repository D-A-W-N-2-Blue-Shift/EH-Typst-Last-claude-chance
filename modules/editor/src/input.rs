// ============================================================================
// modules/editor/src/input.rs — Clavier, raccourcis, keybinds.ron
//
// - Keybinds : ~/.config/engram_hive/keybinds.ron (créé au premier
//   lancement avec les défauts commentés). Format "Ctrl+Shift+R".
//   "Ctrl" = COMMAND egui (Cmd sur mac, Ctrl ailleurs).
// - handle_input : appelé chaque frame DANS le viewport de la fenêtre
//   (chaque fenêtre OS reçoit ses propres événements). Traduit les
//   événements en transactions de buffer :
//     frappe → begin_txn(Typing) → insert → smart typography → snippet →
//     end_txn. La conversion typo rejoint la transaction de la frappe :
//     un Ctrl+Z annule tout.
// - Le popup d'auto-complétion a priorité sur les flèches/Entrée/Echap.
// - Toutes les mutations par-frame restent O(log N) + O(taille de l'édition).
// ============================================================================

use std::collections::HashMap;
use std::path::Path;

use crate::buffer::{SharedBufferExt, TxnKind};
use crate::config::Config;
use crate::snippets::SnippetSet;
use crate::typography::{self, Lang};
use crate::viewport::word_bounds;
use crate::wikilinks::{self, WikilinkIndex};
use crate::EditorWindow;
use engram_core::atomic_write;

/// Lecture synchrone du presse-papier (pour le « Coller » du menu contextuel ;
/// la frappe Ctrl+V passe, elle, par l'événement egui::Event::Paste). Toute
/// erreur (presse-papier vide/indisponible) → None, jamais de panique.
fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

// ===========================================================================
// Keybinds.
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub mods: egui::Modifiers,
    pub key: egui::Key,
}

pub struct Keybinds {
    map: HashMap<String, Chord>,
}

/// (action, chord par défaut) — tout est surchargable dans keybinds.ron.
const DEFAULTS: &[(&str, &str)] = &[
    ("editor.save", "Ctrl+S"),
    ("editor.reload", "Ctrl+Shift+R"),
    ("editor.search", "Ctrl+F"),
    ("editor.replace", "Ctrl+H"),
    ("editor.focus_mode", "F11"),
    ("editor.typewriter", "Ctrl+Alt+T"),
    ("editor.zoom_reset", "Ctrl+0"),
    ("editor.zoom_in", "Ctrl+="),
    ("editor.zoom_out", "Ctrl+-"),
    ("editor.undo", "Ctrl+Z"),
    ("editor.redo", "Ctrl+Shift+Z"),
    ("editor.select_all", "Ctrl+A"),
    ("editor.search_next", "F3"),
    ("editor.search_prev", "Shift+F3"),
    ("editor.new_view", "Ctrl+Shift+N"),
    ("editor.toc", "Ctrl+Shift+O"),
    ("editor.render", "Ctrl+Alt+R"),
    ("editor.open_kate", "Ctrl+Alt+K"),
    ("editor.open_okular", "Ctrl+Alt+O"),
    ("editor.open_render_dir", "Ctrl+Alt+D"),
    ("editor.goto_error", "Ctrl+Alt+E"),
    // Formatage Typst (autour de la sélection, ou du mot courant sinon).
    ("editor.bold", "Ctrl+B"),
    ("editor.italic", "Ctrl+I"),
    ("editor.code", "Ctrl+`"),
    ("editor.link", "Ctrl+K"),
    ("editor.wikilink", "Ctrl+Shift+K"),
];

impl Keybinds {
    /// Charge keybinds.ron (créé avec les défauts au premier lancement).
    pub fn load(config_dir: &Path) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let mut map = HashMap::new();
        for (action, chord) in DEFAULTS {
            match parse_chord(chord) {
                Some(c) => {
                    map.insert(action.to_string(), c);
                }
                None => errors.push(format!(
                    "Raccourci par défaut invalide ignoré : {action} = {chord:?}"
                )),
            }
        }
        let path = config_dir.join("keybinds.ron");
        if !path.exists() {
            let mut content = String::from(
                "// ============================================================================\n\
                 // keybinds.ron — Raccourcis clavier d'Engram_Hive\n\
                 // Format : \"Ctrl+Shift+R\", \"F11\", \"Alt+X\"…\n\
                 // Supprimer une ligne = revenir au défaut.\n\
                 // ============================================================================\n{\n",
            );
            for (action, chord) in DEFAULTS {
                content.push_str(&format!("    \"{action}\": \"{chord}\",\n"));
            }
            content.push_str("}\n");
            if let Err(e) = atomic_write(&path, content.as_bytes()) {
                errors.push(format!("Impossible d'écrire {} : {e}.", path.display()));
            }
            return (Self { map }, errors);
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|raw| {
                ron::from_str::<HashMap<String, String>>(&raw).map_err(|e| e.to_string())
            }) {
            Ok(user) => {
                for (action, chord) in user {
                    match parse_chord(&chord) {
                        Some(c) => {
                            map.insert(action, c);
                        }
                        None => errors.push(format!(
                            "keybinds.ron : '{chord}' n'est pas un raccourci pour {action}. \
                             J'attends du \"Ctrl+Shift+X\". Défaut conservé."
                        )),
                    }
                }
            }
            Err(e) => errors.push(format!(
                "keybinds.ron illisible ({e}). Les raccourcis par défaut s'appliquent."
            )),
        }
        (Self { map }, errors)
    }

    /// Le raccourci de `action` vient-il d'être pressé ? (consomme la touche)
    pub fn consume(&self, ctx: &egui::Context, action: &str) -> bool {
        let Some(chord) = self.map.get(action) else {
            return false;
        };
        ctx.input_mut(|i| i.consume_key(chord.mods, chord.key))
    }
}

impl Default for Keybinds {
    fn default() -> Self {
        let mut map = HashMap::new();
        for (action, chord) in DEFAULTS {
            if let Some(c) = parse_chord(chord) {
                map.insert(action.to_string(), c);
            }
        }
        Self { map }
    }
}

/// "Ctrl+Shift+R" → Chord. "Ctrl" = COMMAND (cross-platform egui).
fn parse_chord(s: &str) -> Option<Chord> {
    let mut mods = egui::Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        let part = part.trim();
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => mods = mods.plus(egui::Modifiers::COMMAND),
            "shift" => mods = mods.plus(egui::Modifiers::SHIFT),
            "alt" => mods = mods.plus(egui::Modifiers::ALT),
            // Symboles que egui::Key::from_name ne reconnaît pas par leur glyphe.
            "=" | "equals" => key = Some(egui::Key::Equals),
            "+" | "plus" => key = Some(egui::Key::Plus),
            "-" | "minus" => key = Some(egui::Key::Minus),
            _ => key = egui::Key::from_name(part),
        }
    }
    key.map(|key| Chord { mods, key })
}

// ===========================================================================
// Gestion des événements d'une fenêtre.
// ===========================================================================

/// Ce que lib.rs doit faire suite aux entrées de cette frame.
#[derive(Default)]
pub struct InputOutput {
    pub save: bool,
    pub reload: bool,
    pub toggle_focus: bool,
    pub new_view: bool,
    pub toggle_toc: bool,
    pub render: bool,
    pub open_kate: bool,
    pub open_okular: bool,
    pub open_render_dir: bool,
    pub goto_error: bool,
}

impl EditorWindow {
    /// Traite le clavier de CETTE fenêtre. Appelé dans son viewport.
    pub fn handle_input(
        &mut self,
        ctx: &egui::Context,
        cfg: &Config,
        keybinds: &Keybinds,
        snippets: &SnippetSet,
        index: &WikilinkIndex,
    ) -> InputOutput {
        let mut out = InputOutput::default();
        if self.confirm_close {
            return out; // le dialogue de fermeture a la main.
        }
        if self.table_dialog.is_some() {
            // Dialogue tableau modal : il capte le clavier (Entrée valide, Échap
            // annule). On ne touche pas au buffer tant qu'il est ouvert.
            return out;
        }

        // --- Assistance tableaux (§6) : intercepte Tab/Shift+Tab/Entrée
        // si le curseur est dans un bloc tableau Typst. Doit passer
        // AVANT les autres consommateurs de Tab/Entrée.
        if self.handle_table_assist(ctx) {
            return out;
        }

        // --- Raccourcis globaux (consommés avant tout le reste). ---
        // Redo avant Undo : Ctrl+Shift+Z doit gagner sur Ctrl+Z.
        if keybinds.consume(ctx, "editor.redo") {
            let c = self.buffer.write_buf().redo();
            if let Some(c) = c {
                self.set_cursor(c, false);
            }
        }
        if keybinds.consume(ctx, "editor.undo") {
            let c = self.buffer.write_buf().undo();
            if let Some(c) = c {
                self.set_cursor(c, false);
            }
        }
        if keybinds.consume(ctx, "editor.save") {
            out.save = true;
        }
        if keybinds.consume(ctx, "editor.reload") {
            out.reload = true;
        }
        if keybinds.consume(ctx, "editor.render") {
            out.render = true;
        }
        if keybinds.consume(ctx, "editor.open_kate") {
            out.open_kate = true;
        }
        if keybinds.consume(ctx, "editor.open_okular") {
            out.open_okular = true;
        }
        if keybinds.consume(ctx, "editor.open_render_dir") {
            out.open_render_dir = true;
        }
        if keybinds.consume(ctx, "editor.goto_error") {
            out.goto_error = true;
        }
        if keybinds.consume(ctx, "editor.focus_mode") {
            out.toggle_focus = true;
        }
        if keybinds.consume(ctx, "editor.typewriter") {
            self.typewriter = !self.typewriter;
        }
        if keybinds.consume(ctx, "editor.zoom_reset") {
            self.font_size = cfg.simple.font_size as f32;
        }
        // Zoom CLAVIER (parade fiable, par fenêtre) : keybinds.consume passe par
        // consume_key, donc l'événement est routé à la fenêtre qui a le focus
        // clavier — pas de synchro entre fenêtres (contrairement à la molette,
        // dont egui partage le zoom_delta entre viewports immédiats).
        if keybinds.consume(ctx, "editor.zoom_in") {
            self.font_size =
                (self.font_size * 1.1).clamp(cfg.vomi.zoom_min_pt, cfg.vomi.zoom_max_pt);
            self.last_input = std::time::Instant::now();
        }
        if keybinds.consume(ctx, "editor.zoom_out") {
            self.font_size =
                (self.font_size / 1.1).clamp(cfg.vomi.zoom_min_pt, cfg.vomi.zoom_max_pt);
            self.last_input = std::time::Instant::now();
        }
        if keybinds.consume(ctx, "editor.new_view") {
            out.new_view = true;
        }
        if keybinds.consume(ctx, "editor.toc") {
            out.toggle_toc = true;
        }
        if keybinds.consume(ctx, "editor.search") {
            self.search.open_search();
        }
        if keybinds.consume(ctx, "editor.replace") {
            self.search.open_replace();
        }
        if self.search.open {
            if keybinds.consume(ctx, "editor.search_next") {
                self.jump_to_match(true);
            }
            if keybinds.consume(ctx, "editor.search_prev") {
                self.jump_to_match(false);
            }
        }
        if keybinds.consume(ctx, "editor.select_all") {
            let len = self.buffer.read_buf().rope.len_chars();
            self.anchor = Some(0);
            self.cursor = len;
        }

        // Formatage Typst : entoure la sélection (ou le mot courant).
        if keybinds.consume(ctx, "editor.bold") {
            self.wrap_format("**", "**");
        }
        if keybinds.consume(ctx, "editor.italic") {
            self.wrap_format("*", "*");
        }
        if keybinds.consume(ctx, "editor.code") {
            self.wrap_format("`", "`");
        }
        if keybinds.consume(ctx, "editor.link") {
            self.insert_typst_link();
        }
        if keybinds.consume(ctx, "editor.wikilink") {
            self.wrap_format("[[", "]]");
        }

        // Echap : autocomplete > recherche > mode focus.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            if self.autocomplete.open {
                self.autocomplete.close();
            } else if self.search.open {
                self.search.close();
            } else if self.focus.active {
                out.toggle_focus = true;
            }
        }

        // Zoom PAR FENÊTRE (jamais ctx.set_zoom_factor qui est global) :
        // egui agrège Ctrl+Molette ET le pincement dans zoom_delta(). On ne
        // l'applique qu'à la fenêtre SOUS LE POINTEUR — c'est elle que
        // l'utilisateur vise quand il scrolle, et c'est plus robuste que le
        // focus OS (l'input peut être partagé entre viewports immédiats, et
        // la fenêtre survolée n'a pas forcément le focus clavier).
        let hovered = ctx.input(|i| i.pointer.hover_pos()).is_some();
        let raw_delta = ctx.input(|i| i.zoom_delta());
        if hovered && (raw_delta - 1.0).abs() > f32::EPSILON {
            // §8 — amortit le pas de zoom via cfg.vomi.zoom_scroll_step.
            // Delta natif d'egui = 1 + d ; on renvoie 1 + d * step. step = 1
            // reproduit le comportement d'avant, step = 0.5 (défaut brief)
            // rend Ctrl+Molette deux fois moins nerveux, step = 0.25 très doux.
            let step = cfg.vomi.zoom_scroll_step.clamp(0.05, 1.0);
            let dampened = 1.0 + (raw_delta - 1.0) * step;
            self.font_size =
                (self.font_size * dampened).clamp(cfg.vomi.zoom_min_pt, cfg.vomi.zoom_max_pt);
            self.last_input = std::time::Instant::now();
        }

        // Un widget egui (champ de recherche…) a le focus clavier :
        // le corps du texte ne mange pas ses événements. MAIS si aucun champ
        // de saisie ne doit être actif (recherche fermée) et qu'un widget
        // retient quand même le focus (bouton cliqué, popup, champ fermé),
        // on le RELÂCHE : c'est ce focus résiduel qui faisait qu'une fenêtre
        // n'acceptait plus la frappe jusqu'à un aller-retour vers le file tree.
        //
        // CRITIQUE : la mémoire de focus d'egui est GLOBALE au Context (partagée
        // par tous les viewports/fenêtres OS). Ne relâcher le focus QUE si CETTE
        // fenêtre éditeur est la fenêtre active. Sinon on arrachait, chaque
        // frame, le focus des champs d'AUTRES viewports (dialogue nouveau
        // projet, police du cockpit…) : la 1re lettre passait puis le champ
        // perdait le focus — c'était la cause du bug « saisie à une seule
        // lettre ». Voir tests/focus_surrender.rs.
        let viewport_focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        if viewport_focused && !self.search.open {
            let stuck = ctx.memory(|m| m.focused());
            if let Some(id) = stuck {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }
        if ctx.memory(|m| m.focused().is_some()) {
            return out;
        }

        // --- Popup d'auto-complétion : priorité sur flèches/Entrée/Tab. ---
        if self.autocomplete.open {
            let down =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
            let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
            let accept = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
            });
            let n = self.autocomplete.candidates.len();
            if n > 0 {
                if down {
                    self.autocomplete.selected = (self.autocomplete.selected + 1) % n;
                }
                if up {
                    self.autocomplete.selected = (self.autocomplete.selected + n - 1) % n;
                }
                if accept {
                    self.accept_completion();
                    self.after_edit(ctx, cfg, index);
                    return out;
                }
            }
        }

        // --- Événements texte / touches. ---
        let events = ctx.input(|i| i.events.clone());
        let mut edited = false;
        for event in events {
            match event {
                egui::Event::Text(t) => {
                    self.type_text(&t, cfg, snippets);
                    edited = true;
                }
                egui::Event::Paste(t) => {
                    self.replace_selection_with(&t);
                    edited = true;
                }
                egui::Event::Copy => {
                    if let Some(text) = self.selected_text() {
                        ctx.copy_text(text);
                    }
                }
                egui::Event::Cut => {
                    if let Some(text) = self.selected_text() {
                        ctx.copy_text(text);
                        self.replace_selection_with("");
                        edited = true;
                    }
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if self.handle_key(key, modifiers, cfg) {
                        edited = true;
                    }
                }
                _ => {}
            }
        }

        if edited {
            self.after_edit(ctx, cfg, index);
        }
        out
    }

    /// Recherche : sauter à l'occurrence suivante/précédente et la montrer.
    pub fn jump_to_match(&mut self, forward: bool) {
        if let Some((s, e)) = self.search.step(forward) {
            self.anchor = Some(s);
            self.cursor = e;
            self.scroll_cursor_into_view = true;
        }
    }

    /// Après toute édition clavier : curseur visible, blink remis à zéro,
    /// auto-complétion recalculée.
    fn after_edit(&mut self, ctx: &egui::Context, cfg: &Config, index: &WikilinkIndex) {
        self.scroll_cursor_into_view = true;
        self.last_input = std::time::Instant::now();
        self.refresh_autocomplete(cfg, index);
        ctx.request_repaint();
    }

    /// Le popup suit la frappe : ouvert dès [[ + min_chars, fuzzy sur
    /// l'index fourni par le core.
    pub fn refresh_autocomplete(&mut self, cfg: &Config, index: &WikilinkIndex) {
        if !cfg.simple.wikilink_autocomplete {
            return;
        }
        let buf = self.buffer.read_buf();
        match wikilinks::query_at_cursor(&buf.rope, self.cursor) {
            Some((start, q)) if q.chars().count() >= cfg.vomi.wikilink_autocomplete_min_chars => {
                let candidates = index.complete(&q, cfg.vomi.wikilink_autocomplete_max_results);
                if candidates.is_empty() {
                    self.autocomplete.close();
                } else {
                    self.autocomplete.open = true;
                    self.autocomplete.query_start = start;
                    if self.autocomplete.selected >= candidates.len() {
                        self.autocomplete.selected = 0;
                    }
                    self.autocomplete.candidates = candidates;
                }
            }
            _ => self.autocomplete.close(),
        }
    }

    /// Insère le candidat sélectionné : remplace la requête, ferme par ]].
    pub fn accept_completion(&mut self) {
        let Some(name) = self
            .autocomplete
            .candidates
            .get(self.autocomplete.selected)
            .cloned()
        else {
            return;
        };
        let start = self.autocomplete.query_start;
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        buf.delete(start, self.cursor);
        buf.insert(start, &format!("{name}]]"));
        let cursor = start + name.chars().count() + 2;
        buf.end_txn(cursor);
        buf.commit_txn();
        drop(buf);
        self.cursor = cursor;
        self.anchor = None;
        self.autocomplete.close();
    }

    // -----------------------------------------------------------------------
    // Frappe.
    // -----------------------------------------------------------------------

    /// Insertion de texte tapé (avec sélection remplacée, smart typography
    /// et snippets — le tout dans la transaction de la frappe).
    fn type_text(&mut self, text: &str, cfg: &Config, snippets: &SnippetSet) {
        if text.is_empty() || text.chars().all(|c| c.is_control()) {
            return;
        }

        // Auto-close — type-through : taper la fermeture juste devant une
        // fermeture déjà là ne la double pas, on passe par-dessus.
        if cfg.simple.auto_close_pairs && self.anchor.is_none() && text.chars().count() == 1 {
            if let Some(c) = text.chars().next() {
                if matches!(c, ')' | ']') {
                    let next = {
                        let buf = self.buffer.read_buf();
                        (self.cursor < buf.rope.len_chars()).then(|| buf.rope.char(self.cursor))
                    };
                    if next == Some(c) {
                        self.cursor += 1;
                        return;
                    }
                }
            }
        }

        let mut buf = self.buffer.write_buf();
        let lang = Lang::parse(
            self.stats
                .meta
                .language
                .as_deref()
                .unwrap_or(&cfg.simple.language),
        );
        let smart = self
            .stats
            .meta
            .smart_typography
            .unwrap_or(cfg.simple.smart_typography);

        if let Some((s, e)) = self.selection() {
            // Remplacement de sélection : transaction isolée (pas de
            // coalescence avec la frappe précédente).
            buf.begin_txn(TxnKind::Other, self.cursor);
            buf.delete(s, e);
            self.cursor = s;
            self.anchor = None;
        } else {
            buf.begin_txn(TxnKind::Typing, self.cursor);
        }

        buf.insert(self.cursor, text);
        self.cursor += text.chars().count();

        // Smart typography : dans la MÊME transaction (un seul Ctrl+Z).
        if smart && text.chars().count() == 1 {
            if let Some(c) = text.chars().next() {
                if let Some(nc) =
                    typography::after_insert(&mut buf, self.cursor, c, lang, &cfg.vomi)
                {
                    self.cursor = nc;
                }
            }
        }

        // ;table : ouvre le dialogue d'insertion de tableau (pas un snippet
        // texte). Détecté avant les snippets génériques. Le token est retiré, la
        // transaction close, et le dialogue prend le relais (insertion à la
        // validation).
        if !cfg.simple.snippet_prefix.is_empty() {
            let trigger = format!("{}table", cfg.simple.snippet_prefix);
            if let Some(start) = token_eq(&buf.rope, self.cursor, &trigger) {
                buf.delete(start, self.cursor);
                self.cursor = start;
                buf.end_txn(self.cursor);
                drop(buf);
                self.table_dialog = Some(crate::table_dialog::TableDialogState::default());
                self.last_input = std::time::Instant::now();
                return;
            }
        }

        // Snippets : ;sce → template, même transaction.
        if let Some((start, content)) =
            snippets.expansion_at(&buf.rope, self.cursor, &cfg.simple.snippet_prefix)
        {
            buf.delete(start, self.cursor);
            buf.insert(start, &content);
            self.cursor = start + content.chars().count();
        }

        // Auto-close des paires ( ) et [ ] (les guillemets sont gérés par la
        // smart typography). Même transaction → un seul Ctrl+Z annule la
        // frappe ET la fermeture auto. On n'auto-ferme que si le caractère
        // suivant ne colle pas à un mot (évite de casser « (mot »).
        if cfg.simple.auto_close_pairs && text.chars().count() == 1 {
            let close = match text.chars().next() {
                Some('(') => Some(')'),
                Some('[') => Some(']'),
                _ => None,
            };
            if let Some(close) = close {
                let next = (self.cursor < buf.rope.len_chars()).then(|| buf.rope.char(self.cursor));
                let ok = next.is_none_or(|n| !n.is_alphanumeric());
                if ok {
                    buf.insert(self.cursor, &close.to_string());
                    // le curseur reste AVANT la fermeture (entre les deux).
                }
            }
        }

        buf.end_txn(self.cursor);
    }

    /// Insère le Typst d'un tableau à la position du curseur (transaction
    /// isolée) et place le curseur dans la première cellule (`cursor_offset`
    /// chars dans le bloc). Démarre sur une ligne propre si on n'y est pas déjà.
    pub(crate) fn insert_table(&mut self, typst: &str, cursor_offset: usize) {
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        if let Some((s, e)) = self.selection() {
            buf.delete(s, e);
            self.cursor = s;
            self.anchor = None;
        }
        let need_nl = self.cursor > 0 && buf.rope.char(self.cursor - 1) != '\n';
        let lead = if need_nl { 1 } else { 0 };
        let text = if need_nl {
            format!("\n{typst}")
        } else {
            typst.to_string()
        };
        buf.insert(self.cursor, &text);
        self.cursor += lead + cursor_offset;
        self.anchor = None;
        buf.end_txn(self.cursor);
        buf.commit_txn();
        self.scroll_cursor_into_view = true;
        self.last_input = std::time::Instant::now();
    }

    /// Colle/remplace la sélection par `text` (transaction isolée).
    fn replace_selection_with(&mut self, text: &str) {
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        if let Some((s, e)) = self.selection() {
            buf.delete(s, e);
            self.cursor = s;
            self.anchor = None;
        }
        buf.insert(self.cursor, text);
        self.cursor += text.chars().count();
        buf.end_txn(self.cursor);
        buf.commit_txn();
    }

    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        if a == self.cursor {
            return None;
        }
        Some((a.min(self.cursor), a.max(self.cursor)))
    }

    /// Entoure la sélection (ou le mot courant si aucune) de `prefix`/`suffix`.
    /// Une seule transaction (un Ctrl+Z annule tout).
    pub(crate) fn wrap_format(&mut self, prefix: &str, suffix: &str) {
        let (s, e) = self.selection().unwrap_or_else(|| {
            let buf = self.buffer.read_buf();
            word_bounds(&buf.rope, self.cursor)
        });
        let plen = prefix.chars().count();
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        // suffixe d'abord (indice plus haut) pour que `s` reste valide.
        buf.insert(e, suffix);
        buf.insert(s, prefix);
        buf.end_txn(self.cursor);
        buf.commit_txn();
        drop(buf);
        if prefix == "[" && suffix == "]()" {
            self.cursor = e + plen + 2; // legacy de lien gardé pour compat.
            self.anchor = None;
        } else {
            // Sélectionne le texte entouré (curseur après le contenu).
            self.anchor = Some(s + plen);
            self.cursor = e + plen;
        }
        self.scroll_cursor_into_view = true;
        self.last_input = std::time::Instant::now();
    }

    pub fn selected_text(&self) -> Option<String> {
        let (s, e) = self.selection()?;
        let buf = self.buffer.read_buf();
        Some(buf.rope.slice(s..e).to_string())
    }

    /// Applique une action du menu contextuel (clic droit). Exactement les
    /// mêmes effets que les hotkeys correspondantes. Retourne true si le
    /// buffer a été modifié (le copier ne modifie rien).
    pub(crate) fn apply_edit_action(
        &mut self,
        action: crate::viewport::EditAction,
        ctx: &egui::Context,
    ) -> bool {
        use crate::viewport::EditAction as A;
        match action {
            A::Bold => self.wrap_format("**", "**"),
            A::Italic => self.wrap_format("*", "*"),
            A::Code => self.wrap_format("`", "`"),
            A::Link => self.insert_typst_link(),
            A::Wikilink => self.wrap_format("[[", "]]"),
            A::Copy => {
                if let Some(text) = self.selected_text() {
                    ctx.copy_text(text);
                }
                return false;
            }
            A::Cut => {
                let Some(text) = self.selected_text() else {
                    return false;
                };
                ctx.copy_text(text);
                self.replace_selection_with("");
            }
            A::Paste => {
                let Some(text) = read_clipboard() else {
                    return false;
                };
                self.replace_selection_with(&text);
            }
            A::Toc => {
                self.toc.toggle();
                return false;
            }
            A::InsertTable => {
                self.table_dialog = Some(crate::table_dialog::TableDialogState::default());
                return false;
            }
        }
        self.scroll_cursor_into_view = true;
        self.last_input = std::time::Instant::now();
        true
    }

    /// Insère `#link("url")[texte]` autour de la sélection ou du mot courant.
    /// Le curseur retombe dans les guillemets pour saisir l'URL.
    fn insert_typst_link(&mut self) {
        let (s, e) = self.selection().unwrap_or_else(|| {
            let buf = self.buffer.read_buf();
            word_bounds(&buf.rope, self.cursor)
        });
        let link_text = {
            let buf = self.buffer.read_buf();
            buf.rope.slice(s..e).to_string()
        };
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        buf.delete(s, e);
        let inserted = format!("#link(\"\")[{link_text}]");
        buf.insert(s, &inserted);
        buf.end_txn(s + "#link(\"".chars().count());
        buf.commit_txn();
        drop(buf);
        self.anchor = None;
        self.cursor = s + "#link(\"".chars().count();
        self.scroll_cursor_into_view = true;
        self.last_input = std::time::Instant::now();
    }

    /// Intercepte Tab / Shift+Tab / Entrée si le curseur est dans un bloc
    /// tableau. Retourne true si l'événement a été traité (touche consommée,
    /// le caller doit court-circuiter le reste de handle_input pour la frame).
    fn handle_table_assist(&mut self, ctx: &egui::Context) -> bool {
        use crate::table_edit_assist as ta;
        // Si le curseur est en sélection, on laisse le comportement par défaut
        // (Tab indenterait normalement le bloc) — pas d'assistance tableau.
        if self.anchor.is_some_and(|a| a != self.cursor) {
            return false;
        }
        // Peek (lecture seule) : on ne veut PAS consommer si on n'est pas dans
        // un tableau ; sinon Tab serait avalé partout dans l'éditeur.
        let (tab_pressed, shift_tab_pressed, enter_pressed) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Tab)
                    && !i.modifiers.shift
                    && !i.modifiers.command
                    && !i.modifiers.alt,
                i.key_pressed(egui::Key::Tab)
                    && i.modifiers.shift
                    && !i.modifiers.command
                    && !i.modifiers.alt,
                i.key_pressed(egui::Key::Enter) && !i.modifiers.any(),
            )
        });
        if !tab_pressed && !shift_tab_pressed && !enter_pressed {
            return false;
        }
        let rope = self.buffer.read_buf().rope.clone();
        let edit = if tab_pressed {
            ta::handle_tab(&rope, self.cursor)
        } else if shift_tab_pressed {
            ta::handle_shift_tab(&rope, self.cursor)
        } else {
            ta::handle_enter(&rope, self.cursor)
        };
        let Some(edit) = edit else { return false };

        // À ce stade on est SÛR d'être dans un tableau et d'avoir une action :
        // on consomme l'événement avant d'appliquer.
        let mods = if shift_tab_pressed {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        let key = if enter_pressed {
            egui::Key::Enter
        } else {
            egui::Key::Tab
        };
        let _ = ctx.input_mut(|i| i.consume_key(mods, key));

        match edit {
            ta::TableEdit::MoveCursor(c) => {
                self.set_cursor(c, false);
            }
            ta::TableEdit::Insert {
                at,
                text,
                new_cursor,
            } => {
                let mut buf = self.buffer.write_buf();
                buf.begin_txn(TxnKind::Other, self.cursor);
                buf.insert(at, &text);
                drop(buf);
                self.set_cursor(new_cursor, false);
            }
        }
        true
    }

    fn set_cursor(&mut self, c: usize, select: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = c;
        self.desired_x = None;
        self.scroll_cursor_into_view = true;
        self.last_input = std::time::Instant::now();
    }

    // -----------------------------------------------------------------------
    // Touches non-texte. Retourne true si le buffer a été modifié.
    // -----------------------------------------------------------------------

    fn handle_key(&mut self, key: egui::Key, mods: egui::Modifiers, cfg: &Config) -> bool {
        use egui::Key as K;
        let len = { self.buffer.read_buf().rope.len_chars() };
        match key {
            K::ArrowLeft => {
                let c = if mods.command {
                    self.prev_word_boundary()
                } else if let (Some((s, _)), false) = (self.selection(), mods.shift) {
                    s // une flèche sans Shift replie la sélection.
                } else {
                    self.cursor.saturating_sub(1)
                };
                self.set_cursor(c, mods.shift);
                false
            }
            K::ArrowRight => {
                let c = if mods.command {
                    self.next_word_boundary()
                } else if let (Some((_, e)), false) = (self.selection(), mods.shift) {
                    e
                } else {
                    (self.cursor + 1).min(len)
                };
                self.set_cursor(c, mods.shift);
                false
            }
            K::ArrowUp | K::ArrowDown => {
                // Déplacement VISUEL (lignes wrappées) : résolu au rendu,
                // où les galleys existent. On note l'intention.
                if mods.shift && self.anchor.is_none() {
                    self.anchor = Some(self.cursor);
                } else if !mods.shift {
                    self.anchor = None;
                }
                self.pending_vmove = Some(if key == K::ArrowUp { -1 } else { 1 });
                self.last_input = std::time::Instant::now();
                false
            }
            K::PageUp | K::PageDown => {
                if mods.shift && self.anchor.is_none() {
                    self.anchor = Some(self.cursor);
                } else if !mods.shift {
                    self.anchor = None;
                }
                let row_h = self.font_size * cfg.vomi.line_height;
                let rows = ((self.viewport_h / row_h).floor() as i32 - 1).max(1);
                self.pending_vmove = Some(if key == K::PageUp { -rows } else { rows });
                self.last_input = std::time::Instant::now();
                false
            }
            K::Home => {
                let c = if mods.command {
                    0
                } else {
                    let buf = self.buffer.read_buf();
                    let line = buf.rope.char_to_line(self.cursor.min(len));
                    buf.rope.line_to_char(line)
                };
                self.set_cursor(c, mods.shift);
                false
            }
            K::End => {
                let c = if mods.command {
                    len
                } else {
                    let buf = self.buffer.read_buf();
                    let line = buf.rope.char_to_line(self.cursor.min(len));
                    let start = buf.rope.line_to_char(line);
                    let chars = buf.rope.line(line).len_chars();
                    let nl = if line + 1 < buf.rope.len_lines() {
                        1
                    } else {
                        0
                    };
                    start + chars - nl
                };
                self.set_cursor(c, mods.shift);
                false
            }
            K::Backspace => {
                let mut buf = self.buffer.write_buf();
                if let Some((s, e)) = self.selection() {
                    buf.begin_txn(TxnKind::Other, self.cursor);
                    buf.delete(s, e);
                    buf.end_txn(s);
                    self.cursor = s;
                    self.anchor = None;
                } else if self.cursor > 0 {
                    let from = if mods.command {
                        drop(buf);
                        let f = self.prev_word_boundary();
                        buf = self.buffer.write_buf();
                        f
                    } else {
                        self.cursor - 1
                    };
                    buf.begin_txn(TxnKind::Deleting, self.cursor);
                    buf.delete(from, self.cursor);
                    buf.end_txn(from);
                    self.cursor = from;
                } else {
                    return false;
                }
                true
            }
            K::Delete => {
                let mut buf = self.buffer.write_buf();
                if let Some((s, e)) = self.selection() {
                    buf.begin_txn(TxnKind::Other, self.cursor);
                    buf.delete(s, e);
                    buf.end_txn(s);
                    self.cursor = s;
                    self.anchor = None;
                } else if self.cursor < len {
                    let to = if mods.command {
                        drop(buf);
                        let t = self.next_word_boundary();
                        buf = self.buffer.write_buf();
                        t
                    } else {
                        self.cursor + 1
                    };
                    buf.begin_txn(TxnKind::Deleting, self.cursor);
                    buf.delete(self.cursor, to);
                    buf.end_txn(self.cursor);
                } else {
                    return false;
                }
                true
            }
            K::Enter => {
                self.insert_newline_with_list_continuation();
                true
            }
            K::Tab => {
                if mods.shift {
                    self.dedent();
                } else {
                    self.indent(cfg);
                }
                true
            }
            _ => false,
        }
    }

    fn prev_word_boundary(&self) -> usize {
        let buf = self.buffer.read_buf();
        let rope = &buf.rope;
        let mut c = self.cursor.min(rope.len_chars());
        while c > 0 && !rope.char(c - 1).is_alphanumeric() {
            c -= 1;
        }
        if c == 0 {
            return 0;
        }
        word_bounds(rope, c - 1).0
    }

    fn next_word_boundary(&self) -> usize {
        let buf = self.buffer.read_buf();
        let rope = &buf.rope;
        let len = rope.len_chars();
        let mut c = self.cursor.min(len);
        while c < len && !rope.char(c).is_alphanumeric() {
            c += 1;
        }
        if c >= len {
            return len;
        }
        word_bounds(rope, c).1
    }

    /// Entrée : nouvelle ligne + auto-continuation de liste.
    /// `- item` + Entrée → nouvelle ligne `- `. Ligne `- ` vide + Entrée →
    /// on sort de la liste (le marqueur s'efface).
    fn insert_newline_with_list_continuation(&mut self) {
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        if let Some((s, e)) = self.selection() {
            buf.delete(s, e);
            self.cursor = s;
            self.anchor = None;
        }
        let line_idx = buf.rope.char_to_line(self.cursor.min(buf.rope.len_chars()));
        let line_start = buf.rope.line_to_char(line_idx);
        let before: String = buf.rope.slice(line_start..self.cursor).to_string();
        let marker = list_marker(&before);
        match marker {
            Some(m) if before.trim_start() == m.trim_start() => {
                // Marqueur seul sur la ligne : on sort de la liste.
                let del_from = line_start;
                buf.delete(del_from, self.cursor);
                buf.insert(del_from, "\n");
                self.cursor = del_from + 1;
            }
            Some(m) => {
                let ins = format!("\n{m}");
                buf.insert(self.cursor, &ins);
                self.cursor += ins.chars().count();
            }
            None => {
                buf.insert(self.cursor, "\n");
                self.cursor += 1;
            }
        }
        buf.end_txn(self.cursor);
        buf.commit_txn();
    }

    /// Tab : 2 espaces (jamais de tab). Sélection multi-lignes → indente
    /// chaque ligne (une transaction).
    fn indent(&mut self, _cfg: &Config) {
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        match self.selection() {
            Some((s, e)) if buf.rope.char_to_line(s) != buf.rope.char_to_line(e) => {
                let first = buf.rope.char_to_line(s);
                let last = buf.rope.char_to_line(e);
                for line in (first..=last).rev() {
                    let at = buf.rope.line_to_char(line);
                    buf.insert(at, "  ");
                }
                let lines = last - first + 1;
                self.cursor = e + lines * 2;
                self.anchor = Some(s);
            }
            _ => {
                if let Some((s, e)) = self.selection() {
                    buf.delete(s, e);
                    self.cursor = s;
                    self.anchor = None;
                }
                buf.insert(self.cursor, "  ");
                self.cursor += 2;
            }
        }
        buf.end_txn(self.cursor);
        buf.commit_txn();
    }

    /// Shift+Tab : retire jusqu'à 2 espaces en tête de ligne(s).
    fn dedent(&mut self) {
        let mut buf = self.buffer.write_buf();
        buf.begin_txn(TxnKind::Other, self.cursor);
        let (s, e) = self.selection().unwrap_or((self.cursor, self.cursor));
        let first = buf.rope.char_to_line(s.min(buf.rope.len_chars()));
        let last = buf.rope.char_to_line(e.min(buf.rope.len_chars()));
        let mut removed_total = 0;
        for line in (first..=last).rev() {
            let at = buf.rope.line_to_char(line);
            let text: String = buf.rope.line(line).chars().take(2).collect();
            let n = text.chars().take_while(|c| *c == ' ').count().min(2);
            if n > 0 {
                buf.delete(at, at + n);
                removed_total += n;
            }
        }
        self.cursor = self.cursor.saturating_sub(removed_total.min(2));
        if let Some(a) = self.anchor {
            self.anchor = Some(a.saturating_sub(removed_total));
        }
        buf.end_txn(self.cursor);
        buf.commit_txn();
    }
}

/// Le marqueur de liste de la ligne (indentation incluse) : "- ", "* ",
/// "+ ", "> ", "12. " — ou None.
/// Le token courant (du dernier blanc de la ligne au curseur) est-il exactement
/// `trigger` ? Retourne le char de départ du token. Sert au déclencheur ;table,
/// calqué sur la détection de snippets.
fn token_eq(rope: &ropey::Rope, cursor: usize, trigger: &str) -> Option<usize> {
    let line_idx = rope.char_to_line(cursor.min(rope.len_chars()));
    let line_start = rope.line_to_char(line_idx);
    let before: String = rope.slice(line_start..cursor).to_string();
    let token_start_byte = before
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    let token = &before[token_start_byte..];
    if token == trigger {
        Some(cursor - token.chars().count())
    } else {
        None
    }
}

fn list_marker(line_before_cursor: &str) -> Option<String> {
    let indent: String = line_before_cursor
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let rest = &line_before_cursor[indent.len()..];
    if let Some(r) = rest
        .strip_prefix("- ")
        .map(|_| "- ")
        .or_else(|| rest.strip_prefix("* ").map(|_| "* "))
        .or_else(|| rest.strip_prefix("+ ").map(|_| "+ "))
        .or_else(|| rest.strip_prefix("> ").map(|_| "> "))
    {
        return Some(format!("{indent}{r}"));
    }
    // Liste numérotée : "12. " → marqueur suivant "13. ".
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        if let Some(after) = rest[digits.len()..].strip_prefix(". ") {
            let _ = after;
            let n: u64 = digits.parse().ok()?;
            return Some(format!("{indent}{}. ", n + 1));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_parsing() -> Result<(), Box<dyn std::error::Error>> {
        let c = parse_chord("Ctrl+Shift+R").ok_or("Ctrl+Shift+R doit parser")?;
        assert_eq!(c.key, egui::Key::R);
        assert!(c.mods.command && c.mods.shift && !c.mods.alt);
        let c = parse_chord("F11").ok_or("F11 doit parser")?;
        assert_eq!(c.key, egui::Key::F11);
        assert_eq!(c.mods, egui::Modifiers::NONE);
        let c = parse_chord("Ctrl+0").ok_or("Ctrl+0 doit parser")?;
        assert_eq!(c.key, egui::Key::Num0);
        assert!(parse_chord("Ctrl+Patate").is_none());
        Ok(())
    }

    #[test]
    fn list_markers() {
        assert_eq!(list_marker("- item"), Some("- ".into()));
        assert_eq!(list_marker("  - sous-item"), Some("  - ".into()));
        assert_eq!(list_marker("> citation"), Some("> ".into()));
        assert_eq!(list_marker("3. troisième"), Some("4. ".into()));
        assert_eq!(list_marker("du texte - tiret"), None);
        assert_eq!(list_marker("- "), Some("- ".into()));
    }
}
