// ============================================================================
// modules/nexus_hub/src/tree.rs — Arbre de navigation (doc §5.1)
//
// Lecture directe du filesystem à chaque frame, pas d'index séparé : la
// structure d'un projet Nexus est petite et fixe (5 dossiers standard, doc
// §3), contrairement au corpus potentiellement énorme que file_tree (côté
// écrivain) doit indexer — son mécanisme scan+DB+watcher serait disproportionné
// ici. `.engram/` (détail d'implémentation, pas contenu utilisateur) est
// délibérément exclu.
// ============================================================================

use std::path::{Path, PathBuf};

pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

const STANDARD_DIRS: &[&str] = &[
    "01_journal",
    "02_sante",
    "03_todo",
    "04_articles",
    "05_reference",
];

/// Construit l'arbre à partir des dossiers standard présents (doc §3). Un
/// dossier standard absent (projet pas encore scaffoldé, ou ouvert avant
/// que le scaffold complet existe) est simplement omis — pas une erreur.
pub fn build(root: &Path) -> Vec<TreeNode> {
    STANDARD_DIRS
        .iter()
        .filter_map(|name| {
            let path = root.join(name);
            path.is_dir().then(|| build_node(name, &path))
        })
        .collect()
}

fn build_node(name: &str, path: &Path) -> TreeNode {
    let mut children = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let child_path = entry.path();
            let child_name = entry.file_name().to_string_lossy().into_owned();
            if child_path.is_dir() {
                children.push(build_node(&child_name, &child_path));
            } else {
                children.push(TreeNode {
                    name: child_name,
                    path: child_path,
                    is_dir: false,
                    children: Vec::new(),
                });
            }
        }
    }
    TreeNode {
        name: name.to_string(),
        path: path.to_path_buf(),
        is_dir: true,
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_omet_les_dossiers_standard_absents() {
        let tmp = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(tmp.path().join("01_journal")).expect("mkdir");
        let nodes = build(tmp.path());
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "01_journal");
    }

    #[test]
    fn build_liste_les_fichiers_et_sous_dossiers_tries() {
        let tmp = tempfile::tempdir().expect("tmp");
        let journal = tmp.path().join("01_journal");
        std::fs::create_dir_all(journal.join("2026")).expect("mkdir");
        std::fs::write(journal.join("2026").join("2026-07-14.typst"), "").expect("write");
        std::fs::write(journal.join("2026").join("2026-07-01.typst"), "").expect("write");
        let nodes = build(tmp.path());
        let year_node = &nodes[0].children[0];
        assert_eq!(year_node.name, "2026");
        assert!(year_node.is_dir);
        assert_eq!(year_node.children.len(), 2);
        // Triés par nom : 2026-07-01 avant 2026-07-14.
        assert_eq!(year_node.children[0].name, "2026-07-01.typst");
        assert!(!year_node.children[0].is_dir);
    }

    #[test]
    fn build_ignore_engram_qui_nest_pas_un_dossier_standard() {
        let tmp = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(tmp.path().join(".engram")).expect("mkdir");
        std::fs::write(tmp.path().join(".engram").join("nexus.db"), "").expect("write");
        let nodes = build(tmp.path());
        assert!(nodes.is_empty());
    }

    #[test]
    fn build_vide_si_aucun_dossier_standard() {
        let tmp = tempfile::tempdir().expect("tmp");
        assert!(build(tmp.path()).is_empty());
    }
}
