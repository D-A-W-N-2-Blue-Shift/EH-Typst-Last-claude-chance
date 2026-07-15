// ============================================================================
// modules/health/src/notes.rs — Observations libres (doc §3 : notes_sante)
//
// Un seul fichier par projet (`02_sante/notes_sante.typst`), pas un par jour
// comme le journal. Le scaffold du hub (modules/nexus_hub/src/project.rs) le
// crée déjà vide à la création du projet — ce module est ce qui le lit et
// l'écrit réellement, ce qui manquait jusqu'ici (aucun module n'y touchait).
//
// Écart documenté identique à journal/articles (README_MODULE.md respectifs) :
// édition via `TextEdit::multiline`, pas le moteur ropey/coloration complet
// d'`editor` (constructeur privé, inaccessible hors crate).
// ============================================================================

use std::path::{Path, PathBuf};

/// Chemin relatif utilisé comme clé dans `fts_content.file_path` (doc §3) —
/// un seul fichier notes_sante par projet, donc une clé fixe plutôt qu'un
/// slug généré comme journal/articles.
pub const FTS_KEY: &str = "02_sante/notes_sante.typst";

pub struct NotesState {
    pub content: String,
    /// `true` une fois le contenu initial lu depuis le disque pour le projet
    /// courant — évite de recharger (et d'écraser une saisie en cours) à
    /// chaque frame.
    pub loaded: bool,
    /// `true` si `content` a changé depuis le dernier chargement/sauvegarde
    /// réussie — pilote l'affichage "Enregistré" vs modifications en attente.
    pub dirty: bool,
}

impl Default for NotesState {
    fn default() -> Self {
        Self {
            content: String::new(),
            loaded: false,
            dirty: false,
        }
    }
}

pub fn file_path(project_root: &Path) -> PathBuf {
    project_root.join("02_sante").join("notes_sante.typst")
}

/// Lit le fichier s'il existe ; renvoie une chaîne vide s'il n'existe pas
/// encore (le scaffold le crée vide, mais un projet ouvert sans passer par
/// « Nouveau » peut ne pas l'avoir).
pub fn load(project_root: &Path) -> Result<String, String> {
    let path = file_path(project_root);
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Lecture de {} impossible : {e}", path.display()))
}

pub fn save(project_root: &Path, content: &str) -> Result<(), String> {
    let path = file_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Création de {} impossible : {e}", parent.display()))?;
    }
    std::fs::write(&path, content)
        .map_err(|e| format!("Écriture de {} impossible : {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_renvoie_vide_si_fichier_absent() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        assert_eq!(load(tmp.path())?, "");
        Ok(())
    }

    #[test]
    fn save_puis_load_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        save(tmp.path(), "Observation libre du jour.")?;
        assert_eq!(load(tmp.path())?, "Observation libre du jour.");
        Ok(())
    }

    #[test]
    fn save_cree_le_dossier_02_sante_si_absent() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        assert!(!tmp.path().join("02_sante").exists());
        save(tmp.path(), "contenu")?;
        assert!(file_path(tmp.path()).is_file());
        Ok(())
    }

    #[test]
    fn save_ecrase_le_contenu_precedent() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        save(tmp.path(), "premier")?;
        save(tmp.path(), "second")?;
        assert_eq!(load(tmp.path())?, "second");
        Ok(())
    }
}
