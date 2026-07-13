// ============================================================================
// modules/nexus_hub/src/project.rs — Arborescence projet Nexus (doc §3)
//
// Structure de référence :
//   01_journal/, 02_sante/{medicaments.typst,notes_sante.typst},
//   03_todo/{backlog.typst,archive/}, 04_articles/, 05_reference/, .engram/
//
// Les fichiers amorcés (medicaments.typst, notes_sante.typst, backlog.typst)
// sont créés VIDES : leur contenu (frontmatter YAML, structure) relève des
// modules health/todo (§5.3/§5.4), pas encore écrits — inventer leur forme
// ici serait une donnée non demandée (§7.6 YAGNI, §A2 exactitude).
//
// N'écrase JAMAIS un fichier existant (même règle que file_tree::project,
// modules/file_tree/src/project.rs).
// ============================================================================

use std::path::Path;

const STANDARD_DIRS: &[&str] = &[
    "01_journal",
    "02_sante",
    "03_todo/archive",
    "04_articles",
    "05_reference",
];

const STANDARD_FILES: &[&str] = &[
    "02_sante/medicaments.typst",
    "02_sante/notes_sante.typst",
    "03_todo/backlog.typst",
];

/// Génère la structure standardisée dans `root`. N'écrase jamais un fichier
/// existant. Crée aussi `.engram/`, mais PAS `nexus.db` lui-même : c'est
/// `nexus_db::open_db` qui en est l'unique source de vérité du schéma.
pub fn scaffold(root: &Path) -> Result<(), String> {
    for d in STANDARD_DIRS {
        let p = root.join(d);
        std::fs::create_dir_all(&p)
            .map_err(|e| format!("Impossible de créer {} : {e}", p.display()))?;
    }
    for rel in STANDARD_FILES {
        let p = root.join(rel);
        if !p.exists() {
            std::fs::write(&p, "")
                .map_err(|e| format!("Impossible de créer {} : {e}", p.display()))?;
        }
    }
    let engram = root.join(".engram");
    std::fs::create_dir_all(&engram)
        .map_err(|e| format!("Impossible de créer {} : {e}", engram.display()))?;
    Ok(())
}

/// Un `.engram/` présent signale un projet Nexus déjà initialisé.
pub fn is_nexus_project(root: &Path) -> bool {
    root.join(".engram").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_cree_la_structure_doc_3() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        scaffold(root)?;
        for d in [
            "01_journal",
            "02_sante",
            "03_todo/archive",
            "04_articles",
            "05_reference",
            ".engram",
        ] {
            assert!(root.join(d).is_dir(), "dossier manquant : {d}");
        }
        for f in [
            "02_sante/medicaments.typst",
            "02_sante/notes_sante.typst",
            "03_todo/backlog.typst",
        ] {
            assert!(root.join(f).is_file(), "fichier manquant : {f}");
        }
        assert!(is_nexus_project(root));
        Ok(())
    }

    #[test]
    fn scaffold_necrase_pas_un_fichier_existant() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        std::fs::create_dir_all(root.join("02_sante"))?;
        std::fs::write(root.join("02_sante/medicaments.typst"), "MON REGISTRE")?;
        scaffold(root)?;
        assert_eq!(
            std::fs::read_to_string(root.join("02_sante/medicaments.typst"))?,
            "MON REGISTRE"
        );
        Ok(())
    }

    #[test]
    fn is_nexus_project_faux_avant_scaffold() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        assert!(!is_nexus_project(tmp.path()));
        Ok(())
    }
}
