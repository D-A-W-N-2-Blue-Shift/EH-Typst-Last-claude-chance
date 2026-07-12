// ============================================================================
// modules/file_tree/src/layout.rs — Session de fenêtres locale
//
// La session du projet garde uniquement l'état régénérable utile au module :
// positions et tailles des fenêtres locales. Aucun mode d'attache, aucune
// logique de fusion de fenêtres.
// ============================================================================

use std::path::{Path, PathBuf};

use engram_core::atomic_write;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WindowSession {
    pub id: String,
    pub position: (f32, f32),
    pub size: (f32, f32),
}

impl Default for WindowSession {
    fn default() -> Self {
        Self {
            id: String::new(),
            position: (100.0, 100.0),
            size: (280.0, 900.0),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct SessionConfig {
    pub windows: Vec<WindowSession>,
}

fn session_path(root: &Path) -> PathBuf {
    root.join(".engram").join("session.ron")
}

impl SessionConfig {
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(session_path(root))
            .ok()
            .and_then(|raw| ron::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        let pretty = ron::ser::PrettyConfig::new();
        let body = ron::ser::to_string_pretty(self, pretty)
            .map_err(|e| format!("Sérialisation session.ron impossible : {e}"))?;
        let path = session_path(root);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        atomic_write(&path, body.as_bytes())
            .map_err(|e| format!("Impossible d'écrire {} : {e}", path.display()))
    }
}
