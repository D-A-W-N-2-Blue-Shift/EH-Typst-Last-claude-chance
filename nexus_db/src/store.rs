// ============================================================================
// nexus_db/src/store.rs — Modèles typés + accès à `nexus.db`
//
// Un struct par table du doc §8, et pour chaque entité les opérations de base
// (insertion, listing, lecture par clé unique) qui PROUVENT la validité du
// schéma (round-trip) sans anticiper la logique métier des modules (§7.6
// YAGNI : moyennes de sommeil, transitions de statut, corrélations viendront
// avec les modules health/todo/dashboard).
//
// Colonnes NULLables du §8 → Option<T>. Champs `tags` → Vec<String> encodé en
// JSON TEXT. Aucun `unwrap`/`expect` (§7.5).
// ============================================================================

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;

// ---------------------------------------------------------------------------
// Modèles (doc §8).
// ---------------------------------------------------------------------------

/// Médicament de référence (table `medications`).
#[derive(Debug, Clone, PartialEq)]
pub struct Medication {
    pub id: String,
    pub nom: String,
    pub molecule: String,
    pub dose_default: f64,
    pub notes: Option<String>,
}

/// Prise effective (table `med_doses`). `ressenti` 1-5 optionnel.
#[derive(Debug, Clone, PartialEq)]
pub struct MedDose {
    pub id: String,
    pub med_id: String,
    pub taken_at: String,
    pub dose_mg: f64,
    pub ressenti: Option<i64>,
    pub notes: Option<String>,
}

/// Point d'état psy, 5 dimensions 1-5 (table `mood_log`).
#[derive(Debug, Clone, PartialEq)]
pub struct MoodLog {
    pub id: String,
    pub logged_at: String,
    pub epuisement: i64,
    pub cognitif: i64,
    pub sensoriel: i64,
    pub masquage: i64,
    pub fonctionnement: i64,
    pub notes: Option<String>,
}

/// Nuit de sommeil (table `sleep_log`, `date` UNIQUE).
#[derive(Debug, Clone, PartialEq)]
pub struct SleepLog {
    pub id: String,
    pub date: String,
    pub duration_h: f64,
    pub quality: i64,
    pub notes: Option<String>,
}

/// Tâche (table `tasks`). `parent_id` = sous-tâche (1 niveau).
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: String,
    pub parent_id: Option<String>,
    pub titre: String,
    pub statut: String,
    pub priorite: Option<i64>,
    pub energie: Option<String>,
    pub contexte: Option<String>,
    pub duree_min: Option<i64>,
    pub echeance: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub notes: Option<String>,
}

/// Règle de récurrence d'une tâche (table `task_recurrence`).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskRecurrence {
    pub id: String,
    pub task_id: String,
    pub regle: String,
    pub prochaine_date: String,
}

/// Transition de statut d'une tâche (table `task_history`).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskHistory {
    pub id: String,
    pub task_id: String,
    pub ancien_statut: String,
    pub nouveau_statut: String,
    pub changed_at: String,
}

/// Index d'une entrée de journal (table `journal_entries`, `date` UNIQUE).
#[derive(Debug, Clone, PartialEq)]
pub struct JournalEntry {
    pub id: String,
    pub date: String,
    pub file_path: String,
    pub word_count: i64,
    pub tags: Vec<String>,
}

/// Index d'un article (table `articles`, `file_path` UNIQUE).
#[derive(Debug, Clone, PartialEq)]
pub struct Article {
    pub id: String,
    pub file_path: String,
    pub titre: String,
    pub statut: String,
    pub tags: Vec<String>,
    pub date_cible: String,
    pub destination: String,
    pub word_count: i64,
    pub updated_at: String,
}

/// Résultat d'une recherche plein-texte (`fts_content`).
#[derive(Debug, Clone, PartialEq)]
pub struct FtsHit {
    pub file_path: String,
    pub snippet: String,
}

// ---------------------------------------------------------------------------
// medications.
// ---------------------------------------------------------------------------

/// Insère un médicament de référence.
pub fn insert_medication(conn: &Connection, m: &Medication) -> Result<()> {
    conn.execute(
        "INSERT INTO medications (id, nom, molecule, dose_default, notes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![m.id, m.nom, m.molecule, m.dose_default, m.notes],
    )?;
    Ok(())
}

/// Liste les médicaments, triés par nom.
pub fn list_medications(conn: &Connection) -> Result<Vec<Medication>> {
    let mut stmt = conn
        .prepare("SELECT id, nom, molecule, dose_default, notes FROM medications ORDER BY nom")?;
    let rows = stmt.query_map([], |row| {
        Ok(Medication {
            id: row.get(0)?,
            nom: row.get(1)?,
            molecule: row.get(2)?,
            dose_default: row.get(3)?,
            notes: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// med_doses.
// ---------------------------------------------------------------------------

/// Enregistre une prise. La clé étrangère `med_id` est vérifiée par SQLite.
pub fn insert_med_dose(conn: &Connection, d: &MedDose) -> Result<()> {
    conn.execute(
        "INSERT INTO med_doses (id, med_id, taken_at, dose_mg, ressenti, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![d.id, d.med_id, d.taken_at, d.dose_mg, d.ressenti, d.notes],
    )?;
    Ok(())
}

/// Liste les prises, les plus récentes d'abord (`taken_at` décroissant).
pub fn list_med_doses(conn: &Connection) -> Result<Vec<MedDose>> {
    let mut stmt = conn.prepare(
        "SELECT id, med_id, taken_at, dose_mg, ressenti, notes
         FROM med_doses ORDER BY taken_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MedDose {
            id: row.get(0)?,
            med_id: row.get(1)?,
            taken_at: row.get(2)?,
            dose_mg: row.get(3)?,
            ressenti: row.get(4)?,
            notes: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Enregistre le ressenti différé d'une prise déjà existante (doc §5.3 :
/// « champ optionnel disponible 3h après la dernière prise »). Ne touche
/// aucune autre colonne.
pub fn update_med_dose_ressenti(conn: &Connection, dose_id: &str, ressenti: i64) -> Result<()> {
    conn.execute(
        "UPDATE med_doses SET ressenti = ?1 WHERE id = ?2",
        params![ressenti, dose_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// mood_log.
// ---------------------------------------------------------------------------

/// Enregistre un point d'état psy. Les CHECK 1-5 sont vérifiés par SQLite.
pub fn insert_mood_log(conn: &Connection, m: &MoodLog) -> Result<()> {
    conn.execute(
        "INSERT INTO mood_log
         (id, logged_at, epuisement, cognitif, sensoriel, masquage, fonctionnement, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            m.id,
            m.logged_at,
            m.epuisement,
            m.cognitif,
            m.sensoriel,
            m.masquage,
            m.fonctionnement,
            m.notes
        ],
    )?;
    Ok(())
}

/// Liste les points d'état psy, les plus récents d'abord.
pub fn list_mood_logs(conn: &Connection) -> Result<Vec<MoodLog>> {
    let mut stmt = conn.prepare(
        "SELECT id, logged_at, epuisement, cognitif, sensoriel, masquage, fonctionnement, notes
         FROM mood_log ORDER BY logged_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MoodLog {
            id: row.get(0)?,
            logged_at: row.get(1)?,
            epuisement: row.get(2)?,
            cognitif: row.get(3)?,
            sensoriel: row.get(4)?,
            masquage: row.get(5)?,
            fonctionnement: row.get(6)?,
            notes: row.get(7)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// sleep_log (date UNIQUE : upsert).
// ---------------------------------------------------------------------------

/// Insère ou met à jour la nuit de `date` (une nuit = une ligne, doc §5.3).
pub fn upsert_sleep_log(conn: &Connection, s: &SleepLog) -> Result<()> {
    conn.execute(
        "INSERT INTO sleep_log (id, date, duration_h, quality, notes)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(date) DO UPDATE SET
             duration_h = excluded.duration_h,
             quality    = excluded.quality,
             notes      = excluded.notes",
        params![s.id, s.date, s.duration_h, s.quality, s.notes],
    )?;
    Ok(())
}

/// Liste les nuits, la plus récente d'abord.
pub fn list_sleep_logs(conn: &Connection) -> Result<Vec<SleepLog>> {
    let mut stmt = conn
        .prepare("SELECT id, date, duration_h, quality, notes FROM sleep_log ORDER BY date DESC")?;
    let rows = stmt.query_map([], |row| {
        Ok(SleepLog {
            id: row.get(0)?,
            date: row.get(1)?,
            duration_h: row.get(2)?,
            quality: row.get(3)?,
            notes: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Lit la nuit d'une date précise, si elle existe.
pub fn get_sleep_log_by_date(conn: &Connection, date: &str) -> Result<Option<SleepLog>> {
    let r = conn
        .query_row(
            "SELECT id, date, duration_h, quality, notes FROM sleep_log WHERE date = ?1",
            params![date],
            |row| {
                Ok(SleepLog {
                    id: row.get(0)?,
                    date: row.get(1)?,
                    duration_h: row.get(2)?,
                    quality: row.get(3)?,
                    notes: row.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(r)
}

// ---------------------------------------------------------------------------
// tasks.
// ---------------------------------------------------------------------------

/// Insère une tâche.
pub fn insert_task(conn: &Connection, t: &Task) -> Result<()> {
    conn.execute(
        "INSERT INTO tasks
         (id, parent_id, titre, statut, priorite, energie, contexte, duree_min,
          echeance, created_at, updated_at, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            t.id,
            t.parent_id,
            t.titre,
            t.statut,
            t.priorite,
            t.energie,
            t.contexte,
            t.duree_min,
            t.echeance,
            t.created_at,
            t.updated_at,
            t.notes
        ],
    )?;
    Ok(())
}

/// Liste les tâches, les plus récemment créées d'abord.
pub fn list_tasks(conn: &Connection) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(
        "SELECT id, parent_id, titre, statut, priorite, energie, contexte, duree_min,
                echeance, created_at, updated_at, notes
         FROM tasks ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Task {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            titre: row.get(2)?,
            statut: row.get(3)?,
            priorite: row.get(4)?,
            energie: row.get(5)?,
            contexte: row.get(6)?,
            duree_min: row.get(7)?,
            echeance: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            notes: row.get(11)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Lit une tâche par identifiant, si elle existe.
pub fn get_task(conn: &Connection, id: &str) -> Result<Option<Task>> {
    let r = conn
        .query_row(
            "SELECT id, parent_id, titre, statut, priorite, energie, contexte, duree_min,
                    echeance, created_at, updated_at, notes
             FROM tasks WHERE id = ?1",
            params![id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    titre: row.get(2)?,
                    statut: row.get(3)?,
                    priorite: row.get(4)?,
                    energie: row.get(5)?,
                    contexte: row.get(6)?,
                    duree_min: row.get(7)?,
                    echeance: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    notes: row.get(11)?,
                })
            },
        )
        .optional()?;
    Ok(r)
}

/// Fait transitionner une tâche vers `new_statut` et enregistre la
/// transition dans `task_history` — atomique (doc §5.4 : « pas d'oubli
/// silencieusement » ; un statut ne change jamais sans laisser de trace).
/// Retourne la tâche mise à jour. `Err` si la tâche n'existe pas.
pub fn transition_task_status(
    conn: &mut Connection,
    task_id: &str,
    new_statut: &str,
    changed_at: &str,
) -> Result<Task> {
    let tx = conn.transaction()?;
    let ancien_statut: String = tx.query_row(
        "SELECT statut FROM tasks WHERE id = ?1",
        params![task_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE tasks SET statut = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_statut, changed_at, task_id],
    )?;
    tx.execute(
        "INSERT INTO task_history (id, task_id, ancien_statut, nouveau_statut, changed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            crate::new_id(),
            task_id,
            ancien_statut,
            new_statut,
            changed_at
        ],
    )?;
    let updated = tx.query_row(
        "SELECT id, parent_id, titre, statut, priorite, energie, contexte, duree_min,
                echeance, created_at, updated_at, notes
         FROM tasks WHERE id = ?1",
        params![task_id],
        |row| {
            Ok(Task {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                titre: row.get(2)?,
                statut: row.get(3)?,
                priorite: row.get(4)?,
                energie: row.get(5)?,
                contexte: row.get(6)?,
                duree_min: row.get(7)?,
                echeance: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                notes: row.get(11)?,
            })
        },
    )?;
    tx.commit()?;
    Ok(updated)
}

// ---------------------------------------------------------------------------
// task_recurrence / task_history.
// ---------------------------------------------------------------------------

/// Insère une règle de récurrence pour une tâche.
pub fn insert_task_recurrence(conn: &Connection, r: &TaskRecurrence) -> Result<()> {
    conn.execute(
        "INSERT INTO task_recurrence (id, task_id, regle, prochaine_date)
         VALUES (?1, ?2, ?3, ?4)",
        params![r.id, r.task_id, r.regle, r.prochaine_date],
    )?;
    Ok(())
}

/// Lit la règle de récurrence d'une tâche, si elle existe.
pub fn get_task_recurrence(conn: &Connection, task_id: &str) -> Result<Option<TaskRecurrence>> {
    let r = conn
        .query_row(
            "SELECT id, task_id, regle, prochaine_date FROM task_recurrence WHERE task_id = ?1",
            params![task_id],
            |row| {
                Ok(TaskRecurrence {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    regle: row.get(2)?,
                    prochaine_date: row.get(3)?,
                })
            },
        )
        .optional()?;
    Ok(r)
}

/// Ajoute une ligne d'historique de transition de statut.
pub fn insert_task_history(conn: &Connection, h: &TaskHistory) -> Result<()> {
    conn.execute(
        "INSERT INTO task_history (id, task_id, ancien_statut, nouveau_statut, changed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            h.id,
            h.task_id,
            h.ancien_statut,
            h.nouveau_statut,
            h.changed_at
        ],
    )?;
    Ok(())
}

/// Liste l'historique d'une tâche, du plus ancien au plus récent.
pub fn list_task_history(conn: &Connection, task_id: &str) -> Result<Vec<TaskHistory>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, ancien_statut, nouveau_statut, changed_at
         FROM task_history WHERE task_id = ?1 ORDER BY changed_at ASC",
    )?;
    let rows = stmt.query_map(params![task_id], |row| {
        Ok(TaskHistory {
            id: row.get(0)?,
            task_id: row.get(1)?,
            ancien_statut: row.get(2)?,
            nouveau_statut: row.get(3)?,
            changed_at: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// journal_entries (date UNIQUE : upsert) — tags JSON.
// ---------------------------------------------------------------------------

/// Insère ou met à jour l'index de l'entrée de `date`.
pub fn upsert_journal_entry(conn: &Connection, e: &JournalEntry) -> Result<()> {
    let tags = serde_json::to_string(&e.tags)?;
    conn.execute(
        "INSERT INTO journal_entries (id, date, file_path, word_count, tags)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(date) DO UPDATE SET
             file_path  = excluded.file_path,
             word_count = excluded.word_count,
             tags       = excluded.tags",
        params![e.id, e.date, e.file_path, e.word_count, tags],
    )?;
    Ok(())
}

/// Lit l'entrée de journal d'une date, si elle existe.
pub fn get_journal_entry_by_date(conn: &Connection, date: &str) -> Result<Option<JournalEntry>> {
    let raw = conn
        .query_row(
            "SELECT id, date, file_path, word_count, tags FROM journal_entries WHERE date = ?1",
            params![date],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    match raw {
        Some((id, date, file_path, word_count, tags_json)) => {
            let tags: Vec<String> = serde_json::from_str(&tags_json)?;
            Ok(Some(JournalEntry {
                id,
                date,
                file_path,
                word_count,
                tags,
            }))
        }
        None => Ok(None),
    }
}

/// Liste les entrées de journal, la plus récente d'abord.
pub fn list_journal_entries(conn: &Connection) -> Result<Vec<JournalEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, file_path, word_count, tags FROM journal_entries ORDER BY date DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (id, date, file_path, word_count, tags_json) = r?;
        let tags: Vec<String> = serde_json::from_str(&tags_json)?;
        out.push(JournalEntry {
            id,
            date,
            file_path,
            word_count,
            tags,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// articles (file_path UNIQUE : upsert) — tags JSON.
// ---------------------------------------------------------------------------

/// Insère ou met à jour l'index d'un article (clé : `file_path`).
pub fn upsert_article(conn: &Connection, a: &Article) -> Result<()> {
    let tags = serde_json::to_string(&a.tags)?;
    conn.execute(
        "INSERT INTO articles
         (id, file_path, titre, statut, tags, date_cible, destination, word_count, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(file_path) DO UPDATE SET
             titre       = excluded.titre,
             statut      = excluded.statut,
             tags        = excluded.tags,
             date_cible  = excluded.date_cible,
             destination = excluded.destination,
             word_count  = excluded.word_count,
             updated_at  = excluded.updated_at",
        params![
            a.id,
            a.file_path,
            a.titre,
            a.statut,
            tags,
            a.date_cible,
            a.destination,
            a.word_count,
            a.updated_at
        ],
    )?;
    Ok(())
}

/// Liste les articles, triés par titre.
pub fn list_articles(conn: &Connection) -> Result<Vec<Article>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, titre, statut, tags, date_cible, destination, word_count, updated_at
         FROM articles ORDER BY titre",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (
            id,
            file_path,
            titre,
            statut,
            tags_json,
            date_cible,
            destination,
            word_count,
            updated_at,
        ) = r?;
        let tags: Vec<String> = serde_json::from_str(&tags_json)?;
        out.push(Article {
            id,
            file_path,
            titre,
            statut,
            tags,
            date_cible,
            destination,
            word_count,
            updated_at,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// fts_content : recherche plein-texte.
// ---------------------------------------------------------------------------

/// Indexe (ou ré-indexe) le corps d'un fichier pour la recherche plein-texte.
/// Motif DELETE-puis-INSERT (FTS5 n'a pas d'UPSERT), calqué sur indexer.rs.
pub fn fts_upsert(conn: &Connection, file_path: &str, body_text: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM fts_content WHERE file_path = ?1",
        params![file_path],
    )?;
    conn.execute(
        "INSERT INTO fts_content (file_path, body_text) VALUES (?1, ?2)",
        params![file_path, body_text],
    )?;
    Ok(())
}

/// Recherche plein-texte, triée par pertinence (bm25). La requête est
/// normalisée (termes entre guillemets, jointure AND) pour ne jamais produire
/// une erreur de syntaxe FTS sur une saisie libre.
pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<FtsHit>> {
    let normalized = normalize_fts_query(query);
    if normalized.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT file_path, snippet(fts_content, 1, '[', ']', ' … ', 12)
         FROM fts_content WHERE fts_content MATCH ?1 ORDER BY bm25(fts_content) LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![normalized, limit as i64], |row| {
        Ok(FtsHit {
            file_path: row.get(0)?,
            snippet: row.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Réduit une saisie libre à une requête FTS sûre : chaque terme est nettoyé
/// (alphanumérique + underscore), mis entre guillemets, et les termes sont
/// joints par AND. Vide si rien d'exploitable.
fn normalize_fts_query(input: &str) -> String {
    let mut terms = Vec::new();
    for raw in input.split_whitespace() {
        let term: String = raw
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !term.is_empty() {
            terms.push(format!("\"{term}\""));
        }
    }
    terms.join(" AND ")
}

// ===========================================================================
// Tests : round-trip par table, clés étrangères, CHECK 1-5, contraintes
// UNIQUE, FTS. C'est la preuve que le schéma du doc §8 tient (§16 : la preuve,
// pas l'affirmation).
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::open_in_memory;
    use crate::{new_id, schema::EXPECTED_TABLES};

    fn a_medication() -> Medication {
        Medication {
            id: new_id(),
            nom: "Méthylphénidate LP".into(),
            molecule: "methylphenidate".into(),
            dose_default: 20.0,
            notes: Some("registre de référence".into()),
        }
    }

    #[test]
    fn schema_cree_toutes_les_tables_attendues() -> Result<()> {
        let conn = open_in_memory()?;
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        for expected in EXPECTED_TABLES {
            assert!(
                names.iter().any(|n| n == expected),
                "table attendue absente : {expected}"
            );
        }
        Ok(())
    }

    #[test]
    fn medication_round_trip() -> Result<()> {
        let conn = open_in_memory()?;
        let m = a_medication();
        insert_medication(&conn, &m)?;
        let all = list_medications(&conn)?;
        assert_eq!(all, vec![m]);
        Ok(())
    }

    #[test]
    fn med_dose_respecte_la_cle_etrangere() -> Result<()> {
        let conn = open_in_memory()?;
        // med_id inexistant : la clé étrangère doit rejeter l'insertion.
        let orphan = MedDose {
            id: new_id(),
            med_id: "inexistant".into(),
            taken_at: "2026-07-12T09:00:00".into(),
            dose_mg: 20.0,
            ressenti: None,
            notes: None,
        };
        assert!(
            insert_med_dose(&conn, &orphan).is_err(),
            "une prise orpheline (med_id inconnu) doit être rejetée"
        );
        Ok(())
    }

    #[test]
    fn med_dose_round_trip_avec_medicament() -> Result<()> {
        let conn = open_in_memory()?;
        let m = a_medication();
        insert_medication(&conn, &m)?;
        let d = MedDose {
            id: new_id(),
            med_id: m.id.clone(),
            taken_at: "2026-07-12T09:00:00".into(),
            dose_mg: 20.0,
            ressenti: Some(4),
            notes: None,
        };
        insert_med_dose(&conn, &d)?;
        assert_eq!(list_med_doses(&conn)?, vec![d]);
        Ok(())
    }

    #[test]
    fn update_med_dose_ressenti_ne_touche_que_ce_champ() -> Result<()> {
        let conn = open_in_memory()?;
        let m = a_medication();
        insert_medication(&conn, &m)?;
        let d = MedDose {
            id: new_id(),
            med_id: m.id.clone(),
            taken_at: "2026-07-12T09:00:00".into(),
            dose_mg: 20.0,
            ressenti: None,
            notes: None,
        };
        insert_med_dose(&conn, &d)?;
        update_med_dose_ressenti(&conn, &d.id, 3)?;
        let all = list_med_doses(&conn)?;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].ressenti, Some(3));
        assert_eq!(all[0].dose_mg, 20.0);
        Ok(())
    }

    #[test]
    fn mood_log_round_trip() -> Result<()> {
        let conn = open_in_memory()?;
        let m = MoodLog {
            id: new_id(),
            logged_at: "2026-07-12T20:00:00".into(),
            epuisement: 4,
            cognitif: 2,
            sensoriel: 3,
            masquage: 5,
            fonctionnement: 2,
            notes: Some("soirée difficile".into()),
        };
        insert_mood_log(&conn, &m)?;
        assert_eq!(list_mood_logs(&conn)?, vec![m]);
        Ok(())
    }

    #[test]
    fn mood_log_rejette_score_hors_domaine() -> Result<()> {
        let conn = open_in_memory()?;
        let bad = MoodLog {
            id: new_id(),
            logged_at: "2026-07-12T20:00:00".into(),
            epuisement: 6, // hors 1-5 : le CHECK doit rejeter.
            cognitif: 2,
            sensoriel: 3,
            masquage: 5,
            fonctionnement: 2,
            notes: None,
        };
        assert!(
            insert_mood_log(&conn, &bad).is_err(),
            "un score hors 1-5 doit être rejeté par le CHECK"
        );
        Ok(())
    }

    #[test]
    fn sleep_log_upsert_par_date() -> Result<()> {
        let conn = open_in_memory()?;
        let mut s = SleepLog {
            id: new_id(),
            date: "2026-07-12".into(),
            duration_h: 7.5,
            quality: 3,
            notes: None,
        };
        upsert_sleep_log(&conn, &s)?;
        // Même date, nouvelles valeurs : mise à jour, pas de doublon.
        s.duration_h = 8.0;
        s.quality = 4;
        upsert_sleep_log(&conn, &s)?;
        let all = list_sleep_logs(&conn)?;
        assert_eq!(all.len(), 1, "une nuit = une ligne (date UNIQUE)");
        let got = get_sleep_log_by_date(&conn, "2026-07-12")?;
        assert_eq!(got.map(|x| (x.duration_h, x.quality)), Some((8.0, 4)));
        Ok(())
    }

    #[test]
    fn task_round_trip_avec_sous_tache_historique_recurrence() -> Result<()> {
        let conn = open_in_memory()?;
        let parent = Task {
            id: new_id(),
            parent_id: None,
            titre: "Dossier MDPH".into(),
            statut: "today".into(),
            priorite: Some(1),
            energie: Some("high".into()),
            contexte: Some("ordi".into()),
            duree_min: Some(90),
            echeance: Some("2026-07-20".into()),
            created_at: "2026-07-12T10:00:00".into(),
            updated_at: "2026-07-12T10:00:00".into(),
            notes: None,
        };
        insert_task(&conn, &parent)?;
        let child = Task {
            id: new_id(),
            parent_id: Some(parent.id.clone()),
            titre: "Rassembler les justificatifs".into(),
            statut: "backlog".into(),
            priorite: None,
            energie: Some("low".into()),
            contexte: Some("maison".into()),
            duree_min: Some(20),
            echeance: None,
            created_at: "2026-07-12T10:05:00".into(),
            updated_at: "2026-07-12T10:05:00".into(),
            notes: None,
        };
        insert_task(&conn, &child)?;
        assert_eq!(list_tasks(&conn)?.len(), 2);
        assert_eq!(get_task(&conn, &parent.id)?.as_ref(), Some(&parent));

        // Historique de transition.
        let h = TaskHistory {
            id: new_id(),
            task_id: parent.id.clone(),
            ancien_statut: "today".into(),
            nouveau_statut: "doing".into(),
            changed_at: "2026-07-12T11:00:00".into(),
        };
        insert_task_history(&conn, &h)?;
        assert_eq!(list_task_history(&conn, &parent.id)?, vec![h]);

        // Récurrence.
        let rec = TaskRecurrence {
            id: new_id(),
            task_id: parent.id.clone(),
            regle: "hebdo".into(),
            prochaine_date: "2026-07-19".into(),
        };
        insert_task_recurrence(&conn, &rec)?;
        assert_eq!(get_task_recurrence(&conn, &parent.id)?, Some(rec));
        Ok(())
    }

    #[test]
    fn transition_task_status_met_a_jour_et_historise() -> Result<()> {
        let mut conn = open_in_memory()?;
        let t = Task {
            id: new_id(),
            parent_id: None,
            titre: "Appeler la CPAM".into(),
            statut: "today".into(),
            priorite: None,
            energie: Some("medium".into()),
            contexte: Some("téléphone".into()),
            duree_min: Some(15),
            echeance: None,
            created_at: "2026-07-12T09:00:00".into(),
            updated_at: "2026-07-12T09:00:00".into(),
            notes: None,
        };
        insert_task(&conn, &t)?;

        let updated = transition_task_status(&mut conn, &t.id, "doing", "2026-07-12T10:00:00")?;
        assert_eq!(updated.statut, "doing");
        assert_eq!(updated.updated_at, "2026-07-12T10:00:00");

        let history = list_task_history(&conn, &t.id)?;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].ancien_statut, "today");
        assert_eq!(history[0].nouveau_statut, "doing");
        Ok(())
    }

    #[test]
    fn transition_task_status_inconnue_echoue() -> Result<()> {
        let mut conn = open_in_memory()?;
        assert!(
            transition_task_status(&mut conn, "inexistant", "done", "2026-07-12T10:00:00").is_err()
        );
        Ok(())
    }

    #[test]
    fn journal_entry_upsert_et_tags_json() -> Result<()> {
        let conn = open_in_memory()?;
        let e = JournalEntry {
            id: new_id(),
            date: "2026-07-12".into(),
            file_path: "01_journal/2026/2026-07-12.typst".into(),
            word_count: 412,
            tags: vec!["burnout".into(), "sommeil".into()],
        };
        upsert_journal_entry(&conn, &e)?;
        assert_eq!(
            get_journal_entry_by_date(&conn, "2026-07-12")?,
            Some(e.clone())
        );
        // Upsert même date : pas de doublon, tags mis à jour.
        let e2 = JournalEntry {
            id: new_id(),
            tags: vec!["repos".into()],
            word_count: 500,
            ..e.clone()
        };
        upsert_journal_entry(&conn, &e2)?;
        let all = list_journal_entries(&conn)?;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].tags, vec!["repos".to_string()]);
        assert_eq!(all[0].word_count, 500);
        Ok(())
    }

    #[test]
    fn article_upsert_et_tags_json() -> Result<()> {
        let conn = open_in_memory()?;
        let a = Article {
            id: new_id(),
            file_path: "04_articles/burnout_autistique.typst".into(),
            titre: "Le burnout autistique".into(),
            statut: "brouillon".into(),
            tags: vec!["neurodivergence".into(), "santé".into()],
            date_cible: "2026-08-01".into(),
            destination: "blog".into(),
            word_count: 1200,
            updated_at: "2026-07-12T12:00:00".into(),
        };
        upsert_article(&conn, &a)?;
        assert_eq!(list_articles(&conn)?, vec![a.clone()]);
        // Upsert même file_path : mise à jour du statut.
        let a2 = Article {
            id: new_id(),
            statut: "revue".into(),
            ..a.clone()
        };
        upsert_article(&conn, &a2)?;
        let all = list_articles(&conn)?;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].statut, "revue");
        Ok(())
    }

    #[test]
    fn fts_indexe_et_retrouve() -> Result<()> {
        let conn = open_in_memory()?;
        fts_upsert(
            &conn,
            "01_journal/2026/2026-07-12.typst",
            "Journée de brouillard cognitif intense, sommeil haché.",
        )?;
        fts_upsert(
            &conn,
            "04_articles/repos.typst",
            "Le repos réparateur est un besoin, pas un luxe.",
        )?;
        let hits = fts_search(&conn, "cognitif", 10)?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_path, "01_journal/2026/2026-07-12.typst");
        // Ré-indexation du même fichier : pas de doublon.
        fts_upsert(
            &conn,
            "01_journal/2026/2026-07-12.typst",
            "Contenu remplacé, plus aucun brouillard.",
        )?;
        assert_eq!(fts_search(&conn, "cognitif", 10)?.len(), 0);
        assert_eq!(fts_search(&conn, "brouillard", 10)?.len(), 1);
        // Saisie vide ou non exploitable : aucun résultat, aucune erreur.
        assert!(fts_search(&conn, "   ", 10)?.is_empty());
        Ok(())
    }
}
