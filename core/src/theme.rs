// ============================================================================
// core/src/theme.rs — Thème partagé de Hive_RBMK_Tcherenkov
//
// Source unique des couleurs et polices, partagée par tous les modules.
// Une seule source de vérité : les sections `theme` et `theme_expert` de
// `Hive_RBMK.ron`.
//
// À NE PAS confondre avec les configs PAR module (modules/<nom>/). Ici :
// l'apparence globale. Là-bas : le comportement de chaque module.
//
// Les couleurs sont en "#rrggbb". Champ commenté / vide = défaut. Syntaxe
// cassée = log + fallback sur les défauts (la palette néon de Steve).
// ============================================================================

use egui::Color32;

/// Config simple, user-facing (section `theme` de Hive_RBMK.ron).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename = "Theme", default)]
pub struct ThemeConfig {
    pub background: String,
    pub foreground: String,
    pub accent_primary: String,
    pub accent_secondary: String,
    pub font_editor: String,
    pub font_size: u32,
    pub font_ui: String,
    pub font_ui_size: u32,
    pub cursor: String,
    pub border_color: String,
    pub border_width: u32,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: "#000000".into(),
            foreground: "#FF00FF".into(),
            accent_primary: "#FF00FF".into(),
            accent_secondary: "#7B00FF".into(),
            font_editor: "Georgia".into(),
            font_size: 16,
            font_ui: "FiraSans".into(),
            font_ui_size: 13,
            cursor: "#FF00FF".into(),
            border_color: "#7B00FF".into(),
            border_width: 2,
        }
    }
}

/// Config experte, optionnelle (theme_expert de `Hive_RBMK.ron`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename = "ThemeExpert", default)]
pub struct ThemeExpert {
    pub selection_background: String,
    pub selection_foreground: String,
    pub bold_color: String,
    pub italic_color: String,
    pub highlight_background: String,
    pub highlight_foreground: String,
    pub heading_h1: String,
    pub heading_h2: String,
    pub heading_h3: String,
    pub heading_h4: String,
    pub heading_h5: String,
    pub heading_h6: String,
    pub wikilink_valid: String,
    pub wikilink_orphan: String,
    pub wikilink_bracket_opacity: u8,
    pub border_glow: bool,
    pub border_width: u32,
    pub border_color: String,
}

impl Default for ThemeExpert {
    fn default() -> Self {
        Self {
            selection_background: "#FFD700".into(),
            selection_foreground: "#000000".into(),
            bold_color: "#9B59B6".into(),
            italic_color: "#FF55FF".into(),
            highlight_background: "#FF8C00".into(),
            highlight_foreground: "#000000".into(),
            heading_h1: "#FF00FF".into(),
            heading_h2: "#CC00FF".into(),
            heading_h3: "#9900FF".into(),
            heading_h4: "#6600FF".into(),
            heading_h5: "#3300FF".into(),
            heading_h6: "#7B00FF".into(),
            wikilink_valid: "#FF00FF".into(),
            wikilink_orphan: "#FF4444".into(),
            wikilink_bracket_opacity: 10,
            border_glow: true,
            border_width: 2,
            border_color: "#7B00FF".into(),
        }
    }
}

/// Palette résolue (Color32 + polices), prête à l'emploi par les modules.
/// Calculée une fois au chargement, jamais par frame.
#[derive(Debug, Clone)]
pub struct Palette {
    pub background: Color32,
    pub foreground: Color32,
    pub accent_primary: Color32,
    pub accent_secondary: Color32,
    pub cursor: Color32,
    pub border: Color32,
    pub border_width: f32,
    pub border_glow: bool,
    pub selection_bg: Color32,
    pub selection_fg: Color32,
    pub bold: Color32,
    pub italic: Color32,
    pub highlight_bg: Color32,
    pub highlight_fg: Color32,
    pub headings: [Color32; 6],
    pub wikilink_valid: Color32,
    pub wikilink_orphan: Color32,
    pub wikilink_bracket_opacity: u8,
    pub font_editor: String,
    pub font_editor_size: f32,
    pub font_ui: String,
    pub font_ui_size: f32,
}

impl Default for Palette {
    fn default() -> Self {
        Self::resolve(
            &ThemeConfig::default(),
            &ThemeExpert::default(),
            &mut Vec::new(),
        )
    }
}

/// "#rrggbb" → Color32, sinon `fallback` (+ erreur GLaDOS si non vide/invalide).
fn parse_color(s: &str, fallback: Color32, field: &str, errors: &mut Vec<String>) -> Color32 {
    if s.is_empty() {
        return fallback;
    }
    let hex = s.trim_start_matches('#');
    if hex.len() == 6 {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            return Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
        }
    }
    errors.push(format!(
        "theme : '{s}' n'est pas une couleur pour {field}. J'attends du #rrggbb. \
         Fallback sur le défaut."
    ));
    fallback
}

impl Palette {
    pub fn resolve(simple: &ThemeConfig, expert: &ThemeExpert, errors: &mut Vec<String>) -> Self {
        let d = ThemeConfig::default();
        let de = ThemeExpert::default();
        let c = |s: &str, def: &str, field: &str, e: &mut Vec<String>| {
            // `def` est toujours un #rrggbb valide (issu des défauts).
            let fallback = parse_color(def, Color32::MAGENTA, field, &mut Vec::new());
            parse_color(s, fallback, field, e)
        };
        Self {
            background: c(&simple.background, &d.background, "background", errors),
            foreground: c(&simple.foreground, &d.foreground, "foreground", errors),
            accent_primary: c(
                &simple.accent_primary,
                &d.accent_primary,
                "accent_primary",
                errors,
            ),
            accent_secondary: c(
                &simple.accent_secondary,
                &d.accent_secondary,
                "accent_secondary",
                errors,
            ),
            cursor: c(&simple.cursor, &d.cursor, "cursor", errors),
            border: c(
                &expert.border_color,
                &de.border_color,
                "border_color",
                errors,
            ),
            border_width: if expert.border_width > 0 {
                expert.border_width as f32
            } else {
                simple.border_width as f32
            },
            border_glow: expert.border_glow,
            selection_bg: c(
                &expert.selection_background,
                &de.selection_background,
                "selection_background",
                errors,
            ),
            selection_fg: c(
                &expert.selection_foreground,
                &de.selection_foreground,
                "selection_foreground",
                errors,
            ),
            bold: c(&expert.bold_color, &de.bold_color, "bold_color", errors),
            italic: c(
                &expert.italic_color,
                &de.italic_color,
                "italic_color",
                errors,
            ),
            highlight_bg: c(
                &expert.highlight_background,
                &de.highlight_background,
                "highlight_background",
                errors,
            ),
            highlight_fg: c(
                &expert.highlight_foreground,
                &de.highlight_foreground,
                "highlight_foreground",
                errors,
            ),
            headings: [
                c(&expert.heading_h1, &de.heading_h1, "heading_h1", errors),
                c(&expert.heading_h2, &de.heading_h2, "heading_h2", errors),
                c(&expert.heading_h3, &de.heading_h3, "heading_h3", errors),
                c(&expert.heading_h4, &de.heading_h4, "heading_h4", errors),
                c(&expert.heading_h5, &de.heading_h5, "heading_h5", errors),
                c(&expert.heading_h6, &de.heading_h6, "heading_h6", errors),
            ],
            wikilink_valid: c(
                &expert.wikilink_valid,
                &de.wikilink_valid,
                "wikilink_valid",
                errors,
            ),
            wikilink_orphan: c(
                &expert.wikilink_orphan,
                &de.wikilink_orphan,
                "wikilink_orphan",
                errors,
            ),
            wikilink_bracket_opacity: expert.wikilink_bracket_opacity,
            font_editor: simple.font_editor.clone(),
            font_editor_size: simple.font_size as f32,
            font_ui: simple.font_ui.clone(),
            font_ui_size: simple.font_ui_size as f32,
        }
    }

    /// Charge la section `theme` et la section `theme_expert` de
    /// `Hive_RBMK.ron`. Retourne (palette, erreurs à loguer/afficher).
    pub fn load(
        _config_dir: &std::path::Path,
        licorne: &crate::licorne::Licorne,
    ) -> (Self, Vec<String>) {
        let mut errors = Vec::new();

        let simple: ThemeConfig = licorne.section("theme", &mut errors);
        let expert: ThemeExpert = licorne.section("theme_expert", &mut errors);

        (Self::resolve(&simple, &expert, &mut errors), errors)
    }
}

/// Applique une palette résolue aux visuels egui. Le `Context` egui est partagé
/// par TOUS les viewports (fenêtres OS) : un seul appel repeint toute l'app.
/// Source unique d'apparence, appelée au démarrage (app) ET à chaud après une
/// modification dans le cockpit (« Appliquer » sans redémarrage).
pub fn apply_palette(ctx: &egui::Context, p: &Palette) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = p.background;
    v.window_fill = p.background;
    v.extreme_bg_color = p.background;
    v.faint_bg_color = egui::Color32::from_gray(12);
    v.selection.bg_fill = p.selection_bg;
    v.selection.stroke = egui::Stroke::new(1.0_f32, p.accent_primary);
    v.hyperlink_color = p.accent_secondary;
    v.override_text_color = Some(p.foreground);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, p.foreground);
    let bw = p.border_width.max(1.0);
    v.window_stroke = egui::Stroke::new(bw, p.border);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, p.accent_secondary);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.5_f32, p.accent_primary);
    ctx.set_visuals(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_resolve_to_neon_palette() {
        let p = Palette::default();
        assert_eq!(p.background, Color32::from_rgb(0x00, 0x00, 0x00));
        assert_eq!(p.foreground, Color32::from_rgb(0xFF, 0x00, 0xFF));
        assert_eq!(p.accent_primary, Color32::from_rgb(0xFF, 0x00, 0xFF));
        assert_eq!(p.accent_secondary, Color32::from_rgb(0x7B, 0x00, 0xFF));
        assert_eq!(p.headings[5], Color32::from_rgb(0x7B, 0x00, 0xFF));
        assert_eq!(p.selection_bg, Color32::from_rgb(0xFF, 0xD7, 0x00));
        assert_eq!(p.font_editor_size, 16.0);
    }

    #[test]
    fn shipped_expert_model_parses() -> Result<(), Box<dyn std::error::Error>> {
        let sections: std::collections::HashMap<String, ron::Value> =
            ron::from_str(include_str!("../../config/Hive_RBMK.ron"))?;
        let e: ThemeExpert = sections
            .get("theme_expert")
            .ok_or("section theme_expert absente")?
            .clone()
            .into_rust()?;
        assert_eq!(e.wikilink_bracket_opacity, 10);
        assert!(e.border_glow);
        Ok(())
    }

    #[test]
    fn broken_color_falls_back_and_reports() {
        let simple = ThemeConfig {
            accent_primary: "pas une couleur".into(),
            ..ThemeConfig::default()
        };
        let mut errors = Vec::new();
        let p = Palette::resolve(&simple, &ThemeExpert::default(), &mut errors);
        assert_eq!(errors.len(), 1);
        assert_eq!(p.accent_primary, Color32::from_rgb(0xFF, 0x00, 0xFF)); // fallback défaut
    }
}
