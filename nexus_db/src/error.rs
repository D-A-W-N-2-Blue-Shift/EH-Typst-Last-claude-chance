// ============================================================================
// nexus_db/src/error.rs — Erreurs typées de la couche de données
//
// §7.5 : erreurs typées, pas de masquage silencieux, pas de String nue qui
// perd le type de la cause. La conversion depuis rusqlite / serde_json se fait
// via #[from] pour propager avec `?` sans jamais paniquer.
// ============================================================================

use thiserror::Error;

/// Toute erreur remontée par `nexus_db`. La couche UI la traduira en
/// `UserError { user_message, technical, provider }` (§8.3) au moment de
/// l'affichage — ici on conserve la cause technique typée.
#[derive(Debug, Error)]
pub enum NexusDbError {
    /// Erreur du moteur SQLite (contrainte violée, requête invalide, I/O…).
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Sérialisation/désérialisation d'un champ `tags` (JSON, doc §8).
    #[error("erreur JSON (champ tags) : {0}")]
    Json(#[from] serde_json::Error),

    /// Création du dossier `.engram/` avant ouverture de la base.
    #[error("erreur d'entrée/sortie : {0}")]
    Io(#[from] std::io::Error),
}

/// Alias local : tout le crate renvoie `Result<T, NexusDbError>`.
pub type Result<T> = std::result::Result<T, NexusDbError>;
