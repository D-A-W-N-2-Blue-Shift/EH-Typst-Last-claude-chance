// ============================================================================
// modules/nexus_hub/src/project.rs — Arborescence projet Nexus (doc §3)
//
// Structure de référence (Markdown-first, brief 15/07/2026) :
//   01_journal/, 02_sante/{medicaments.md,notes_sante.md},
//   03_todo/{backlog.md,archive/}, 04_articles/, 05_reference/, .engram/
//
// Les fichiers amorcés (medicaments.md, notes_sante.md, backlog.md) sont
// créés VIDES : leur contenu relève des modules health/todo — inventer leur
// forme ici serait une donnée non demandée (§7.6 YAGNI, §A2 exactitude).
// Un projet hérité qui porte encore les variantes .typst les garde telles
// quelles, sans compagnon .md automatique.
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

/// Fichiers amorcés en Markdown (brief 15/07/2026 : Markdown-first). Chaque
/// entrée porte aussi son équivalent hérité `.typst` : un projet d'avant le
/// fallback qui repasse par « Nouveau » ne doit PAS recevoir un compagnon
/// `.md` à côté de son `.typst` existant.
const STANDARD_FILES: &[(&str, &str)] = &[
    ("02_sante/medicaments.md", "02_sante/medicaments.typst"),
    ("02_sante/notes_sante.md", "02_sante/notes_sante.typst"),
    ("03_todo/backlog.md", "03_todo/backlog.typst"),
];

/// Génère la structure standardisée dans `root`. N'écrase jamais un fichier
/// existant (ni ne double un `.typst` hérité par un `.md`). Crée aussi
/// `.engram/`, mais PAS `nexus.db` lui-même : c'est `nexus_db::open_db` qui
/// en est l'unique source de vérité du schéma.
pub fn scaffold(root: &Path) -> Result<(), String> {
    for d in STANDARD_DIRS {
        let p = root.join(d);
        std::fs::create_dir_all(&p)
            .map_err(|e| format!("Impossible de créer {} : {e}", p.display()))?;
    }
    for (rel, legacy_rel) in STANDARD_FILES {
        let p = root.join(rel);
        if !p.exists() && !root.join(legacy_rel).exists() {
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
            "02_sante/medicaments.md",
            "02_sante/notes_sante.md",
            "03_todo/backlog.md",
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
        std::fs::write(root.join("02_sante/medicaments.md"), "MON REGISTRE")?;
        scaffold(root)?;
        assert_eq!(
            std::fs::read_to_string(root.join("02_sante/medicaments.md"))?,
            "MON REGISTRE"
        );
        Ok(())
    }

    #[test]
    fn scaffold_ne_double_pas_un_typst_herite_par_un_md() -> Result<(), Box<dyn std::error::Error>>
    {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        std::fs::create_dir_all(root.join("02_sante"))?;
        std::fs::write(root.join("02_sante/notes_sante.typst"), "notes héritées")?;
        scaffold(root)?;
        // Le .typst hérité est intact, AUCUN notes_sante.md compagnon créé.
        assert_eq!(
            std::fs::read_to_string(root.join("02_sante/notes_sante.typst"))?,
            "notes héritées"
        );
        assert!(!root.join("02_sante/notes_sante.md").exists());
        // Les autres fichiers, sans hérité, sont bien créés en .md.
        assert!(root.join("03_todo/backlog.md").is_file());
        Ok(())
    }

    #[test]
    fn is_nexus_project_faux_avant_scaffold() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        assert!(!is_nexus_project(tmp.path()));
        Ok(())
    }
}
