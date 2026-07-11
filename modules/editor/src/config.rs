// ============================================================================
// modules/editor/src/config.rs — Chargement de la config double RON
//
// Config simple par module + config experte UNIFIÉE :
//   - section "editor" de ~/.config/engram_hive/engram.ron : simple,
//     ~10 valeurs essentielles
//   - section "editor_expert" de ~/.config/engram_hive/engram.ron : expert,
//     optionnel
//
// Règle : commenter une ligne dans un RON = revenir à la valeur par défaut
// (tous les champs sont #[serde(default)] via #[serde(default)] sur la struct).
//
// Les couleurs par défaut reprennent la palette OLED noir + rose néon du core
// (theme.ron). Chaque couleur est surchargeable dans la section "editor"
// (format "#rrggbb").
// ============================================================================

use std::path::{Path, PathBuf};

/// Config simple, user-facing (section `editor` de `engram.ron`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Editor {
    /// Police de prose. Cherchée dans ~/.config/engram_hive/fonts/ puis
    /// dans les polices système. Introuvable = défaut egui + log.
    pub font_editor: String,
    pub font_size: u32,
    /// Largeur de colonne en caractères approximatifs (0 = pleine largeur).
    pub column_width: u32,
    pub typewriter_mode: bool,
    pub smart_typography: bool,
    /// "fr" ou "en" — pilote guillemets et espaces insécables.
    pub language: String,
    pub wikilink_autocomplete: bool,
    pub snippet_prefix: String,
    pub line_numbers: bool,
    pub highlight_current_line: bool,
    pub autosave_minutes: u64,
    /// Auto-fermeture des paires ( ) et [ ] (les guillemets restent gérés
    /// par la smart typography).
    pub auto_close_pairs: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            font_editor: "Georgia".into(),
            font_size: 15,
            column_width: 72,
            typewriter_mode: false,
            smart_typography: true,
            language: "fr".into(),
            wikilink_autocomplete: true,
            snippet_prefix: ";".into(),
            line_numbers: false,
            highlight_current_line: true,
            autosave_minutes: 5,
            auto_close_pairs: true,
        }
    }
}

/// Config experte, optionnelle (section `editor_expert` de `engram.ron`).
/// Les couleurs vides ("") = couleur par défaut du thème OLED rose.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EditorVomi {
    // --- Curseur ---
    pub cursor_thickness: f32,
    pub cursor_blink_ms: u64,
    pub cursor_color: String,
    // --- Mise en page ---
    pub line_height: f32,
    pub paragraph_spacing: f32,
    pub editor_padding: f32,
    // --- Coloration ---
    /// Lignes parsées en plus autour du viewport (marge de confort).
    pub syntax_highlight_buffer_lines: usize,
    pub heading_h1_scale: f32,
    pub heading_h2_scale: f32,
    pub heading_h3_scale: f32,
    pub heading_h4_scale: f32,
    pub heading_h5_scale: f32,
    pub heading_h6_scale: f32,
    pub heading_h1: String,
    pub heading_h2: String,
    pub heading_h3: String,
    pub heading_h4: String,
    pub heading_h5: String,
    pub heading_h6: String,
    pub foreground: String,
    pub inline_code_foreground: String,
    pub inline_code_background: String,
    pub code_block_foreground: String,
    pub code_block_background: String,
    pub blockquote_color: String,
    pub blockquote_border: String,
    pub wikilink_valid: String,
    pub wikilink_orphan: String,
    pub tag_color: String,
    pub frontmatter_color: String,
    pub comment_prefix: String,
    pub comment_color: String,
    pub markers_color: String,
    pub current_line_background: String,
    pub selection_background: String,
    pub search_highlight_background: String,
    pub search_current_background: String,
    // --- Zoom ---
    pub zoom_min_pt: f32,
    pub zoom_max_pt: f32,
    /// §8 — Sensibilité de Ctrl+Molette. Facteur multiplicatif appliqué au
    /// delta remonté par egui : 1.0 = comportement natif (brutal), 0.5 =
    /// deux fois moins nerveux (défaut d'après le brief 2/7/2026), 0.25 =
    /// très doux. En dessous de 0.05 le zoom devient inutilisable.
    pub zoom_scroll_step: f32,
    // --- Typographie ---
    pub french_quotes: bool,
    pub auto_emdash: bool,
    pub auto_ellipsis: bool,
    pub auto_nbsp: bool,
    // --- Wikilinks ---
    /// Opacité des crochets [[ ]] en pourcent (15 = quasi invisibles).
    pub wikilink_bracket_opacity: u8,
    pub wikilink_autocomplete_min_chars: usize,
    pub wikilink_autocomplete_max_results: usize,
    // --- Stats / objectif ---
    pub goal_reached_color: String,
    pub stats_debounce_ms: u64,
    pub status_bar_color: String,
    // --- Focus / typewriter ---
    pub focus_show_status_bar: bool,
    pub focus_minimal_status: bool,
    /// Position verticale de la ligne courante en mode typewriter (0.0 = haut,
    /// 0.5 = centre).
    pub typewriter_position: f32,
    // --- Sauvegarde ---
    /// Durée du flag "c'est moi qui écris" (silence notify), en secondes.
    pub self_write_silence_secs: u64,
}

impl Default for EditorVomi {
    fn default() -> Self {
        Self {
            cursor_thickness: 2.0,
            cursor_blink_ms: 500,
            cursor_color: String::new(),
            line_height: 1.6,
            paragraph_spacing: 14.0,
            editor_padding: 40.0,
            syntax_highlight_buffer_lines: 10,
            heading_h1_scale: 1.6,
            heading_h2_scale: 1.4,
            heading_h3_scale: 1.25,
            heading_h4_scale: 1.15,
            heading_h5_scale: 1.05,
            heading_h6_scale: 1.0,
            heading_h1: String::new(),
            heading_h2: String::new(),
            heading_h3: String::new(),
            heading_h4: String::new(),
            heading_h5: String::new(),
            heading_h6: String::new(),
            foreground: String::new(),
            inline_code_foreground: String::new(),
            inline_code_background: String::new(),
            code_block_foreground: String::new(),
            code_block_background: String::new(),
            blockquote_color: String::new(),
            blockquote_border: String::new(),
            wikilink_valid: String::new(),
            wikilink_orphan: String::new(),
            tag_color: String::new(),
            frontmatter_color: String::new(),
            comment_prefix: "%".into(),
            comment_color: String::new(),
            markers_color: String::new(),
            current_line_background: String::new(),
            selection_background: String::new(),
            search_highlight_background: String::new(),
            search_current_background: String::new(),
            zoom_min_pt: 10.0,
            zoom_max_pt: 48.0,
            zoom_scroll_step: 0.5,
            french_quotes: true,
            auto_emdash: true,
            auto_ellipsis: true,
            auto_nbsp: true,
            wikilink_bracket_opacity: 15,
            wikilink_autocomplete_min_chars: 1,
            wikilink_autocomplete_max_results: 8,
            goal_reached_color: String::new(),
            stats_debounce_ms: 200,
            status_bar_color: String::new(),
            focus_show_status_bar: true,
            focus_minimal_status: true,
            typewriter_position: 0.4,
            self_write_silence_secs: 2,
        }
    }
}

/// Options globales EH5 lues depuis `engram.ron`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Basic {
    pub font_size: u32,
}

impl Default for Basic {
    fn default() -> Self {
        Self { font_size: 15 }
    }
}

/// Palette résolue : chaque champ de couleur vide retombe sur le défaut
/// OLED rose. Calculée une fois au chargement, pas par frame.
#[derive(Debug, Clone)]
pub struct Theme {
    pub foreground: egui::Color32,
    pub cursor: egui::Color32,
    pub headings: [egui::Color32; 6],
    pub heading_scales: [f32; 6],
    pub inline_code_fg: egui::Color32,
    pub inline_code_bg: egui::Color32,
    pub code_block_fg: egui::Color32,
    pub code_block_bg: egui::Color32,
    pub blockquote: egui::Color32,
    pub blockquote_border: egui::Color32,
    pub wikilink_valid: egui::Color32,
    pub wikilink_orphan: egui::Color32,
    pub tag: egui::Color32,
    pub frontmatter: egui::Color32,
    pub comment: egui::Color32,
    pub markers: egui::Color32,
    pub current_line_bg: egui::Color32,
    pub selection_bg: egui::Color32,
    pub search_bg: egui::Color32,
    pub search_current_bg: egui::Color32,
    pub goal_reached: egui::Color32,
    pub status_bar: egui::Color32,
}

/// "#rrggbb" → Color32, sinon `fallback`. Une couleur illisible est une
/// erreur GLaDOS remontée par le chargement (pas un échec silencieux).
fn parse_color(
    s: &str,
    fallback: egui::Color32,
    errors: &mut Vec<String>,
    field: &str,
) -> egui::Color32 {
    if s.is_empty() {
        return fallback;
    }
    let hex = s.trim_start_matches('#');
    if hex.len() == 6 {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            return egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
        }
    }
    errors.push(format!(
        "licorne-a-gerber.ron (section editor) : '{s}' n'est pas une couleur pour \
         {field}. J'attends du #rrggbb. Fallback sur le thème."
    ));
    fallback
}

impl Theme {
    /// Résout les couleurs de l'éditeur. La SOURCE par défaut est la palette
    /// globale (theme.ron) ; licorne-a-gerber_editor.ron peut surcharger
    /// chaque champ finement (couleur vide = on garde le défaut de la palette).
    pub fn resolve(p: &engram_core::Palette, v: &EditorVomi, errors: &mut Vec<String>) -> Self {
        let muted = egui::Color32::from_gray(110);
        let c = |s: &str, f: egui::Color32, n: &str, e: &mut Vec<String>| parse_color(s, f, e, n);
        Self {
            foreground: c(&v.foreground, p.foreground, "foreground", errors),
            cursor: c(&v.cursor_color, p.cursor, "cursor_color", errors),
            headings: [
                c(&v.heading_h1, p.headings[0], "heading_h1", errors),
                c(&v.heading_h2, p.headings[1], "heading_h2", errors),
                c(&v.heading_h3, p.headings[2], "heading_h3", errors),
                c(&v.heading_h4, p.headings[3], "heading_h4", errors),
                c(&v.heading_h5, p.headings[4], "heading_h5", errors),
                c(&v.heading_h6, p.headings[5], "heading_h6", errors),
            ],
            heading_scales: [
                v.heading_h1_scale,
                v.heading_h2_scale,
                v.heading_h3_scale,
                v.heading_h4_scale,
                v.heading_h5_scale,
                v.heading_h6_scale,
            ],
            inline_code_fg: c(
                &v.inline_code_foreground,
                p.accent_secondary,
                "inline_code_foreground",
                errors,
            ),
            inline_code_bg: c(
                &v.inline_code_background,
                egui::Color32::from_gray(24),
                "inline_code_background",
                errors,
            ),
            code_block_fg: c(
                &v.code_block_foreground,
                p.accent_secondary,
                "code_block_foreground",
                errors,
            ),
            code_block_bg: c(
                &v.code_block_background,
                egui::Color32::from_gray(16),
                "code_block_background",
                errors,
            ),
            blockquote: c(
                &v.blockquote_color,
                egui::Color32::from_rgb(180, 175, 185),
                "blockquote_color",
                errors,
            ),
            blockquote_border: c(
                &v.blockquote_border,
                p.accent_secondary,
                "blockquote_border",
                errors,
            ),
            wikilink_valid: c(
                &v.wikilink_valid,
                p.wikilink_valid,
                "wikilink_valid",
                errors,
            ),
            wikilink_orphan: c(
                &v.wikilink_orphan,
                p.wikilink_orphan,
                "wikilink_orphan",
                errors,
            ),
            tag: c(&v.tag_color, p.accent_secondary, "tag_color", errors),
            frontmatter: c(
                &v.frontmatter_color,
                egui::Color32::from_rgb(140, 170, 160),
                "frontmatter_color",
                errors,
            ),
            comment: c(&v.comment_color, muted, "comment_color", errors),
            markers: c(
                &v.markers_color,
                egui::Color32::from_gray(95),
                "markers_color",
                errors,
            ),
            current_line_bg: c(
                &v.current_line_background,
                egui::Color32::from_gray(14),
                "current_line_background",
                errors,
            ),
            selection_bg: c(
                &v.selection_background,
                p.selection_bg,
                "selection_background",
                errors,
            ),
            search_bg: c(
                &v.search_highlight_background,
                egui::Color32::from_rgb(90, 70, 10),
                "search_highlight_background",
                errors,
            ),
            search_current_bg: c(
                &v.search_current_background,
                egui::Color32::from_rgb(140, 105, 15),
                "search_current_background",
                errors,
            ),
            goal_reached: c(
                &v.goal_reached_color,
                egui::Color32::from_rgb(80, 220, 120),
                "goal_reached_color",
                errors,
            ),
            status_bar: c(&v.status_bar_color, muted, "status_bar_color", errors),
        }
    }
}

/// Les deux configs + palette résolue, telles qu'utilisées au runtime.
#[derive(Debug, Clone)]
pub struct Config {
    pub simple: Editor,
    pub vomi: EditorVomi,
    pub theme: Theme,
    /// Dossier ~/.config/engram_hive/modules/editor/
    pub module_config_dir: PathBuf,
    /// Dossier ~/.config/engram_hive (snippets/, fonts/, keybinds.ron).
    pub config_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let vomi = EditorVomi::default();
        let theme = Theme::resolve(&engram_core::Palette::default(), &vomi, &mut Vec::new());
        Self {
            simple: Editor::default(),
            vomi,
            theme,
            module_config_dir: PathBuf::new(),
            config_dir: PathBuf::new(),
        }
    }
}

impl Config {
    /// Charge la section `editor` puis `editor_expert` depuis `engram.ron`.
    /// Les couleurs dérivent de la palette globale `theme`, surchargeables
    /// dans la section experte. Retourne (config, erreurs GLaDOS à afficher).
    pub fn load(
        config_dir: &Path,
        theme: &engram_core::Palette,
        licorne: &engram_core::Licorne,
    ) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let dir = config_dir.join("modules").join("editor");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            errors.push(format!(
                "Impossible de créer {} : {e}. Je continue avec les valeurs par \
                 défaut, mais rien ne sera persisté. À toi de voir.",
                dir.display()
            ));
        }

        let simple: Editor = licorne.section("editor", &mut errors);
        let vomi: EditorVomi = licorne.section("editor_expert", &mut errors);

        let theme = Theme::resolve(theme, &vomi, &mut errors);
        (
            Self {
                simple,
                vomi,
                theme,
                module_config_dir: dir,
                config_dir: config_dir.to_path_buf(),
            },
            errors,
        )
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // les constantes en queue de fichier sont historiquement gardées après le mod tests
mod tests {
    use super::*;

    /// Les modèles commités dans modules/editor/config/ doivent parser :
    /// un exemple qui ne compile pas est un mensonge.
    #[test]
    fn shipped_config_examples_parse() -> Result<(), Box<dyn std::error::Error>> {
        let sections: std::collections::HashMap<String, ron::Value> =
            ron::from_str(include_str!("../../../config/engram.ron"))?;
        let simple: Editor = sections
            .get("editor")
            .ok_or("section editor absente")?
            .clone()
            .into_rust()?;
        assert_eq!(simple.font_size, 15);
        let vomi: EditorVomi = sections
            .get("editor_expert")
            .ok_or("section editor_expert absente")?
            .clone()
            .into_rust()?;
        assert_eq!(vomi.cursor_blink_ms, 500);
        Ok(())
    }

    #[test]
    fn broken_color_falls_back_with_glados() {
        let mut errors = Vec::new();
        let vomi = EditorVomi {
            cursor_color: "vite".into(),
            ..EditorVomi::default()
        };
        let palette = engram_core::Palette::default();
        let theme = Theme::resolve(&palette, &vomi, &mut errors);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("cursor_color"));
        // Couleur cassée → fallback sur le curseur de la palette (#FF00FF).
        assert_eq!(theme.cursor, palette.cursor);
    }
}
