// ============================================================================
// nexus_db/src/open.rs — Ouverture de `nexus.db`
//
// Trois portes d'entrée, calquées sur file_tree/src/indexer.rs :
//   - open_db      : lecture/écriture, WAL + foreign_keys ON, schéma appliqué.
//   - open_ro_db   : lecture seule (pour `nexus_inspect`, doc §5.8).
//   - open_in_memory : base éphémère pour les tests (foreign_keys ON, schéma).
//
// Aucun `unwrap`/`expect` (§7.5) : tout se propage en NexusDbError via `?`.
// ============================================================================

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::error::Result;
use crate::schema::SCHEMA;

/// Ouvre (ou crée) `nexus.db` en lecture/écriture. Crée le dossier parent
/// (`.engram/`) au besoin, active WAL et les clés étrangères, applique le
/// schéma de façon idempotente.
pub fn open_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(2000))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Ouvre `nexus.db` en lecture seule (audit sans risque de mutation, doc §5.8).
/// La base doit déjà exister — aucune création, aucune application de schéma.
pub fn open_ro_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_millis(2000))?;
    conn.pragma_update(None, "query_only", true)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// Base en mémoire, schéma appliqué, clés étrangères actives. Réservée aux
/// tests : pas de WAL (inutile hors fichier).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}
