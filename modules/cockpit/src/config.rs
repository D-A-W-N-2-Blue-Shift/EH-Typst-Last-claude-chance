use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub closed: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { closed: true }
    }
}

impl Config {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir
            .join("modules")
            .join("cockpit")
            .join("cockpit.ron")
    }

    pub fn load(config_dir: &Path) -> (Self, Vec<String>) {
        let path = Self::path(config_dir);
        match std::fs::read_to_string(&path) {
            Ok(raw) => match ron::from_str::<Self>(&raw) {
                Ok(cfg) => (cfg, Vec::new()),
                Err(e) => (
                    Self::default(),
                    vec![format!(
                        "{} illisible ({e}) ; cockpit fermé par défaut.",
                        path.display()
                    )],
                ),
            },
            Err(_) => (Self::default(), Vec::new()),
        }
    }

    pub fn save(&self, config_dir: &Path) -> Result<PathBuf, String> {
        let path = Self::path(config_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Impossible de créer {} : {e}", parent.display()))?;
        }
        let body = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("Sérialisation cockpit.ron : {e}"))?;
        std::fs::write(&path, body).map_err(|e| format!("Écriture {} : {e}", path.display()))?;
        Ok(path)
    }
}
