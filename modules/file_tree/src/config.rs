// ============================================================================
// modules/file_tree/src/config.rs — Chargement de la config double RON
//
// Config simple par module + config experte UNIFIÉE :
//   - section "file_tree" de ~/.config/engram_hive/engram.ron : simple,
//     valeurs essentielles
//   - section "file_tree_expert" de ~/.config/engram_hive/engram.ron : expert
//
// Règle : commenter une ligne dans un RON = revenir à la valeur par défaut
// (tous les champs sont #[serde(default)]).
// ============================================================================

use std::path::{Path, PathBuf};

/// Config simple, user-facing (section `file_tree` de `engram.ron`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileTree {
    pub default_width: u32,
    pub show_word_counts: bool,
    pub show_status_indicators: bool,
    pub show_symlink_icons: bool,
    pub font_size: u32,
    pub reopen_last_project: bool,
}

impl Default for FileTree {
    fn default() -> Self {
        Self {
            default_width: 280,
            show_word_counts: true,
            show_status_indicators: true,
            show_symlink_icons: true,
            font_size: 13,
            reopen_last_project: true,
        }
    }
}

/// Config experte, optionnelle (section `file_tree_expert` de `engram.ron`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileTreeVomi {
    // --- Indexeur ---
    pub notify_debounce_ms: u64,
    pub scan_parallelism: usize,
    pub extra_extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    // --- Affichage ---
    pub indent_size: u32,
    pub row_height: u32,
    pub folder_color: String,
    pub symlink_color: String,
    pub en_cours_symlink_color: String,
    pub goal_pending_color: String,
    pub goal_reached_color: String,
    // --- Stats ---
    pub stats_unit: String,
    pub abbreviate_counts: bool,
    pub abbreviate_threshold: u64,
}

impl Default for FileTreeVomi {
    fn default() -> Self {
        Self {
            notify_debounce_ms: 500,
            scan_parallelism: 4,
            extra_extensions: Vec::new(),
            exclude_dirs: Vec::new(),
            indent_size: 16,
            row_height: 22,
            folder_color: "#aa00ff".to_string(),
            symlink_color: String::new(),
            en_cours_symlink_color: "#ff55ff".to_string(),
            goal_pending_color: String::new(),
            goal_reached_color: String::new(),
            stats_unit: "words".to_string(),
            abbreviate_counts: true,
            abbreviate_threshold: 1000,
        }
    }
}

/// Les deux configs fusionnées, telles qu'utilisées par le module au runtime.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub simple: FileTree,
    pub vomi: FileTreeVomi,
    /// Dossier ~/.config/engram_hive/modules/file_tree/
    pub module_config_dir: PathBuf,
}

impl Config {
    /// Charge les sections `file_tree` et `file_tree_expert` de `engram.ron`.
    /// Retourne (config, erreurs GLaDOS à afficher).
    pub fn load(config_dir: &Path, licorne: &engram_core::Licorne) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let dir = config_dir.join("modules").join("file_tree");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            errors.push(format!(
                "Impossible de créer {} : {e}. Je continue avec les valeurs par défaut, \
                 mais rien ne sera persisté. À toi de voir.",
                dir.display()
            ));
        }

        let simple: FileTree = licorne.section("file_tree", &mut errors);
        let vomi: FileTreeVomi = licorne.section("file_tree_expert", &mut errors);

        (
            Self {
                simple,
                vomi,
                module_config_dir: dir,
            },
            errors,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn shipped_file_tree_section_parses() -> Result<(), Box<dyn std::error::Error>> {
        let sections: HashMap<String, ron::Value> =
            ron::from_str(include_str!("../../../config/engram.ron"))?;
        let simple: FileTree = sections
            .get("file_tree")
            .ok_or("section file_tree absente")?
            .clone()
            .into_rust()?;
        assert_eq!(simple.font_size, 13);
        let vomi: FileTreeVomi = sections
            .get("file_tree_expert")
            .ok_or("section file_tree_expert absente")?
            .clone()
            .into_rust()?;
        assert_eq!(vomi.notify_debounce_ms, 500);
        assert_eq!(vomi.stats_unit, "words");
        Ok(())
    }
}
