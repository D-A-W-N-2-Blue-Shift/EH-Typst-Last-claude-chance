// ============================================================================
// modules/editor/src/toc.rs — Sommaire (table des matières) du fichier courant
//
// Panneau latéral listant les titres H1→H5 du document, avec indentation par
// niveau et couleur du thème. Clic sur un titre → scroll vers sa ligne. Échap
// ferme. Ouverture : Ctrl+Shift+O ou menu contextuel « Sommaire ».
//
// Le parsing ignore le bloc de métadonnées en tête et l'intérieur des blocs
// de code (``` … ```), pour ne pas confondre une expression Typst avec un titre.
// Reparse débounce (200 ms) tant que le panneau est ouvert.
// ============================================================================

use std::time::{Duration, Instant};

use ropey::Rope;

use crate::config::Theme;

/// Un titre du document.
pub struct TocEntry {
    /// Niveau 1..=5 (nombre de `=`).
    pub level: usize,
    pub text: String,
    /// Index de ligne (0-based) du titre dans le buffer.
    pub line: usize,
}

#[derive(Default)]
pub struct TocState {
    pub open: bool,
    entries: Vec<TocEntry>,
    last_parse: Option<Instant>,
}

impl TocState {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        // Forcer un reparse à la prochaine ouverture.
        self.last_parse = None;
    }

    /// Reparse au plus toutes les 200 ms (débounce) tant que le panneau est
    /// ouvert : suit l'édition en temps réel sans rescanner à chaque frame.
    pub fn refresh(&mut self, rope: &Rope) {
        let stale = self
            .last_parse
            .is_none_or(|t| t.elapsed() > Duration::from_millis(200));
        if stale {
            self.entries = parse(rope);
            self.last_parse = Some(Instant::now());
        }
    }
}

/// Extrait les titres Typst H1→H5. Ignore le bloc de métadonnées et les blocs de code.
pub fn parse(rope: &Rope) -> Vec<TocEntry> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut in_frontmatter = false;
    for (i, line) in rope.lines().enumerate() {
        let s = line.to_string();
        let t = s.trim_start();
        if i == 0 && (t.trim_end() == "---" || t.trim_end() == "/*") {
            in_frontmatter = true;
            continue;
        }
        if i == 1 && in_frontmatter && t.trim_end() == "---" {
            continue;
        }
        if in_frontmatter {
            if t.trim_end() == "---" || t.trim_end() == "*/" {
                in_frontmatter = false;
            }
            continue;
        }
        // Clôtures de bloc de code.
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // Titre : 1..=5 `=` suivis d'un espace.
        let levels = t.bytes().take_while(|&b| b == b'=').count();
        if (1..=5).contains(&levels) {
            let rest = &t[levels..];
            if rest.starts_with(' ') {
                let text = rest.trim().to_string();
                if !text.is_empty() {
                    out.push(TocEntry {
                        level: levels,
                        text,
                        line: i,
                    });
                }
            }
        }
    }
    out
}

/// Dessine le panneau Sommaire (SidePanel droit). Retourne la ligne du titre
/// cliqué, le cas échéant. Échap (ou ✕) ferme le panneau.
pub fn show(ctx: &egui::Context, state: &mut TocState, theme: &Theme) -> Option<usize> {
    if !state.open {
        return None;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        state.open = false;
        return None;
    }
    let mut clicked = None;
    let mut close = false;
    egui::SidePanel::right("editor_toc")
        .resizable(true)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Sommaire");
                if ui.small_button("✕").clicked() {
                    close = true;
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if state.entries.is_empty() {
                        ui.weak("Aucun titre dans ce fichier.");
                    }
                    for e in &state.entries {
                        ui.horizontal(|ui| {
                            let indent = (e.level as f32 - 1.0) * 12.0;
                            ui.add_space(indent);
                            let color = theme.headings[(e.level - 1).min(5)];
                            // H1 en gras ; H4/H5 légèrement réduits. Les autres
                            // gardent la taille de base. Couleur = niveau du titre.
                            let mut rt = egui::RichText::new(&e.text).color(color);
                            if e.level == 1 {
                                rt = rt.strong();
                            } else if e.level >= 4 {
                                rt = rt.size(12.0);
                            }
                            // Le label se tronque (…) à la largeur restante du
                            // panel : rétréci au drag, le texte ne déborde pas.
                            let label = egui::Label::new(rt).truncate().sense(egui::Sense::click());
                            if ui.add(label).clicked() {
                                clicked = Some(e.line);
                            }
                        });
                    }
                });
        });
    if close {
        state.open = false;
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_levels_and_lines() {
        let rope = Rope::from_str("= Titre\ntexte\n== Sous\n=== Sous-sous\n");
        let toc = parse(&rope);
        assert_eq!(toc.len(), 3);
        assert_eq!(toc[0].level, 1);
        assert_eq!(toc[0].text, "Titre");
        assert_eq!(toc[0].line, 0);
        assert_eq!(toc[1].level, 2);
        assert_eq!(toc[1].line, 2);
        assert_eq!(toc[2].level, 3);
    }

    #[test]
    fn ignores_frontmatter_and_code_fences() {
        let rope = Rope::from_str(
            "/*\n---\ntitle: x\n# pas un titre (yaml)\n---\n*/\n= Vrai titre\n```\n# commentaire code\n```\n== Après\n",
        );
        let toc = parse(&rope);
        let texts: Vec<_> = toc.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["Vrai titre", "Après"]);
    }

    #[test]
    fn h6_and_non_headings_ignored() {
        let rope = Rope::from_str("====== H6 ignoré\n#pas-de-espace\ntexte # milieu\n");
        assert!(parse(&rope).is_empty());
    }
}
