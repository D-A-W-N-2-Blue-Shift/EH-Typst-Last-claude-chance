// ============================================================================
// modules/nexus_hub/src/summary.rs — Résumé du jour (doc §5.1)
//
// Remplace les stats narratives de Hive : le hub Nexus n'affiche pas de
// compteur de mots, mais un état du jour au coup d'œil — entrée journal
// écrite aujourd'hui ? dernier score mood du jour, s'il existe ? quelles
// tâches sont dans la colonne "today" du kanban ?
//
// Logique pure, testée sans DB ni egui : la construction du résumé prend en
// entrée ce que le hub a déjà lu (une requête par table), pas de nouvelle
// requête SQL ad hoc ici.
// ============================================================================

use nexus_db::{JournalEntry, MoodLog, Task};

pub struct DaySummary {
    pub journal_today: Option<JournalEntry>,
    pub mood_today: Option<MoodLog>,
    pub today_tasks: Vec<Task>,
}

/// Construit le résumé du jour à partir des données déjà chargées.
/// `all_mood_logs` doit être triée par `logged_at` décroissant (contrat de
/// `nexus_db::list_mood_logs`) : le premier match du jour est donc le plus
/// récent, qui est ce qu'on veut afficher pour "le score mood du jour".
pub fn build(
    journal_today: Option<JournalEntry>,
    all_mood_logs: &[MoodLog],
    all_tasks: &[Task],
    today: chrono::NaiveDate,
) -> DaySummary {
    let mood_today = all_mood_logs
        .iter()
        .find(|m| logged_at_is_today(&m.logged_at, today))
        .cloned();
    let today_tasks = all_tasks
        .iter()
        .filter(|t| t.statut == "today")
        .cloned()
        .collect();
    DaySummary {
        journal_today,
        mood_today,
        today_tasks,
    }
}

fn logged_at_is_today(logged_at: &str, today: chrono::NaiveDate) -> bool {
    logged_at
        .get(0..10)
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        == Some(today)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mood(logged_at: &str) -> MoodLog {
        MoodLog {
            id: "m1".into(),
            logged_at: logged_at.into(),
            epuisement: 3,
            cognitif: 3,
            sensoriel: 3,
            masquage: 3,
            fonctionnement: 3,
            notes: None,
        }
    }

    fn task(statut: &str) -> Task {
        Task {
            id: "t1".into(),
            parent_id: None,
            titre: "titre".into(),
            statut: statut.into(),
            priorite: None,
            energie: None,
            contexte: None,
            duree_min: None,
            echeance: None,
            created_at: "2026-07-14T00:00:00Z".into(),
            updated_at: "2026-07-14T00:00:00Z".into(),
            notes: None,
        }
    }

    #[test]
    fn journal_absent_si_aucune_entree_ce_jour() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let s = build(None, &[], &[], today);
        assert!(s.journal_today.is_none());
    }

    #[test]
    fn mood_du_jour_retrouve_le_plus_recent_du_jour() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let logs = vec![
            mood("2026-07-14T20:00:00"), // le plus récent du jour (liste déjà triée DESC)
            mood("2026-07-14T08:00:00"),
            mood("2026-07-13T20:00:00"), // hier, ne doit pas matcher
        ];
        let s = build(None, &logs, &[], today);
        assert_eq!(s.mood_today.unwrap().logged_at, "2026-07-14T20:00:00");
    }

    #[test]
    fn mood_absent_si_rien_loggue_aujourdhui() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let logs = vec![mood("2026-07-10T08:00:00")];
        let s = build(None, &logs, &[], today);
        assert!(s.mood_today.is_none());
    }

    #[test]
    fn taches_today_filtre_uniquement_la_colonne_today() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let tasks = vec![task("today"), task("backlog"), task("today"), task("done")];
        let s = build(None, &[], &tasks, today);
        assert_eq!(s.today_tasks.len(), 2);
    }

    #[test]
    fn logged_at_is_today_ignore_lheure() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        assert!(logged_at_is_today("2026-07-14T23:59:59", today));
        assert!(!logged_at_is_today("2026-07-15T00:00:00", today));
        assert!(!logged_at_is_today("bruit", today));
    }
}
