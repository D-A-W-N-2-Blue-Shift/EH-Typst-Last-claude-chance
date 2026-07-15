// ============================================================================
// nexus_db/src/schema.rs — Schéma SQLite de `nexus.db`
//
// Reprend le MCD complet du doc de conception §8, verbatim sur les noms de
// tables et de colonnes. Décisions documentées (§A10) :
//   - `notes`, `ressenti`, `priorite`, `energie`, `contexte`, `duree_min`,
//     `echeance`, `parent_id` sont NULLables comme au §8 (représentés par des
//     Option<T> côté Rust).
//   - `task_recurrence.id` : le §8 écrit « id PK » sans type ; on aligne sur
//     les autres tables (TEXT PK, cf. §8 medications/med_doses/…).
//   - Contraintes CHECK 1..5 ajoutées sur les scores (qualité, ressenti, 5
//     dimensions d'état psy) : le doc borne ces échelles à 1-5 (§5.1/§5.3) ;
//     la base rejette une valeur hors domaine (§A3 robustesse > simplicité).
//   - Les « vues » de corrélation (doc §4.2) NE sont PAS créées ici : le doc
//     précise qu'elles ne sont pas stockées mais exécutées à la demande. Elles
//     vivront dans le module dashboard, testées sur données réelles.
//
// `IF NOT EXISTS` partout : appliquer le schéma sur une base déjà initialisée
// est idempotent (même logique que file_tree/src/indexer.rs).
// ============================================================================

/// DDL complète de `nexus.db`. Appliquée par `open::open_db` et
/// `open::open_in_memory` via `execute_batch`.
pub const SCHEMA: &str = "
-- medications : registre de référence des molécules (doc §8).
CREATE TABLE IF NOT EXISTS medications (
    id            TEXT PRIMARY KEY,
    nom           TEXT NOT NULL,
    molecule      TEXT NOT NULL,
    dose_default  REAL NOT NULL,
    notes         TEXT
);

-- med_doses : une prise effective (doc §8). ressenti 1-5 optionnel (différé).
CREATE TABLE IF NOT EXISTS med_doses (
    id        TEXT PRIMARY KEY,
    med_id    TEXT NOT NULL REFERENCES medications(id),
    taken_at  TEXT NOT NULL,
    dose_mg   REAL NOT NULL,
    ressenti  INTEGER CHECK (ressenti IS NULL OR ressenti BETWEEN 1 AND 5),
    notes     TEXT
);

-- mood_log : état psy, 5 dimensions 1-5 (doc §5.3, base ABM/Raymaker 2020).
CREATE TABLE IF NOT EXISTS mood_log (
    id             TEXT PRIMARY KEY,
    logged_at      TEXT NOT NULL,
    epuisement     INTEGER NOT NULL CHECK (epuisement BETWEEN 1 AND 5),
    cognitif       INTEGER NOT NULL CHECK (cognitif BETWEEN 1 AND 5),
    sensoriel      INTEGER NOT NULL CHECK (sensoriel BETWEEN 1 AND 5),
    masquage       INTEGER NOT NULL CHECK (masquage BETWEEN 1 AND 5),
    fonctionnement INTEGER NOT NULL CHECK (fonctionnement BETWEEN 1 AND 5),
    notes          TEXT
);

-- sleep_log : une nuit = une ligne (date UNIQUE), qualité 1-5 (doc §5.3).
CREATE TABLE IF NOT EXISTS sleep_log (
    id          TEXT PRIMARY KEY,
    date        TEXT NOT NULL UNIQUE,
    duration_h  REAL NOT NULL,
    quality     INTEGER NOT NULL CHECK (quality BETWEEN 1 AND 5),
    notes       TEXT
);

-- tasks : kanban orienté énergie (doc §5.4). parent_id = sous-tâche, 1 niveau.
CREATE TABLE IF NOT EXISTS tasks (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT REFERENCES tasks(id),
    titre       TEXT NOT NULL,
    statut      TEXT NOT NULL,
    priorite    INTEGER,
    energie     TEXT,
    contexte    TEXT,
    duree_min   INTEGER,
    echeance    TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    notes       TEXT
);

-- task_recurrence : règle de régénération d'une tâche récurrente (doc §5.4).
CREATE TABLE IF NOT EXISTS task_recurrence (
    id             TEXT PRIMARY KEY,
    task_id        TEXT NOT NULL REFERENCES tasks(id),
    regle          TEXT NOT NULL,
    prochaine_date TEXT NOT NULL
);

-- task_history : trace des transitions de statut (doc §5.4, pas d'oubli muet).
CREATE TABLE IF NOT EXISTS task_history (
    id             TEXT PRIMARY KEY,
    task_id        TEXT NOT NULL REFERENCES tasks(id),
    ancien_statut  TEXT NOT NULL,
    nouveau_statut TEXT NOT NULL,
    changed_at     TEXT NOT NULL
);

-- journal_entries : index d'une entrée quotidienne (le fichier .md — ou .typst hérité — est la source).
CREATE TABLE IF NOT EXISTS journal_entries (
    id          TEXT PRIMARY KEY,
    date        TEXT NOT NULL UNIQUE,
    file_path   TEXT NOT NULL,
    word_count  INTEGER NOT NULL DEFAULT 0,
    tags        TEXT NOT NULL DEFAULT '[]'
);

-- articles : index d'un article (le fichier .md — ou .typst hérité — est la source).
CREATE TABLE IF NOT EXISTS articles (
    id           TEXT PRIMARY KEY,
    file_path    TEXT NOT NULL UNIQUE,
    titre        TEXT NOT NULL DEFAULT '',
    statut       TEXT NOT NULL DEFAULT 'brouillon',
    tags         TEXT NOT NULL DEFAULT '[]',
    date_cible   TEXT NOT NULL DEFAULT '',
    destination  TEXT NOT NULL DEFAULT '',
    word_count   INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL DEFAULT ''
);

-- fts_content : recherche plein-texte (doc §8) sur journal + articles + notes.
-- FTS5 autonome (pas de content=…) : la table ne référence aucune autre table,
-- elle stocke le corps indexé, comme dans la version écrivain (indexer.rs).
CREATE VIRTUAL TABLE IF NOT EXISTS fts_content USING fts5(
    file_path UNINDEXED,
    body_text
);
";

/// Les tables attendues après application de `SCHEMA` (hors tables internes
/// créées par FTS5). Sert de référence au test d'intégrité du schéma et au
/// futur onglet « Intégrité » de `nexus_inspect` (doc §5.8).
pub const EXPECTED_TABLES: &[&str] = &[
    "medications",
    "med_doses",
    "mood_log",
    "sleep_log",
    "tasks",
    "task_recurrence",
    "task_history",
    "journal_entries",
    "articles",
    "fts_content",
];

/// Sous-ensemble d'`EXPECTED_TABLES` réellement présent dans la base ouverte
/// (ordre d'`EXPECTED_TABLES`). Sert au statut affiché par le hub (doc §5.1 :
/// résumé de l'état) sans que les modules appelants n'écrivent de SQL.
pub fn present_tables(conn: &rusqlite::Connection) -> crate::error::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let names: std::collections::HashSet<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<std::collections::HashSet<String>>>()?;
    Ok(EXPECTED_TABLES
        .iter()
        .filter(|t| names.contains(**t))
        .map(|t| t.to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::open_in_memory;

    #[test]
    fn present_tables_matches_expected_on_fresh_db() -> crate::error::Result<()> {
        let conn = open_in_memory()?;
        let present = present_tables(&conn)?;
        assert_eq!(present.len(), EXPECTED_TABLES.len());
        Ok(())
    }
}
