// ============================================================================
// modules/health/src/notes.rs — Observations libres (doc §3 : notes_sante)
//
// Un seul fichier par projet, pas un par jour comme le journal.
// Markdown-first (brief 15/07/2026) : la référence est
// `02_sante/notes_sante.md` ; un `notes_sante.typst` hérité (projet d'avant
// le fallback) est utilisé TEL QUEL s'il existe et qu'aucun `.md` n'existe —
// jamais converti, jamais renommé. Si les deux existent, le `.md` gagne.
//
// Écart documenté identique à journal/articles (README_MODULE.md respectifs) :
// édition via `TextEdit::multiline`, pas le moteur ropey/coloration complet
// d'`editor` (constructeur privé, inaccessible hors crate).
// ============================================================================

use std::path::{Path, PathBuf};

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

fn md_path(project_root: &Path) -> PathBuf {
    project_root.join("02_sante").join("notes_sante.md")
}

fn legacy_path(project_root: &Path) -> PathBuf {
    project_root.join("02_sante").join("notes_sante.typst")
}

/// Fichier de notes effectif du projet : `.md` prioritaire, `.typst` hérité
/// respecté, `.md` pour toute création.
pub fn file_path(project_root: &Path) -> PathBuf {
    let md = md_path(project_root);
    if md.exists() {
        return md;
    }
    let legacy = legacy_path(project_root);
    if legacy.exists() {
        return legacy;
    }
    md
}

/// Clé d'indexation plein-texte : le chemin RÉEL du fichier édité, relatif à
/// la racine (`02_sante/notes_sante.md` ou `.typst` hérité).
pub fn fts_key(project_root: &Path) -> String {
    file_path(project_root)
        .strip_prefix(project_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "02_sante/notes_sante.md".to_string())
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

/// Le fichier de notes effectif est-il un `.typst` hérité ?
pub fn is_legacy(project_root: &Path) -> bool {
    file_path(project_root)
        .extension()
        .is_some_and(|e| e == "typst")
}

/// Brief Phase 6 — copie Markdown explicite des notes héritées. Convertit
/// `content` (le buffer courant), écrit `notes_sante.md` + le rapport, ne
/// touche JAMAIS au `.typst`. Après cet appel, `file_path()` résout sur le
/// `.md` (priorité Markdown-first). Refuse si le `.md` existe déjà.
pub fn create_md_copy(project_root: &Path, content: &str) -> Result<PathBuf, String> {
    let src = legacy_path(project_root);
    let dst = md_path(project_root);
    if dst.exists() {
        return Err(format!("{} existe déjà — copie refusée.", dst.display()));
    }
    let out = engram_core::typst_fallback::typst_to_md_safe(content);
    engram_core::atomic_write(&dst, out.markdown.as_bytes())?;
    let report_path = src.with_extension("typ-to-md-report.md");
    let report =
        engram_core::typst_fallback::conversion_report("notes_sante.typst", "notes_sante.md", &out);
    engram_core::atomic_write(&report_path, report.as_bytes())?;
    Ok(dst)
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

    #[test]
    fn creation_en_markdown_par_defaut() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        save(tmp.path(), "note")?;
        assert!(md_path(tmp.path()).is_file(), "création attendue en .md");
        assert!(!legacy_path(tmp.path()).exists());
        Ok(())
    }

    #[test]
    fn typst_herite_utilise_tel_quel_sans_conversion() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        std::fs::create_dir_all(tmp.path().join("02_sante"))?;
        std::fs::write(legacy_path(tmp.path()), "notes héritées")?;
        // Lecture : c'est bien l'hérité qui est servi.
        assert_eq!(load(tmp.path())?, "notes héritées");
        assert_eq!(file_path(tmp.path()), legacy_path(tmp.path()));
        assert_eq!(fts_key(tmp.path()), "02_sante/notes_sante.typst");
        // Écriture : elle va dans l'hérité (édition texte simple, doc du
        // brief phase 4), AUCUN .md créé en silence.
        save(tmp.path(), "notes héritées éditées")?;
        assert!(!md_path(tmp.path()).exists());
        assert_eq!(
            std::fs::read_to_string(legacy_path(tmp.path()))?,
            "notes héritées éditées"
        );
        Ok(())
    }

    #[test]
    fn md_prioritaire_si_les_deux_existent() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        std::fs::create_dir_all(tmp.path().join("02_sante"))?;
        std::fs::write(legacy_path(tmp.path()), "hérité")?;
        std::fs::write(md_path(tmp.path()), "markdown")?;
        assert_eq!(load(tmp.path())?, "markdown");
        assert_eq!(fts_key(tmp.path()), "02_sante/notes_sante.md");
        Ok(())
    }

    #[test]
    fn copie_markdown_preserve_le_typst_et_genere_le_rapport(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        std::fs::create_dir_all(tmp.path().join("02_sante"))?;
        let original = "= Notes\n\nObservation.\n";
        std::fs::write(legacy_path(tmp.path()), original)?;
        assert!(is_legacy(tmp.path()));

        let dst = create_md_copy(tmp.path(), original).map_err(std::io::Error::other)?;
        // Original intact à l'octet près, copie convertie, rapport présent.
        assert_eq!(std::fs::read_to_string(legacy_path(tmp.path()))?, original);
        assert!(std::fs::read_to_string(&dst)?.contains("# Notes"));
        assert!(legacy_path(tmp.path())
            .with_extension("typ-to-md-report.md")
            .is_file());
        // Résolution basculée sur le .md, refus de ré-écraser.
        assert!(!is_legacy(tmp.path()));
        assert!(create_md_copy(tmp.path(), "x").is_err());
        Ok(())
    }
}
