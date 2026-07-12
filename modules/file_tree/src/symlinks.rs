// ============================================================================
// modules/file_tree/src/symlinks.rs — Gestion de 06_en_cours/
//
// 06_en_cours/ est une table de travail virtuelle : des symlinks vers les
// originaux, jamais de copies. Modifier le lien = modifier l'original.
//
// Règles de sécurité :
//   - On ne supprime JAMAIS autre chose qu'un symlink depuis en_cours/
//     (vérification via symlink_metadata avant remove_file).
//   - Le fichier .context maintient un mapping lisible lien → cible relative.
//
// Linux uniquement en Phase 1 (std::os::unix::fs::symlink).
// ============================================================================

use std::path::{Path, PathBuf};

pub const EN_COURS: &str = "06_en_cours";
/// §6 du brief 2/7/2026 : le nouveau nom canonique du dossier de session
/// pour les projets créés à partir de maintenant. Les deux noms sont
/// reconnus par le file_tree (coloration symlinks, exclusion des stats).
pub const SESSION: &str = "06_session";
/// Structure « Wingate » (2026-07-04) : la table de liens vit dans
/// `1_atelier/liens/`. Le dossier `liens/` porte la même sémantique que
/// l'ancien dossier de session (symlinks vers les fichiers en cours).
pub const LIENS: &str = "liens";

/// Retourne vrai si le composant de chemin est reconnu comme dossier de
/// session/liens — ancien nom (`06_en_cours/`), `06_session/` (§6), ou
/// `liens/` (structure Wingate, sous `1_atelier/`).
pub fn is_session_folder(name: &str) -> bool {
    name == EN_COURS || name == SESSION || name == LIENS
}

fn en_cours_dir(root: &Path) -> PathBuf {
    // Priorité structure Wingate : 1_atelier/liens/ si l'atelier existe.
    let atelier = root.join("1_atelier");
    if atelier.is_dir() {
        return atelier.join(LIENS);
    }
    // Sinon 06_session/ (nouveau nom historique) puis 06_en_cours/ (ancien).
    let new = root.join(SESSION);
    if new.exists() {
        return new;
    }
    root.join(EN_COURS)
}

/// Ajoute un fichier (ou dossier) à En Cours : crée un symlink absolu.
pub fn add_to_en_cours(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let dir = en_cours_dir(root);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Impossible de créer {} : {e}", dir.display()))?;

    let target = std::fs::canonicalize(target)
        .map_err(|e| format!("Cible introuvable ({}) : {e}", target.display()))?;
    let name = target
        .file_name()
        .ok_or("Cette cible n'a pas de nom de fichier. Je passe.")?;
    let link = dir.join(name);
    if link.exists() || std::fs::symlink_metadata(&link).is_ok() {
        return Err(format!(
            "Impossible de créer le symlink vers '{}' : fichier déjà présent dans \
             En Cours. Un exemplaire suffit, non ?",
            name.to_string_lossy()
        ));
    }
    std::os::unix::fs::symlink(&target, &link)
        .map_err(|e| format!("Impossible de créer le symlink {} : {e}", link.display()))?;
    rewrite_context(root)?;
    Ok(link)
}

/// Retire un lien de En Cours. Refuse catégoriquement si ce n'est pas un symlink.
pub fn remove_from_en_cours(root: &Path, link: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(link)
        .map_err(|e| format!("Lien introuvable ({}) : {e}", link.display()))?;
    if !meta.file_type().is_symlink() {
        return Err(
            "Ce n'est pas un symlink. Je refuse de supprimer un vrai fichier depuis \
             En Cours. Vérifie l'icône 🔗 avant de cliquer."
                .into(),
        );
    }
    std::fs::remove_file(link)
        .map_err(|e| format!("Impossible de supprimer le lien {} : {e}", link.display()))?;
    rewrite_context(root)?;
    Ok(())
}

/// Vide En Cours : retire tous les symlinks, ne touche à rien d'autre.
pub fn clear_en_cours(root: &Path) -> Result<usize, String> {
    let dir = en_cours_dir(root);
    let mut removed = 0;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.filter_map(|e| e.ok()) {
            let p = entry.path();
            if let Ok(meta) = std::fs::symlink_metadata(&p) {
                if meta.file_type().is_symlink() {
                    std::fs::remove_file(&p).map_err(|e| {
                        format!("Impossible de supprimer le lien {} : {e}", p.display())
                    })?;
                    removed += 1;
                }
            }
        }
    }
    rewrite_context(root)?;
    Ok(removed)
}

/// Résout l'original pointé par un lien de En Cours (chemin canonique).
pub fn resolve_original(link: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(link).map_err(|e| {
        format!(
            "Impossible de résoudre l'original de {} : {e}. Le lien est peut-être \
             cassé — la cible a déménagé sans prévenir.",
            link.display()
        )
    })
}

/// Réécrit le fichier .context : mapping lisible lien → cible relative au projet.
/// Format : "scene_1.typ -> ../05_texte/partie_1/chapitre_1/scene_1.typ"
pub fn rewrite_context(root: &Path) -> Result<(), String> {
    let dir = en_cours_dir(root);
    let mut lines: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.filter_map(|e| e.ok()) {
            let p = entry.path();
            let meta = match std::fs::symlink_metadata(&p) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !meta.file_type().is_symlink() {
                continue;
            }
            let target = std::fs::read_link(&p).unwrap_or_default();
            // Cible affichée relative à la racine projet quand possible.
            let display = target
                .strip_prefix(root)
                .map(|rel| format!("../{}", rel.display()))
                .unwrap_or_else(|_| target.display().to_string());
            lines.push(format!(
                "{} -> {}",
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                display
            ));
        }
    }
    lines.sort();
    let path = dir.join(".context");
    engram_core::atomic_write(&path, (lines.join("\n") + "\n").as_bytes())
        .map_err(|e| format!("Impossible d'écrire {} : {e}", path.display()))
}
