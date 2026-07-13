// ============================================================================
// nexus_db/src/lib.rs — Couche de données partagée de Nexus
//
// Ce que je fais : je définis et j'ouvre `nexus.db` (le SQLite du doc §8) et
// j'expose un accès typé (un struct par table, insertion/listing/lecture).
// Comment je marche : `open_db` applique le schéma (idempotent) ; les modules
// Nexus (health, todo, journal, dashboard) m'appellent pour lire/écrire, ils
// ne touchent jamais SQL directement.
// Comment me virer : supprimer `nexus_db/`, retirer le membre du Cargo.toml
// workspace, retirer la dépendance des modules qui m'utilisent.
// Mes dépendances : rusqlite (moteur), serde_json (tags JSON), thiserror
// (erreurs typées). Aucune vers un module ni vers le core.
//
// Doctrine : aucun `unwrap`/`expect` (§7.5), erreurs typées, en-tête français
// par fichier (anti-black-box).
// ============================================================================

pub mod error;
pub mod open;
pub mod schema;
pub mod store;

pub use error::{NexusDbError, Result};
pub use open::{open_db, open_in_memory, open_ro_db};
pub use rusqlite::Connection;
pub use schema::{present_tables, EXPECTED_TABLES, SCHEMA};
pub use store::{
    fts_search, fts_upsert, get_journal_entry_by_date, get_sleep_log_by_date, get_task,
    get_task_recurrence, insert_med_dose, insert_medication, insert_mood_log, insert_task,
    insert_task_history, insert_task_recurrence, list_articles, list_journal_entries,
    list_med_doses, list_medications, list_mood_logs, list_sleep_logs, list_task_history,
    list_tasks, transition_task_status, update_med_dose_ressenti, upsert_article,
    upsert_journal_entry, upsert_sleep_log, Article, FtsHit, JournalEntry, MedDose, Medication,
    MoodLog, SleepLog, Task, TaskHistory, TaskRecurrence,
};

use std::sync::atomic::{AtomicU64, Ordering};

/// Compteur de process pour départager deux identifiants générés dans la même
/// milliseconde. Repart de zéro à chaque lancement — sans effet sur l'unicité
/// (le préfixe milliseconde diffère alors).
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Génère un identifiant TEXT unique (doc §8 : `id TEXT PK`) sans dépendance
/// externe : millisecondes UNIX + compteur atomique de process. Pas de crate
/// `uuid` (§7.4 : pas de dépendance par confort). Deux appels dans la même
/// milliseconde sont départagés par le compteur.
pub fn new_id() -> String {
    let millis = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis(),
        Err(_) => 0,
    };
    let n = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{millis:013}-{n:08}")
}

#[cfg(test)]
mod tests {
    use super::new_id;

    #[test]
    fn new_id_est_unique_sur_une_rafale() {
        // Même milliseconde probable : le compteur doit garantir l'unicité.
        let ids: Vec<String> = (0..1000).map(|_| new_id()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "identifiants en double détectés");
    }
}
