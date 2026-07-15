// ============================================================================
// modules/dashboard/src/config.rs — Section "dashboard" de Hive_RBMK.ron
//
// Sous-ensemble volontairement restreint des « fenêtres temporelles » et
// « seuils d'alerte » du doc §5.7 : seuls les seuils RÉELLEMENT branchés sur
// un comportement runtime sont exposés ici — modules/cockpit_nexus n'affiche
// jamais une config qui ne fait rien (voir son README_MODULE.md).
//
// EXCLU délibérément : le seuil d'échantillon minimal (MIN_SAMPLE_SLEEP,
// logic.rs) reste FIXE, non configurable. C'est le garde-fou anti-invention
// du doc §5.6 (« aucune donnée inventée » — sous le seuil, le dashboard
// affiche « référentiel insuffisant » plutôt qu'une courbe). Le rendre
// configurable permettrait de l'affaiblir (minimum=1) et de fabriquer un
// référentiel qui a l'air valide sans l'être — décision consciente, pas un
// oubli.
// ============================================================================

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DashboardConfig {
    /// Fenêtre de la vue Sommeil au premier affichage (doc §5.7 : « fenêtres
    /// temporelles »). L'utilisateur peut toujours basculer 30/60/90j ensuite.
    pub default_sleep_window_days: i64,
    /// Score en dessous duquel une dimension d'état psy déclenche l'alerte
    /// visuelle (doc §5.6/§5.7 : « seuils d'alerte »).
    pub mood_alert_threshold: f64,
    /// Nombre de jours consécutifs sous le seuil requis pour déclencher
    /// l'alerte.
    pub mood_alert_consecutive_days: i64,
    /// Tâches « bloquées depuis plus de N jours » (doc §5.6, corrélations).
    pub blocked_days_threshold: i64,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            default_sleep_window_days: 30,
            mood_alert_threshold: 2.0,
            mood_alert_consecutive_days: 3,
            blocked_days_threshold: 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaut_identique_au_comportement_precedemment_code_en_dur() {
        // Ces valeurs étaient des littéraux avant cet incrément (sessions
        // 6/7, doc §10) — zéro régression pour un projet sans section
        // "dashboard" dans Hive_RBMK.ron.
        let cfg = DashboardConfig::default();
        assert_eq!(cfg.default_sleep_window_days, 30);
        assert_eq!(cfg.mood_alert_threshold, 2.0);
        assert_eq!(cfg.mood_alert_consecutive_days, 3);
        assert_eq!(cfg.blocked_days_threshold, 7);
    }

    #[test]
    fn round_trip_ron() {
        let cfg = DashboardConfig {
            default_sleep_window_days: 60,
            mood_alert_threshold: 2.5,
            mood_alert_consecutive_days: 4,
            blocked_days_threshold: 10,
        };
        let serialized = ron::to_string(&cfg).expect("serialize");
        let parsed: DashboardConfig = ron::from_str(&serialized).expect("parse");
        assert_eq!(parsed, cfg);
    }
}
