// ============================================================================
// modules/cockpit/src/md_mirror.rs — Génération des .typ miroir pédagogiques
//
// Pour chaque RON édité par le Cockpit, on écrit un .typ à côté qui explique
// chaque champ (rôle, type, valeur courante, défaut). Objectif : la doc reste
// synchronisée avec l'état réel sur disque sans dépendre de l'éditeur RON.
// ============================================================================

use std::path::{Path, PathBuf};

use engram_core::theme::ThemeConfig;

/// Écrit ~/.config/engram_hive/theme.typ à partir du ThemeConfig fourni.
pub fn write_theme_typ(config_dir: &Path, c: &ThemeConfig) -> Result<PathBuf, String> {
    let path = config_dir.join("theme.typ");
    let body = render_theme_typ(c);
    std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn render_theme_typ(c: &ThemeConfig) -> String {
    let d = ThemeConfig::default();
    let mut s = String::new();
    s.push_str("= theme.ron — Doc miroir générée par le Cockpit\n\n");
    s.push_str(
        "Ce fichier est régénéré automatiquement à chaque écriture du Cockpit.\n",
    );
    s.push_str("Ne pas éditer à la main : modifie `theme.ron` (ou utilise le Cockpit).\n\n");
    s.push_str("== Couleurs\n\n");
    s.push_str("Chaque couleur est une chaîne `\"#rrggbb\"`. Vide ou invalide → défaut.\n\n");
    field(
        &mut s,
        "background",
        &c.background,
        &d.background,
        "Couleur de fond globale. OLED-friendly à `#000000`.",
    );
    field(
        &mut s,
        "foreground",
        &c.foreground,
        &d.foreground,
        "Couleur du texte principal (éditeur, file tree).",
    );
    field(
        &mut s,
        "accent_primary",
        &c.accent_primary,
        &d.accent_primary,
        "Accent fort : curseur, liens, mises en valeur.",
    );
    field(
        &mut s,
        "accent_secondary",
        &c.accent_secondary,
        &d.accent_secondary,
        "Accent secondaire : UI, menus, bordures, dossiers du file tree.",
    );
    field(
        &mut s,
        "cursor",
        &c.cursor,
        &d.cursor,
        "Couleur du curseur de l'éditeur.",
    );
    field(
        &mut s,
        "border_color",
        &c.border_color,
        &d.border_color,
        "Couleur des bordures de fenêtre (combiné avec border_width).",
    );

    s.push_str("\n== Tailles\n\n");
    num(
        &mut s,
        "border_width",
        c.border_width as i64,
        d.border_width as i64,
        "Largeur des bordures en pixels. 0 pour aucune.",
    );
    num(
        &mut s,
        "font_size",
        c.font_size as i64,
        d.font_size as i64,
        "Taille de la police de prose (éditeur) en points.",
    );
    num(
        &mut s,
        "font_ui_size",
        c.font_ui_size as i64,
        d.font_ui_size as i64,
        "Taille de la police d'UI (file tree, status bar) en points.",
    );

    s.push_str("\n== Polices\n\n");
    text(
        &mut s,
        "font_editor",
        &c.font_editor,
        &d.font_editor,
        "Nom de la police de prose. Doit être installée sur le système.",
    );
    text(
        &mut s,
        "font_ui",
        &c.font_ui,
        &d.font_ui,
        "Nom de la police d'UI. Doit être installée sur le système.",
    );

    s.push_str("\n== Reload\n\n");
    s.push_str(
        "Le Cockpit n'applique pas à chaud : redémarre Engram_Hive pour voir les \
                changements (sauf si tu en modifies via une commande qui le précise).\n\n",
    );
    s.push_str("== Configuration experte\n\n");
    s.push_str(
        "Les options avancées (sélection, gras/italique, dégradé des titres, glow \
                de bordure…) vivent dans la section `\"theme\"` de \
                `engram.ron` à côté de ce fichier (legacy accepté).\n",
    );
    s
}

fn field(out: &mut String, name: &str, value: &str, default: &str, desc: &str) {
    let marker = if value == default {
        ""
    } else {
        " *(modifié)*"
    };
    out.push_str(&format!(
        "=== `{name}`{marker}\n\n- *Valeur* : `{value}`\n- *Défaut* : `{default}`\n- {desc}\n\n"
    ));
}

fn num(out: &mut String, name: &str, value: i64, default: i64, desc: &str) {
    let marker = if value == default {
        ""
    } else {
        " *(modifié)*"
    };
    out.push_str(&format!(
        "=== `{name}`{marker}\n\n- *Valeur* : `{value}`\n- *Défaut* : `{default}`\n- {desc}\n\n"
    ));
}

fn text(out: &mut String, name: &str, value: &str, default: &str, desc: &str) {
    let marker = if value == default {
        ""
    } else {
        " *(modifié)*"
    };
    out.push_str(&format!(
        "=== `{name}`{marker}\n\n- *Valeur* : `\"{value}\"`\n- *Défaut* : `\"{default}\"`\n- {desc}\n\n"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_default_without_panicking() {
        let body = render_theme_typ(&ThemeConfig::default());
        assert!(body.contains("= theme.ron"));
        assert!(body.contains("background"));
        assert!(body.contains("font_editor"));
    }

    #[test]
    fn marks_modified_fields() {
        let c = ThemeConfig {
            foreground: "#123456".into(),
            ..ThemeConfig::default()
        };
        let body = render_theme_typ(&c);
        assert!(body.contains("foreground` *(modifié)*"));
    }
}
