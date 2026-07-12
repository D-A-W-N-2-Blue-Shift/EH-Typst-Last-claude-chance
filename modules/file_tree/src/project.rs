// ============================================================================
// modules/file_tree/src/project.rs — Ouverture/création de projet, workspace
//
// Un projet Engram_Hive = un dossier racine avec la structure standardisée
// « Wingate » (1_atelier … 8_archives, voir STANDARD_DIRS) + un .engram/ local
// (index.db, session.ron). Les projets historiques (01_architecture …
// 06_session) restent pleinement pris en charge (rétro-compatibilité).
//
// Règles :
//   - Le choix de dossier passe par le sélecteur intégré (folder_picker) ;
//     le module ne demande qu'un NOM de projet/fichier dans les dialogues.
//   - La config du projet (goal global) vit HORS du projet, dans
//     ~/.config/engram_hive/projects/<nom>/project.ron — le dossier projet
//     reste du Typst 100% portable (un `mv` suffit pour migrer).
//   - Le dernier projet ouvert est mémorisé dans
//     ~/.config/engram_hive/modules/file_tree/last_project.ron.
// ============================================================================

use std::path::{Path, PathBuf};

use engram_core::atomic_write;

/// Config légère du projet, stockée hors du dossier projet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Project {
    pub name: String,
    /// Goal global affiché à la racine du tree. 0 = pas de goal, pas d'affichage.
    pub goal_words: u64,
}

/// Dossiers de l'arborescence standardisée (§2.1 du brief).
// Structure de projet standardisée (spec architecte « Wingate », 2026-07-04).
// Les projets historiques (01_architecture … 06_session) restent reconnus par
// le file_tree, symlinks et stats — aucune régression (voir modules concernés).
const STANDARD_DIRS: &[&str] = &[
    // 1_atelier : espace de travail actif. `liens/` est la table de liens
    // (symlinks vers les fichiers en cours, ex-06_session) ; `sticky/` les
    // notes autocollantes.
    "1_atelier/sticky",
    "1_atelier/liens",
    "2_todo",
    "3_plan/chapitrages",
    "3_plan/chronologie",
    "3_plan/revelations",
    "3_plan/systemes",
    "3_plan/archives_plan",
    "4_fiches/personnages",
    "4_fiches/lieux",
    "4_fiches/objets",
    "4_fiches/concepts",
    "4_fiches/entites",
    "4_fiches/organisations",
    "4_fiches/documents_internes",
    "4_fiches/fragments_lore",
    "5_scenes/standby",
    "6_chapitres/chapitrage",
    "6_chapitres/manuscrit_full",
    "7_notes/inbox",
    "7_notes/a_trier",
    "7_notes/triees",
    "7_notes/intuitions",
    "7_notes/documentation",
    "8_archives/scenes_coupees",
    "8_archives/anciens_chapitres",
    "8_archives/anciens_manuscrits_full",
    "8_archives/anciens_plans",
    "8_archives/anciennes_fiches",
    "8_archives/vrac_historique",
];

/// Fichiers par défaut créés avec un nouveau projet (jamais écrasés).
const STANDARD_FILES: &[(&str, &str)] = &[
    (
        "1_atelier/scene_active.typ",
        "/*\n---\nstatut: brouillon\ngoal: 1500\ntags:\n  - atelier\n---\n*/\n\n= Scène active\n\nLa scène sur laquelle tu travailles maintenant. Écris.\n",
    ),
    (
        "1_atelier/chapitre_actif.typ",
        "/*\n---\ntags:\n  - atelier\n---\n*/\n\n= Chapitre actif\n\nLe chapitre en cours d'assemblage.\n",
    ),
    (
        "1_atelier/notes_actives.typ",
        "/*\n---\ntags:\n  - atelier\n---\n*/\n\n= Notes actives\n\nNotes volatiles de la session en cours.\n",
    ),
    (
        "3_plan/chronologie/frise_principale.typ",
        "/*\n---\ntags:\n  - chronologie\n---\n*/\n\n= Frise principale\n\nLes événements dans l'ordre du monde, pas du récit.\n",
    ),
    (
        "5_scenes/scene_1.typ",
        "/*\n---\nstatut: brouillon\ngoal: 1500\n---\n*/\n\n= Scène 1\n\nPremière scène. Écris.\n",
    ),
    (
        "7_notes/index_tags.typ",
        "/*\n---\ntags:\n  - index\n---\n*/\n\n= Index des tags\n\nRecense ici tes tags récurrents (#climax, #mystere, ...).\n",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectState {
    Empty,
    NonEngram,
    Partial { missing: Vec<String> },
    Engram,
}

/// Vérifie si un dossier est déjà un projet Engram_Hive (.engram/ présent).
pub fn is_engram_project(root: &Path) -> bool {
    root.join(".engram").is_dir()
}

pub fn detect_project_state(root: &Path) -> ProjectState {
    if is_engram_project(root) {
        return ProjectState::Engram;
    }
    let mut has_visible = false;
    let mut present_standard = Vec::new();
    for dir in STANDARD_DIRS {
        if root.join(dir).exists() {
            present_standard.push(*dir);
        }
    }
    if let Ok(rd) = std::fs::read_dir(root) {
        for entry in rd.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            has_visible = true;
            break;
        }
    }
    if !has_visible && present_standard.is_empty() {
        return ProjectState::Empty;
    }
    if present_standard.is_empty() {
        ProjectState::NonEngram
    } else if present_standard.len() < STANDARD_DIRS.len() {
        let missing = STANDARD_DIRS
            .iter()
            .filter(|dir| !root.join(dir).exists())
            .map(|s| s.to_string())
            .collect();
        ProjectState::Partial { missing }
    } else {
        ProjectState::Engram
    }
}

pub fn structure_plan() -> Vec<String> {
    let mut out = Vec::new();
    for d in STANDARD_DIRS {
        out.push(format!("dossier: {}", d));
    }
    for (rel, _) in STANDARD_FILES {
        out.push(format!("fichier: {}", rel));
    }
    out.push("dossier: .engram".into());
    out
}

/// Crée un nouveau projet : dossier <location>/<name> + structure complète.
pub fn create_project(location: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Un projet sans nom ? Non. Donne-lui un nom.".into());
    }
    if name.contains('/') || name.contains('\0') {
        return Err(format!(
            "'{name}' n'est pas un nom de dossier valide. Pas de '/' dans un nom de projet."
        ));
    }
    let root = location.join(name);
    if is_engram_project(&root) {
        return Err(format!(
            "'{}' est déjà un projet Engram_Hive. Ouvre-le au lieu de le recréer, \
             je ne vais pas écraser ton travail.",
            root.display()
        ));
    }
    scaffold(&root)?;
    Ok(root)
}

/// Génère la structure standardisée dans `root` (création de projet, ou
/// dossier existant sans .engram/ traité comme nouveau projet).
/// N'écrase JAMAIS un fichier existant.
pub fn scaffold(root: &Path) -> Result<(), String> {
    for d in STANDARD_DIRS {
        let p = root.join(d);
        std::fs::create_dir_all(&p)
            .map_err(|e| format!("Impossible de créer {} : {e}", p.display()))?;
    }
    for (rel, content) in STANDARD_FILES {
        let p = root.join(rel);
        if !p.exists() {
            atomic_write(&p, content.as_bytes())
                .map_err(|e| format!("Impossible d'écrire {} : {e}", p.display()))?;
        }
    }
    let engram = root.join(".engram");
    std::fs::create_dir_all(&engram)
        .map_err(|e| format!("Impossible de créer {} : {e}", engram.display()))?;
    Ok(())
}

/// Charge la config projet (goal global) depuis
/// ~/.config/engram_hive/projects/<nom>/project.ron. Absente = goal 0, pas d'erreur.
pub fn load_project_config(config_dir: &Path, root: &Path) -> Project {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let path = config_dir.join("projects").join(&name).join("project.ron");
    let mut proj = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| ron::from_str::<Project>(&raw).ok())
        .unwrap_or_default();
    if proj.name.is_empty() {
        proj.name = name;
    }
    proj
}

// ---------------------------------------------------------------------------
// Mémoire du dernier projet ouvert (pour reopen_last_project).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct LastProject {
    path: String,
}

fn last_project_file(module_config_dir: &Path) -> PathBuf {
    module_config_dir.join("last_project.ron")
}

pub fn save_last_project(module_config_dir: &Path, root: &Path) {
    let body = format!(
        "// Dernier projet ouvert — géré automatiquement par le file tree.\n\
         LastProject(\n    path: {:?},\n)\n",
        root.display().to_string()
    );
    if let Err(e) = atomic_write(&last_project_file(module_config_dir), body.as_bytes()) {
        tracing::warn!("Impossible de mémoriser le dernier projet : {e}");
    }
}

pub fn load_last_project(module_config_dir: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(last_project_file(module_config_dir)).ok()?;
    let lp: LastProject = ron::from_str(&raw).ok()?;
    let p = PathBuf::from(lp.path);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Opérations fichiers/dossiers du tree (clic droit, palette, CLI).
// ---------------------------------------------------------------------------

/// Crée un fichier .typ dans `parent` avec un bloc de métadonnées minimal.
pub fn new_file(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Un fichier sans nom, vraiment ?".into());
    }
    let name = if name.contains('.') {
        name.to_string()
    } else {
        format!("{name}.typ")
    };
    let path = parent.join(&name);
    if path.exists() {
        return Err(format!(
            "'{name}' existe déjà dans ce dossier. Deux fichiers du même nom au même \
             endroit, ton filesystem refuse et moi aussi."
        ));
    }
    atomic_write(
        &path,
        "/*\n---\nstatut: brouillon\n---\n*/\n\n= Nouveau document\n\n".as_bytes(),
    )
    .map_err(|e| format!("Impossible de créer {} : {e}", path.display()))?;
    Ok(path)
}

pub fn new_folder(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Un dossier sans nom, vraiment ?".into());
    }
    let path = parent.join(name);
    if path.exists() {
        return Err(format!("'{name}' existe déjà ici."));
    }
    std::fs::create_dir(&path)
        .map_err(|e| format!("Impossible de créer le dossier {} : {e}", path.display()))?;
    Ok(path)
}

pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Renommer en rien ? Utilise Supprimer si c'est ce que tu veux.".into());
    }
    let dest = path
        .parent()
        .ok_or("Ce chemin n'a pas de parent. Étrange.")?
        .join(new_name);
    if dest.exists() {
        return Err(format!(
            "'{new_name}' existe déjà à côté. Choisis un autre nom."
        ));
    }
    std::fs::rename(path, &dest)
        .map_err(|e| format!("Impossible de renommer {} : {e}", path.display()))?;
    Ok(dest)
}

/// Supprime un fichier ou dossier. La confirmation (si dossier non vide)
/// est gérée côté UI — ici on exécute.
pub fn delete(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
            .map_err(|e| format!("Impossible de supprimer {} : {e}", path.display()))
    } else {
        std::fs::remove_file(path)
            .map_err(|e| format!("Impossible de supprimer {} : {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_cree_la_structure_wingate() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        scaffold(root)?;

        // Les 8 racines de la structure Wingate + .engram.
        for d in [
            "1_atelier",
            "1_atelier/liens",
            "1_atelier/sticky",
            "2_todo",
            "3_plan/chronologie",
            "4_fiches/personnages",
            "5_scenes/standby",
            "6_chapitres/manuscrit_full",
            "7_notes/inbox",
            "8_archives/vrac_historique",
            ".engram",
        ] {
            assert!(root.join(d).is_dir(), "dossier manquant : {d}");
        }
        // Fichiers actifs amorcés.
        assert!(root.join("1_atelier/scene_active.typ").is_file());
        assert!(root
            .join("3_plan/chronologie/frise_principale.typ")
            .is_file());
        // Un dossier scaffoldé est bien reconnu comme projet Engram.
        assert!(is_engram_project(root));
        Ok(())
    }

    #[test]
    fn scaffold_necrase_pas_un_fichier_existant() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        std::fs::create_dir_all(root.join("1_atelier"))?;
        std::fs::write(root.join("1_atelier/scene_active.typ"), "MON TRAVAIL")?;
        scaffold(root)?;
        // Le contenu existant est préservé (jamais écrasé).
        assert_eq!(
            std::fs::read_to_string(root.join("1_atelier/scene_active.typ"))?,
            "MON TRAVAIL"
        );
        Ok(())
    }
}
