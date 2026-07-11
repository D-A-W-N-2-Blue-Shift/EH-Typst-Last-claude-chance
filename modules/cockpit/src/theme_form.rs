// ============================================================================
// modules/cockpit/src/theme_form.rs — Formulaire de la catégorie Thème
//
// Dessine les champs de ThemeConfig (color pickers, sliders, text edits) et
// sérialise vers un theme.ron formaté avec ses commentaires d'origine.
// `ron::ser` ne sait pas écrire de commentaires : on assemble la chaîne à la
// main. Le résultat reste lisible et reparseable.
// ============================================================================

use engram_core::theme::ThemeConfig;

/// Dessine le formulaire. Mute directement `cfg`.
pub fn draw(ui: &mut egui::Ui, cfg: &mut ThemeConfig) {
    egui::Grid::new("theme_form_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            color_row(ui, "Fond (background)", &mut cfg.background);
            color_row(ui, "Texte (foreground)", &mut cfg.foreground);
            color_row(ui, "Accent primaire", &mut cfg.accent_primary);
            color_row(ui, "Accent secondaire", &mut cfg.accent_secondary);
            color_row(ui, "Curseur", &mut cfg.cursor);
            color_row(ui, "Bordure", &mut cfg.border_color);

            ui.label("Largeur de bordure (px)");
            ui.add(egui::DragValue::new(&mut cfg.border_width).range(0..=64));
            ui.end_row();

            ui.label("Police éditeur");
            ui.text_edit_singleline(&mut cfg.font_editor);
            ui.end_row();

            ui.label("Taille texte éditeur (pt)");
            ui.add(egui::DragValue::new(&mut cfg.font_size).range(8..=48));
            ui.end_row();

            ui.label("Police UI");
            ui.text_edit_singleline(&mut cfg.font_ui);
            ui.end_row();

            ui.label("Taille texte UI (pt)");
            ui.add(egui::DragValue::new(&mut cfg.font_ui_size).range(8..=24));
            ui.end_row();
        });
}

/// Une ligne label + color picker. Le champ reste une chaîne "#rrggbb" dans
/// le RON ; on convertit aller-retour pour l'UI.
fn color_row(ui: &mut egui::Ui, label: &str, hex: &mut String) {
    ui.label(label);
    let mut rgb = hex_to_rgb(hex).unwrap_or([255, 0, 255]);
    let resp = ui.color_edit_button_srgb(&mut rgb);
    ui.label(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
    if resp.changed() {
        *hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
    }
    ui.end_row();
}

fn hex_to_rgb(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Sérialise un ThemeConfig en RON avec commentaires pédagogiques. Pas de
/// ron::ser (qui dépouille les commentaires) : assemblage manuel. Reste
/// reparseable à 100 % (les défauts sont déjà couverts par #[serde(default)]).
pub fn serialize_theme_ron(c: &ThemeConfig) -> String {
    format!(
        r##"// ============================================================================
// theme.ron — Apparence globale d'Engram_Hive (couleurs + polices).
//
// Fichier régénéré par le Cockpit. Les commentaires utilisateur précédents
// ont été remplacés par cette en-tête standard. Voir theme.typ à côté pour
// la doc complète de chaque champ.
//
// Pour les options avancées : section "theme" de engram.ron (legacy accepté).
// Couleurs en "#rrggbb".
// ============================================================================
Theme(
    // Couleur de fond globale.
    background: {bg:?},
    // Couleur du texte principal.
    foreground: {fg:?},
    // Accent principal (curseur, liens, accents).
    accent_primary: {ap:?},
    // Accent secondaire (UI, menus, bordures).
    accent_secondary: {as_:?},
    // Police de prose dans l'éditeur.
    font_editor: {fe:?},
    font_size: {fs},
    // Police d'interface (file tree, status bar, menus).
    font_ui: {fu:?},
    font_ui_size: {fus},
    cursor: {cu:?},
    border_color: {bc:?},
    border_width: {bw},
)
"##,
        bg = c.background,
        fg = c.foreground,
        ap = c.accent_primary,
        as_ = c.accent_secondary,
        fe = c.font_editor,
        fs = c.font_size,
        fu = c.font_ui,
        fus = c.font_ui_size,
        cu = c.cursor,
        bc = c.border_color,
        bw = c.border_width,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_default() -> Result<(), Box<dyn std::error::Error>> {
        let original = ThemeConfig::default();
        let body = serialize_theme_ron(&original);
        let reparsed: ThemeConfig = ron::from_str(&body)?;
        assert_eq!(reparsed.background, original.background);
        assert_eq!(reparsed.foreground, original.foreground);
        assert_eq!(reparsed.font_size, original.font_size);
        assert_eq!(reparsed.border_width, original.border_width);
        Ok(())
    }

    #[test]
    fn hex_parse() {
        assert_eq!(hex_to_rgb("#FF00FF"), Some([255, 0, 255]));
        assert_eq!(hex_to_rgb("000000"), Some([0, 0, 0]));
        assert_eq!(hex_to_rgb("bad"), None);
    }
}
