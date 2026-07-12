// ============================================================================
// modules/claude_terminal/src/config.rs — Configuration du provider CLI
//
// Section dédiée `claude_terminal` dans `engram.ron` pour isoler la clé API
// et le choix de provider.
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum AssistantCliProvider {
    #[default]
    Claude,
    Codex,
    Gemini,
}

impl AssistantCliProvider {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ScopeMode {
    #[default]
    CurrentFile,
    SelectedText,
    Corpus,
    Project,
}

impl ScopeMode {
    pub const ALL: &'static [Self] = &[
        Self::CurrentFile,
        Self::SelectedText,
        Self::Corpus,
        Self::Project,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CurrentFile => "Fichier courant",
            Self::SelectedText => "Texte sélectionné",
            Self::Corpus => "Corpus",
            Self::Project => "Projet complet",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum AnalysisMode {
    #[default]
    Question,
    Micro,
    Macro,
    Narrative,
    Audit,
}

impl AnalysisMode {
    pub const ALL: &'static [Self] = &[
        Self::Question,
        Self::Micro,
        Self::Macro,
        Self::Narrative,
        Self::Audit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Question => "Question libre",
            Self::Micro => "Cohérence micro",
            Self::Macro => "Cohérence macro",
            Self::Narrative => "Piste narrative",
            Self::Audit => "Audit de modification",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[serde(rename = "ClaudeTerminalConfig")]
pub struct ClaudeTerminalConfig {
    pub provider: AssistantCliProvider,
    pub command: String,
    pub args: String,
    pub auth_mode: AuthMode,
    pub api_key: Option<String>,
    pub scope: ScopeMode,
    pub analysis_mode: AnalysisMode,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

impl Default for ClaudeTerminalConfig {
    fn default() -> Self {
        let provider = AssistantCliProvider::default();
        let (command, args) = provider.defaults();
        Self {
            provider,
            command: command.to_string(),
            args: args.to_string(),
            auth_mode: AuthMode::Subscription,
            api_key: None,
            scope: ScopeMode::CurrentFile,
            analysis_mode: AnalysisMode::Question,
            input_cost_per_1k: None,
            output_cost_per_1k: None,
        }
    }
}

impl ClaudeTerminalConfig {
    pub fn load(licorne: &engram_core::Licorne) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let cfg: Self = licorne.section("coh2b", &mut errors);
        if cfg == Self::default() {
            let legacy: Self = licorne.section("claude_terminal", &mut Vec::new());
            if legacy != Self::default() {
                return (legacy, errors);
            }
        }
        (cfg, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_provider_command_and_args() {
        let cfg = ClaudeTerminalConfig::default();
        assert_eq!(cfg.provider, AssistantCliProvider::Claude);
        assert_eq!(cfg.command, "claude");
        assert!(cfg.args.contains("--output-format"));
    }

    #[test]
    fn roundtrip_keeps_multicharacter_command() -> Result<(), Box<dyn std::error::Error>> {
        let cfg = ClaudeTerminalConfig {
            provider: AssistantCliProvider::Gemini,
            command: "gemini-cli".into(),
            args: "--json --model gemini-2.5-pro".into(),
            auth_mode: AuthMode::Subscription,
            api_key: Some("abc123".into()),
            scope: ScopeMode::Project,
            analysis_mode: AnalysisMode::Macro,
            input_cost_per_1k: Some(0.0),
            output_cost_per_1k: Some(0.0),
        };
        let raw = ron::ser::to_string_pretty(&cfg, ron::ser::PrettyConfig::default())?;
        let parsed: ClaudeTerminalConfig = ron::from_str(&raw)?;
        assert_eq!(parsed.command, "gemini-cli");
        assert!(parsed.args.contains("gemini-2.5-pro"));
        assert_eq!(parsed.provider, AssistantCliProvider::Gemini);
        assert_eq!(parsed.scope, ScopeMode::Project);
        assert_eq!(parsed.analysis_mode, AnalysisMode::Macro);
        Ok(())
    }
}
