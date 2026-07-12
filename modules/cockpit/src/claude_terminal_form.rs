// ============================================================================
// modules/cockpit/src/claude_terminal_form.rs — Formulaire config COH2B / provider CLI
//
// Édite coh2b.ron (auth_mode + api_key) sans dépendre de la crate
// provider CLI (cockpit reste découplé). Types locaux miroirs.
// ============================================================================

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Provider {
    #[default]
    Claude,
    Codex,
    Gemini,
}

impl Provider {
    pub const ALL: &'static [Self] = &[Self::Claude, Self::Codex, Self::Gemini];

    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
        }
    }

    pub fn defaults(self) -> (&'static str, &'static str) {
        match self {
            Self::Claude => (
                "claude",
                "--print --output-format json --no-input --max-turns 1",
            ),
            Self::Codex => ("codex", "--json"),
            Self::Gemini => ("gemini", "--json"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum AuthMode {
    #[default]
    Subscription,
    ApiKey,
}

impl AuthMode {
    pub const ALL: &'static [Self] = &[Self::Subscription, Self::ApiKey];

    pub fn label(self) -> &'static str {
        match self {
            Self::Subscription => "Abonnement",
            Self::ApiKey => "Clé API",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[serde(rename = "ClaudeTerminalConfig")]
pub struct Draft {
    pub provider: Provider,
    pub command: String,
    pub args: String,
    pub auth_mode: AuthMode,
    pub api_key: Option<String>,
}

impl Default for Draft {
    fn default() -> Self {
        let provider = Provider::default();
        let (command, args) = provider.defaults();
        Self {
            provider,
            command: command.to_string(),
            args: args.to_string(),
            auth_mode: AuthMode::Subscription,
            api_key: None,
        }
    }
}

pub fn load(licorne: &engram_core::Licorne) -> (Draft, Vec<String>) {
    let mut errors = Vec::new();
    let draft: Draft = licorne.section("coh2b", &mut errors);
    if draft == Draft::default() {
        let legacy: Draft = licorne.section("claude_terminal", &mut Vec::new());
        if legacy != Draft::default() {
            return (legacy, errors);
        }
    }
    (draft, errors)
}

pub fn save(config_dir: &Path, draft: &Draft) -> Result<std::path::PathBuf, String> {
    let path = config_dir.join("coh2b.ron");
    let body = format!(
        "// Configuration du provider CLI.\n\
             // provider : Claude, Codex ou Gemini.\n\
             // command : binaire réellement invoqué.\n\
             // args : arguments passés au binaire.\n\
             // auth_mode : Subscription (login interactif) ou ApiKey (clé API).\n\
             // api_key : renseignée uniquement si auth_mode = ApiKey.\n\
             {}\n",
        ron::ser::to_string_pretty(draft, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("Sérialisation RON : {e}"))?
    );
    std::fs::write(&path, body).map_err(|e| format!("Écriture {} : {e}", path.display()))?;
    Ok(path)
}

pub fn serialize_ron(draft: &Draft) -> String {
    ron::ser::to_string_pretty(draft, ron::ser::PrettyConfig::default()).unwrap_or_default()
}

pub fn draw(ui: &mut egui::Ui, draft: &mut Draft) {
    let provider_before = draft.provider;
    ui.label("Provider CLI :");
    egui::ComboBox::from_id_salt("ct_provider")
        .selected_text(draft.provider.label())
        .show_ui(ui, |ui| {
            for provider in Provider::ALL {
                ui.selectable_value(&mut draft.provider, *provider, provider.label());
            }
        });
    if draft.provider != provider_before {
        let (command, args) = draft.provider.defaults();
        draft.command = command.to_string();
        draft.args = args.to_string();
    }

    ui.label("Binaire CLI :");
    ui.text_edit_singleline(&mut draft.command);

    ui.label("Arguments :");
    ui.text_edit_singleline(&mut draft.args);

    ui.add_space(8.0);

    ui.label("Mode d'authentification :");
    egui::ComboBox::from_id_salt("ct_auth_mode")
        .selected_text(draft.auth_mode.label())
        .show_ui(ui, |ui| {
            for mode in AuthMode::ALL {
                ui.selectable_value(&mut draft.auth_mode, *mode, mode.label());
            }
        });

    ui.add_space(8.0);

    match draft.auth_mode {
        AuthMode::Subscription => {
            ui.weak(
                "Utilise le login interactif du provider CLI choisi. Aucune clé API \
                 nécessaire.",
            );
        }
        AuthMode::ApiKey => {
            ui.label("Clé API Anthropic :");
            let key = draft.api_key.get_or_insert_with(String::new);
            ui.add(
                egui::TextEdit::singleline(key)
                    .password(true)
                    .hint_text("sk-ant-...")
                    .desired_width(400.0),
            );
            ui.add_space(4.0);
            ui.weak("Stockée dans coh2b.ron. Non chiffrée.");
        }
    }
}
