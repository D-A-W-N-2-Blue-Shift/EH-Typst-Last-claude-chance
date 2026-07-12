// ============================================================================
// core/src/config.rs — Registre des modules et chargement de `engram.ron`
//
// Le binaire (app/) enregistre des factories par nom. Le fichier
// ~/.config/engram_hive/engram.ron décide quels modules sont instanciés.
// Le core lui-même ne contient AUCUN nom de module en dur.
// ============================================================================

use crate::module_api::Module;

/// Section `modules` de `engram.ron` : la LISTE des modules activés.
///
/// `rename = "Modules"` est CRITIQUE : le fichier sérialisé commence par
/// `Modules(...)` et RON exige que ce nom corresponde au nom serde de la
/// struct à la relecture.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename = "Modules", default)]
pub struct ModulesConfig {
    /// Noms des modules à charger au lancement.
    pub enabled: Vec<String>,
}

impl Default for ModulesConfig {
    fn default() -> Self {
        Self {
            enabled: vec![
                "file_tree".into(),
                "editor".into(),
                "cockpit".into(),
                "sticky_notes".into(),
            ],
        }
    }
}

impl ModulesConfig {
    /// Charge la section `modules` de `engram.ron`.
    ///
    /// Résilient : section absente ou illisible ⇒ base EH5 par défaut,
    /// avec la colonne vertébrale conservée.
    pub fn load_from_licorne(licorne: &crate::licorne::Licorne, registry: &ModuleRegistry) -> Self {
        let mut errors = Vec::new();
        let mut cfg: Self = licorne.section("modules", &mut errors);
        for e in errors {
            tracing::warn!("{e}");
        }
        if cfg.enabled.is_empty() {
            cfg = Self::default();
        }
        let registered: std::collections::HashSet<&'static str> =
            registry.registered_names().collect();
        let mut enabled = Vec::new();
        for spine in ["file_tree", "editor", "cockpit"] {
            if registered.contains(spine) {
                enabled.push(spine.to_string());
            }
        }
        for name in cfg.enabled {
            if registered.contains(name.as_str()) && !enabled.iter().any(|n| n == &name) {
                enabled.push(name);
            }
        }
        if enabled.is_empty() {
            enabled = registry.registered_names().map(String::from).collect();
        }
        Self { enabled }
    }
}

type ModuleFactory = fn() -> Box<dyn Module>;

/// Registre nom → factory. Rempli par le binaire, consommé par le core.
#[derive(Default)]
pub struct ModuleRegistry {
    factories: Vec<(&'static str, ModuleFactory)>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre une factory. Appelé par app/src/main.rs uniquement.
    pub fn register(&mut self, name: &'static str, factory: ModuleFactory) {
        self.factories.push((name, factory));
    }

    pub fn registered_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.factories.iter().map(|(n, _)| *n)
    }

    /// Instancie les modules activés dans modules.ron, dans l'ordre du fichier.
    /// Un nom inconnu n'est pas fatal : on logue et on continue.
    pub fn instantiate(&self, cfg: &ModulesConfig) -> Vec<Box<dyn Module>> {
        let mut out = Vec::new();
        for name in &cfg.enabled {
            match self.factories.iter().find(|(n, _)| n == name) {
                Some((_, factory)) => out.push(factory()),
                None => tracing::warn!(
                    "modules.ron demande le module '{name}' mais aucune factory \
                     n'est enregistrée sous ce nom. Je l'ignore."
                ),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module_api::{CoreContext, Module, ModuleResponse};

    struct Dummy;
    impl Module for Dummy {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn init(&mut self, _ctx: &CoreContext) -> Result<(), String> {
            Ok(())
        }
        fn update(&mut self, _ctx: &egui::Context, _out: &mut Vec<ModuleResponse>) {}
    }

    /// La forme EXACTE écrite sur le disque doit se relire. Ce test aurait
    /// attrapé le bug « Expected struct `ModulesConfig` but found `Modules` ».
    #[test]
    fn parses_on_disk_struct_name() -> Result<(), Box<dyn std::error::Error>> {
        let cfg: ModulesConfig = ron::from_str("Modules(enabled: [\"a\", \"b\"])")?;
        assert_eq!(cfg.enabled, vec!["a".to_string(), "b".to_string()]);
        Ok(())
    }

    /// Le scénario du bug devient : la section `modules` relue doit rester
    /// stable et conserver la colonne vertébrale.
    #[test]
    fn load_from_licorne_keeps_spine() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("engram.ron"),
            r#"{ "modules": Modules(enabled: ["cockpit", "sticky_notes"]) }"#,
        )?;
        let (licorne, _) = crate::licorne::Licorne::load(dir.path());
        let mut reg = ModuleRegistry::new();
        reg.register("file_tree", || Box::new(Dummy));
        reg.register("editor", || Box::new(Dummy));
        reg.register("cockpit", || Box::new(Dummy));
        reg.register("sticky_notes", || Box::new(Dummy));
        let c = ModulesConfig::load_from_licorne(&licorne, &reg);
        assert_eq!(
            c.enabled,
            vec![
                "file_tree".to_string(),
                "editor".to_string(),
                "cockpit".to_string(),
                "sticky_notes".to_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn default_modules_section_includes_sticky_notes() {
        let cfg = ModulesConfig::default();
        assert_eq!(
            cfg.enabled,
            vec![
                "file_tree".to_string(),
                "editor".to_string(),
                "cockpit".to_string(),
                "sticky_notes".to_string()
            ]
        );
    }

    #[test]
    fn unknown_modules_are_filtered_out() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("engram.ron"),
            r#"{ "modules": Modules(enabled: ["editor", "ghost"]) }"#,
        )?;
        let (licorne, _) = crate::licorne::Licorne::load(dir.path());
        let mut reg = ModuleRegistry::new();
        reg.register("editor", || Box::new(Dummy));
        let c = ModulesConfig::load_from_licorne(&licorne, &reg);
        assert_eq!(c.enabled, vec!["editor".to_string()]);
        Ok(())
    }
}
