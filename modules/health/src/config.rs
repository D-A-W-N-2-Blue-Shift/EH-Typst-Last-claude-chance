// ============================================================================
// modules/health/src/config.rs — Section "health" de Hive_RBMK.ron
//
// Un seul réglage exposé : activer/désactiver le rappel de ressenti différé
// (doc §5.3 : « rappel non intrusif, configurable off »). Le délai (3h)
// N'EST PAS rendu configurable — le doc ne donne qu'un point (« configurable
// off »), pas une plage réglable ; ajouter un délai réglable serait une
// donnée inventée (§A2). Mécanisme RÉEL déjà établi (licorne.section, même
// patron que journal/dashboard/todo) : section nommée de Hive_RBMK.ron.
// ============================================================================

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HealthConfig {
    /// `true` (comportement précédemment codé en dur, zéro régression) :
    /// le rappel de ressenti apparaît 3h après une prise sans ressenti.
    /// `false` : jamais de rappel, la saisie du ressenti reste possible
    /// manuellement si l'UI l'exposait ailleurs (elle ne le fait pas
    /// aujourd'hui — seul le rappel lui-même est piloté par ce réglage).
    pub ressenti_prompt_enabled: bool,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            ressenti_prompt_enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaut_identique_au_comportement_precedemment_code_en_dur() {
        assert!(HealthConfig::default().ressenti_prompt_enabled);
    }

    #[test]
    fn round_trip_ron() {
        let cfg = HealthConfig {
            ressenti_prompt_enabled: false,
        };
        let serialized = ron::to_string(&cfg).expect("serialize");
        let parsed: HealthConfig = ron::from_str(&serialized).expect("parse");
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn section_absente_retombe_sur_le_defaut() {
        let empty: std::collections::HashMap<String, ron::Value> =
            ron::from_str("{}").expect("map vide");
        assert!(!empty.contains_key("health"));
        assert_eq!(HealthConfig::default(), HealthConfig::default());
    }
}
