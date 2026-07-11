// ============================================================================
// modules/file_tree/src/context_menu.rs — Menus clic droit contextuels
//
// Trois contextes (§6.1 du brief) :
//   - dossier : nouveau fichier/dossier, renommer, supprimer, ajouter à En Cours
//   - fichier .typ : ouvrir, renommer, supprimer, ajouter à En Cours, copier chemin
//   - élément de 06_en_cours/ : ouvrir, retirer du contexte, aller à l'original
//
// Ce fichier ne fait que CONSTRUIRE les menus et retourner l'action choisie.
// L'exécution (et les confirmations) vivent dans lib.rs.
// ============================================================================

use std::path::PathBuf;

use crate::stats::TreeNode;

/// Action choisie dans un menu contextuel.
#[derive(Debug, Clone)]
pub enum MenuAction {
    Open(PathBuf),
    NewFileIn(PathBuf),
    NewFolderIn(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
    AddToEnCours(PathBuf),
    CopyPath(PathBuf),
    RemoveFromEnCours(PathBuf),
    GoToOriginal(PathBuf),
}

/// Construit le menu adapté au nœud. Appelé depuis tree_view dans le
/// closure de `Response::context_menu`.
pub fn build(ui: &mut egui::Ui, node: &TreeNode, inside_en_cours: bool) -> Option<MenuAction> {
    let mut action = None;
    let p = || node.path.clone();

    if inside_en_cours && !node.is_en_cours {
        // Élément DANS 06_en_cours/ (symlink).
        if ui.button("Ouvrir").clicked() {
            action = Some(MenuAction::Open(p()));
        }
        if ui.button("Retirer du contexte").clicked() {
            action = Some(MenuAction::RemoveFromEnCours(p()));
        }
        if ui.button("Aller au fichier original").clicked() {
            action = Some(MenuAction::GoToOriginal(p()));
        }
    } else if node.is_dir {
        if ui.button("Nouveau fichier").clicked() {
            action = Some(MenuAction::NewFileIn(p()));
        }
        if ui.button("Nouveau dossier").clicked() {
            action = Some(MenuAction::NewFolderIn(p()));
        }
        ui.separator();
        if ui.button("Renommer").clicked() {
            action = Some(MenuAction::Rename(p()));
        }
        if ui.button("Supprimer").clicked() {
            action = Some(MenuAction::Delete(p()));
        }
        ui.separator();
        if ui.button("Ajouter à En Cours").clicked() {
            action = Some(MenuAction::AddToEnCours(p()));
        }
    } else {
        if ui.button("Ouvrir").clicked() {
            action = Some(MenuAction::Open(p()));
        }
        ui.separator();
        if ui.button("Renommer").clicked() {
            action = Some(MenuAction::Rename(p()));
        }
        if ui.button("Supprimer").clicked() {
            action = Some(MenuAction::Delete(p()));
        }
        ui.separator();
        if ui.button("Ajouter à En Cours").clicked() {
            action = Some(MenuAction::AddToEnCours(p()));
        }
        if ui.button("Copier le chemin").clicked() {
            action = Some(MenuAction::CopyPath(p()));
        }
    }

    if action.is_some() {
        ui.close_menu();
    }
    action
}
