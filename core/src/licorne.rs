// ============================================================================
// core/src/licorne.rs — Configuration UNIFIÉE (Hive_RBMK.ron)
//
// Un seul fichier ~/.config/hive_rbmk_tcherenkov/Hive_RBMK.ron est LA
// configuration de l'application — demande explicite de l'architecte :
// « un seul et unique fichier de configuration ron ». Il regroupe TOUTES
// les options, une SECTION par module, plus les sections globales :
//
//   {
//       "basic": (...),
//       "deep": (...),
//       "modules": (...),
//       "providers": (...),
//       "theme": ThemeVomi(...),
//       "editor": EditorVomi(...),
//       ...
//   }
//
// Le core ne connaît PAS les types experts des modules (EditorVomi, etc.) :
// il dépendrait alors des crates modules, ce que l'architecture interdit.
// On stocke donc chaque section BRUTE (ron::Value) et chaque module
// désérialise la sienne dans son propre type via `section::<T>()`.
//
// Format : un map RON `{ "nom": ... }` (et non un struct nommé) — c'est la
// seule forme que RON sait extraire générique­ment ET qui tolère des sections
// de modules inconnus du core sans les perdre.
//
// Tolérance : fichier absent ⇒ tout aux défauts. Syntaxe globale cassée ⇒
// message GLaDOS + tout aux défauts. Section illisible ⇒ défauts pour CE
// module seulement. Jamais de panique.
// ============================================================================

use std::collections::HashMap;
use std::path::Path;

use crate::atomic_write;
use serde::de::DeserializeOwned;

/// Nom du fichier de configuration unique dans le dossier de config.
pub const CONFIG_FILE: &str = "Hive_RBMK.ron";
/// Ancien nom (héritage Engram) : migré automatiquement au premier
/// lancement post-renommage pour ne perdre AUCUN réglage existant.
const LEGACY_CONFIG_FILE: &str = "engram.ron";
/// Modèle commenté écrit au premier lancement si `Hive_RBMK.ron` manque.
const DEFAULT_CONFIG_RON: &str = include_str!("../../config/Hive_RBMK.ron");

/// Sections expertes parsées, indexées par nom de module.
#[derive(Debug, Clone, Default)]
pub struct Licorne {
    sections: HashMap<String, ron::Value>,
}

impl Licorne {
    /// Charge `Hive_RBMK.ron` depuis le dossier de config. Absent ⇒ un
    /// éventuel `engram.ron` hérité est migré (renommé) pour préserver les
    /// réglages existants, sinon le fichier est créé avec le modèle commenté
    /// du dépôt. Retourne (config, erreurs GLaDOS à loguer/afficher).
    pub fn load(config_dir: &Path) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let path = config_dir.join(CONFIG_FILE);
        if !path.exists() {
            let legacy = config_dir.join(LEGACY_CONFIG_FILE);
            if legacy.exists() {
                match std::fs::rename(&legacy, &path) {
                    Ok(()) => errors.push(format!(
                        "Config migrée : {} → {} (réglages conservés).",
                        legacy.display(),
                        path.display()
                    )),
                    Err(e) => errors.push(format!(
                        "Migration de {} impossible ({e}) : je repars du modèle \
                         par défaut, tes anciens réglages restent dans ce fichier.",
                        legacy.display()
                    )),
                }
            }
        }
        if !path.exists() {
            if let Err(e) = atomic_write(&path, DEFAULT_CONFIG_RON.as_bytes()) {
                errors.push(format!(
                    "Impossible de créer {} : {e}. Je repars sur tous les défauts.",
                    path.display()
                ));
                return (Self::default(), errors);
            }
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!(
                    "{} est illisible ({e}). Je repars sur tous les défauts.",
                    path.display()
                ));
                return (Self::default(), errors);
            }
        };
        match ron::from_str::<HashMap<String, ron::Value>>(&raw) {
            Ok(sections) => (Self { sections }, errors),
            Err(e) => {
                errors.push(format!(
                    "{} : syntaxe cassée ({e}). C'est le fichier expert : si tu \
                     l'édites, assume. Je repars sur TOUS les défauts en attendant.",
                    path.display()
                ));
                (Self::default(), errors)
            }
        }
    }

    /// Désérialise la section `name` dans le type expert `T`. Absente ⇒ défaut
    /// silencieux (cas normal : on ne configure que ce qui intéresse). Présente
    /// mais incompatible ⇒ défaut + message GLaDOS pour ce module seul.
    pub fn section<T: DeserializeOwned + Default>(
        &self,
        name: &str,
        errors: &mut Vec<String>,
    ) -> T {
        let Some(val) = self.sections.get(name) else {
            return T::default();
        };
        match val.clone().into_rust::<T>() {
            Ok(cfg) => cfg,
            Err(e) => {
                errors.push(format!(
                    "Section '{name}' de {} illisible ({e}). Valeurs par défaut \
                     pour ce module.",
                    CONFIG_FILE
                ));
                T::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Default, PartialEq)]
    struct EditorVomi {
        #[serde(default)]
        zoom_min_pt: f32,
        #[serde(default)]
        smart: bool,
    }

    fn write_licorne(dir: &Path, body: &str) -> std::io::Result<()> {
        crate::atomic_write(&dir.join(CONFIG_FILE), body.as_bytes()).map_err(std::io::Error::other)
    }

    #[test]
    fn absent_file_gives_empty_no_error() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let (lic, errs) = Licorne::load(dir.path());
        assert!(errs.is_empty());
        let mut e = Vec::new();
        let ed: EditorVomi = lic.section("editor", &mut e);
        assert_eq!(ed, EditorVomi::default());
        assert!(e.is_empty());
        Ok(())
    }

    #[test]
    fn named_inner_struct_and_missing_fields() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        write_licorne(
            dir.path(),
            r#"{ "editor": EditorVomi(zoom_min_pt: 8.0, smart: true) }"#,
        )?;
        let (lic, errs) = Licorne::load(dir.path());
        assert!(errs.is_empty(), "{errs:?}");
        let mut e = Vec::new();
        let ed: EditorVomi = lic.section("editor", &mut e);
        assert_eq!(
            ed,
            EditorVomi {
                zoom_min_pt: 8.0,
                smart: true
            }
        );
        assert!(e.is_empty());
        Ok(())
    }

    #[test]
    fn unknown_module_section_is_preserved() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        write_licorne(
            dir.path(),
            r#"{ "editor": (smart: true), "demo": (foo: 42) }"#,
        )?;
        let (lic, _) = Licorne::load(dir.path());
        assert!(lic.sections.contains_key("demo"));
        Ok(())
    }

    #[test]
    fn broken_syntax_falls_back_with_glados() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        write_licorne(dir.path(), "{ this is not ron")?;
        let (lic, errs) = Licorne::load(dir.path());
        assert!(!errs.is_empty());
        assert!(lic.sections.is_empty());
        Ok(())
    }
}
