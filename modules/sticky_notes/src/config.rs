use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub closed: bool,
    pub list_filter_tag_type: String,
    pub list_filter_file: String,
    pub list_filter_text: String,
    pub sort_mode: SortMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            closed: true,
            list_filter_tag_type: String::new(),
            list_filter_file: String::new(),
            list_filter_text: String::new(),
            sort_mode: SortMode::DateDesc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SortMode {
    DateDesc,
    DateAsc,
    File,
    TagType,
}

impl Default for SortMode {
    fn default() -> Self {
        Self::DateDesc
    }
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::DateDesc => "Date décroissante",
            Self::DateAsc => "Date croissante",
            Self::File => "Fichier",
            Self::TagType => "Type de tag",
        }
    }

    pub const ALL: &'static [Self] = &[Self::DateDesc, Self::DateAsc, Self::File, Self::TagType];
}

impl Config {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir
            .join("modules")
            .join("sticky_notes")
            .join("sticky_notes.ron")
    }

    pub fn load(config_dir: &Path) -> (Self, Vec<String>) {
        let path = Self::path(config_dir);
        match std::fs::read_to_string(&path) {
            Ok(raw) => match ron::from_str::<Self>(&raw) {
                Ok(cfg) => (cfg, Vec::new()),
                Err(e) => (
                    Self::default(),
                    vec![format!(
                        "{} illisible ({e}) ; sticky notes fermé par défaut.",
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
            .map_err(|e| format!("Sérialisation sticky_notes.ron : {e}"))?;
        std::fs::write(&path, body).map_err(|e| format!("Écriture {} : {e}", path.display()))?;
        Ok(path)
    }
}
