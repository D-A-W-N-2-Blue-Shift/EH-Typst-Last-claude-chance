// ============================================================================
// modules/todo/src/config.rs — Section "todo" de Hive_RBMK.ron
//
// Doc §5.7 : « config todo (colonnes, filtres par défaut) ». Seuls les
// FILTRES sont exposés ici. Les colonnes restent fixes, volontairement non
// configurables : logic::COLUMNS et le statut "done" sont des chaînes
// canoniques dont dépendent transition_task_status (nexus_db) et la
// régénération de récurrence (lib.rs::change_status, comparaison littérale
// `new_statut != "done"`) — les rendre configurables romprait cette logique
// déjà testée, sans bénéfice demandé explicitement par le doc §5.4 (qui
// décrit le kanban à 6 colonnes comme un modèle fixe, pas un réglage).
// Détail dans README_MODULE.md.
// ============================================================================

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TodoConfig {
    /// État initial du filtre « maintenant » à l'ouverture (doc §5.4).
    pub default_filter_maintenant: bool,
    /// Contexte présélectionné à l'ouverture ("" = aucun filtre). Valeurs
    /// reconnues : celles de `CONTEXTES` (lib.rs) — une valeur inconnue est
    /// silencieusement sans effet (aucune tâche ne matchera un contexte qui
    /// n'existe pas), jamais un crash (§7.5).
    pub default_filter_contexte: String,
}

impl Default for TodoConfig {
    fn default() -> Self {
        Self {
            default_filter_maintenant: false,
            default_filter_contexte: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaut_identique_au_comportement_precedemment_code_en_dur() {
        let cfg = TodoConfig::default();
        assert!(!cfg.default_filter_maintenant);
        assert_eq!(cfg.default_filter_contexte, "");
    }

    #[test]
    fn round_trip_ron() {
        let cfg = TodoConfig {
            default_filter_maintenant: true,
            default_filter_contexte: "dehors".to_string(),
        };
        let serialized = ron::to_string(&cfg).expect("serialize");
        let parsed: TodoConfig = ron::from_str(&serialized).expect("parse");
        assert_eq!(parsed, cfg);
    }
}
