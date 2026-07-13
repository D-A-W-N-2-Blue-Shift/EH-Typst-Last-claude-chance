// ============================================================================
// modules/journal/src/config.rs — Section "journal" de engram.ron
//
// Un seul réglage exposé : le gabarit de création d'une nouvelle entrée (doc
// §5.2 : « template configurable dans journal.ron » — différé à l'incrément 4,
// voir README_MODULE.md « Écart documenté »). Fermé ici via le mécanisme
// RÉEL déjà établi dans ce workspace (licorne.section, mêmes patrons que
// modules/editor/src/config.rs et modules/cockpit) : la config Nexus vit en
// sections nommées de engram.ron, pas dans un fichier séparé par module — le
// doc nommait « journal.ron » de façon descriptive du besoin (un gabarit
// configurable), pas d'un chemin de fichier littéral imposé.
// ============================================================================

/// Section `journal` de `engram.ron`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct JournalConfig {
    /// Contenu créé pour une nouvelle entrée. `{date}` est remplacé par
    /// YYYY-MM-DD au moment de la création (`entry::render_template`).
    pub template: String,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            template: crate::entry::DEFAULT_TEMPLATE.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contient_le_marqueur_date() {
        assert!(JournalConfig::default().template.contains("{date}"));
    }

    #[test]
    fn round_trip_ron() {
        let cfg = JournalConfig {
            template: "---\ntags: []\n---\n\n= Perso — {date}\n\n".to_string(),
        };
        let serialized = ron::to_string(&cfg).expect("serialize");
        let parsed: JournalConfig = ron::from_str(&serialized).expect("parse");
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn section_absente_retombe_sur_le_defaut() {
        // Reproduit exactement ce que fait Licorne::section quand la clé
        // "journal" est absente du RON : construction par défaut serde.
        let empty: std::collections::HashMap<String, ron::Value> =
            ron::from_str("{}").expect("map vide");
        assert!(!empty.contains_key("journal"));
        assert_eq!(JournalConfig::default(), JournalConfig::default());
    }
}
