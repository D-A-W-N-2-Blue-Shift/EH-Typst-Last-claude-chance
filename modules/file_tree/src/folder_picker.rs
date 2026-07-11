// ============================================================================
// modules/file_tree/src/folder_picker.rs — Sélecteur de dossier intégré
//
// Remplace rfd / le portail XDG : un navigateur de dossiers 100% egui, sans
// aucune dépendance système. Même comportement sur tout WM, tout sous notre
// contrôle (doctrine anti-black-box). Aucun thread, aucun D-Bus.
//
// Modèle « navigue puis choisis » : on entre dans les dossiers (clic), la
// barre de chemin permet de sauter directement (saisie + Entrée, « ~ » géré),
// et « Choisir ce dossier » valide le dossier COURANT.
//   - OpenProject        → le dossier courant est le projet à ouvrir
//   - NewProjectLocation → le dossier courant est l'emplacement du nouveau
// ============================================================================

use std::path::{Path, PathBuf};

use crate::PickerPurpose;

/// Résultat d'une frame du sélecteur.
pub enum PickerOutcome {
    /// Toujours ouvert, rien à faire.
    Browsing,
    /// L'utilisateur a validé ce dossier.
    Selected(PathBuf),
    /// Fermé sans choisir.
    Cancelled,
}

pub struct FolderPicker {
    pub purpose: PickerPurpose,
    current: PathBuf,
    entries: Vec<PathBuf>,
    /// Barre de chemin éditable (saut direct).
    path_edit: String,
    show_hidden: bool,
    error: Option<String>,
    /// Dossier pour lequel `entries` est valide (recharge si ça change).
    loaded_for: Option<PathBuf>,
    /// Sous-dossier surligné pour la navigation clavier.
    selected: usize,
}

impl FolderPicker {
    pub fn new(purpose: PickerPurpose, start: PathBuf) -> Self {
        let current = first_existing_ancestor(&start);
        Self {
            purpose,
            path_edit: current.display().to_string(),
            current,
            entries: Vec::new(),
            show_hidden: false,
            error: None,
            loaded_for: None,
            selected: 0,
        }
    }

    fn navigate_to(&mut self, dir: PathBuf) {
        self.current = dir;
        self.path_edit = self.current.display().to_string();
        self.loaded_for = None; // forcera le rechargement
        self.selected = 0;
    }

    fn ensure_loaded(&mut self) {
        if self.loaded_for.as_deref() == Some(self.current.as_path()) {
            return;
        }
        match list_subdirs(&self.current, self.show_hidden) {
            Ok(dirs) => {
                self.entries = dirs;
                self.error = None;
            }
            Err(e) => {
                self.entries.clear();
                self.error = Some(e);
            }
        }
        self.loaded_for = Some(self.current.clone());
    }

    pub fn show(&mut self, ctx: &egui::Context) -> PickerOutcome {
        self.ensure_loaded();
        let mut outcome = PickerOutcome::Browsing;
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            return PickerOutcome::Cancelled;
        }

        // Navigation clavier (quand aucun champ de saisie n'a le focus) :
        //   ↑/↓ déplacent la sélection, Entrée entre dans le dossier surligné,
        //   Backspace/← remontent, Ctrl+Entrée choisit le dossier courant.
        if ctx.memory(|m| m.focused().is_none()) {
            let (up, down, enter, back, ctrl_enter) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
                        || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter),
                )
            });
            if ctrl_enter {
                return PickerOutcome::Selected(self.current.clone());
            }
            if back {
                if let Some(parent) = self.current.parent() {
                    self.navigate_to(parent.to_path_buf());
                }
            }
            if !self.entries.is_empty() {
                if up {
                    self.selected = self.selected.saturating_sub(1);
                }
                if down {
                    self.selected = (self.selected + 1).min(self.entries.len() - 1);
                }
                if enter {
                    self.navigate_to(self.entries[self.selected].clone());
                }
            }
        }

        let title = match self.purpose {
            PickerPurpose::OpenProject => "Ouvrir un projet — choisis son dossier",
            PickerPurpose::NewProjectLocation => "Nouveau projet — choisis l'emplacement",
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // Barre de chemin : remonter + saisie directe.
                ui.horizontal(|ui| {
                    if ui.button("⬆").on_hover_text("Dossier parent").clicked() {
                        if let Some(parent) = self.current.parent() {
                            self.navigate_to(parent.to_path_buf());
                        }
                    }
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.path_edit)
                            .desired_width(380.0)
                            .hint_text("/home/toi/romans  (« ~ » accepté)"),
                    );
                    let go = ui.button("Aller").clicked()
                        || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go {
                        let target = expand_tilde(&self.path_edit);
                        if target.is_dir() {
                            self.navigate_to(target);
                        } else {
                            self.error = Some(format!("'{}' n'est pas un dossier.", target.display()));
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.show_hidden, "dossiers cachés").changed() {
                        self.loaded_for = None;
                    }
                });

                if let Some(err) = &self.error {
                    ui.colored_label(egui::Color32::from_rgb(255, 120, 120), err);
                }

                ui.separator();

                // Liste des sous-dossiers.
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.entries.is_empty() && self.error.is_none() {
                            ui.weak("(aucun sous-dossier)");
                        }
                        let mut enter: Option<PathBuf> = None;
                        for (i, dir) in self.entries.iter().enumerate() {
                            let name = dir
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let resp = ui.selectable_label(i == self.selected, format!("📁 {name}"));
                            if i == self.selected && resp.clicked() {
                                // 2e clic sur la ligne déjà surlignée = on entre.
                                enter = Some(dir.clone());
                            } else if resp.clicked() {
                                self.selected = i;
                            }
                            if resp.double_clicked() {
                                enter = Some(dir.clone());
                            }
                        }
                        if let Some(dir) = enter {
                            self.navigate_to(dir);
                        }
                    });

                ui.separator();
                ui.label(format!("Dossier : {}", self.current.display()));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let choose = match self.purpose {
                        PickerPurpose::OpenProject => "Ouvrir ce dossier",
                        PickerPurpose::NewProjectLocation => "Créer le projet ici",
                    };
                    if ui.button(choose).clicked() {
                        outcome = PickerOutcome::Selected(self.current.clone());
                    }
                    if ui.button("Annuler").clicked() {
                        outcome = PickerOutcome::Cancelled;
                    }
                });
                ui.weak("↑↓ naviguer · Entrée entrer · ⌫ remonter · Ctrl+Entrée choisir · Échap annuler");
            });

        outcome
    }
}

/// Sous-dossiers de `dir`, triés par nom, dotfiles optionnels.
fn list_subdirs(dir: &Path, show_hidden: bool) -> Result<Vec<PathBuf>, String> {
    let rd = std::fs::read_dir(dir)
        .map_err(|e| format!("Impossible de lire {} : {e}.", dir.display()))?;
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            show_hidden
                || !p
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
        })
        .collect();
    dirs.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .cmp(&b.file_name().unwrap_or_default().to_ascii_lowercase())
    });
    Ok(dirs)
}

/// Premier ancêtre existant de `start` (au cas où le chemin mémorisé a
/// disparu) ; à défaut, le home, à défaut la racine.
fn first_existing_ancestor(start: &Path) -> PathBuf {
    let mut cur = Some(start);
    while let Some(p) = cur {
        if p.is_dir() {
            return p.to_path_buf();
        }
        cur = p.parent();
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Étend un « ~ » de tête en répertoire personnel.
pub fn expand_tilde(input: &str) -> PathBuf {
    let s = input.trim();
    if s == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(s));
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_sorts_subdirs_skipping_hidden() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        std::fs::create_dir(root.join("Zeta"))?;
        std::fs::create_dir(root.join("alpha"))?;
        std::fs::create_dir(root.join(".caché"))?;
        std::fs::write(root.join("fichier.typ"), "x")?;

        let visible = list_subdirs(root, false)?;
        let names: Vec<String> = visible
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["alpha".to_string(), "Zeta".to_string()]); // triés, pas de fichier, pas de caché

        let all = list_subdirs(root, true)?;
        assert_eq!(all.len(), 3); // le dossier caché réapparaît
        Ok(())
    }

    #[test]
    fn first_existing_ancestor_walks_up() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let deep = tmp.path().join("a/b/c_inexistant");
        let got = first_existing_ancestor(&deep);
        assert_eq!(got, tmp.path()); // a/b n'existent pas → remonte au tmp
        Ok(())
    }

    #[test]
    fn tilde_expansion() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expand_tilde("~"), home);
            assert_eq!(expand_tilde("~/romans"), home.join("romans"));
        }
        assert_eq!(expand_tilde("/abs/olu"), PathBuf::from("/abs/olu"));
    }
}
