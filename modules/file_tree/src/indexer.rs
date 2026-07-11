// ============================================================================
// modules/file_tree/src/indexer.rs — Index SQLite + watcher notify
//
// Tourne dans son propre thread (rusqlite::Connection n'est pas Sync).
// Communication :
//   - UI → indexeur : channel de commandes (Reindex, FullRescan, Shutdown…)
//   - indexeur → UI : snapshot Arc<RwLock<HashMap<chemin, FileStats>>> +
//     compteur de génération atomique (l'UI reconstruit l'arbre quand il bouge)
//
// Mécanique :
//   - scan initial complet au démarrage (cible < 200ms pour 500 fichiers)
//   - notify surveille le projet (hors .engram/ et 06_en_cours/)
//   - debounce 500ms (configurable) avant re-indexation incrémentale
//   - flag silence : quand Engram_Hive écrit lui-même, les événements notify
//     sont ignorés pendant 2s (utilisé par le futur module éditeur)
//
// Déviation assumée vs le brief : la table FTS5 est autonome (pas de
// content='files') car la table files ne stocke pas le corps du texte —
// le schéma externe du brief était incohérent. Documenté dans README_MODULE.md.
// ============================================================================

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use notify::Watcher;

use crate::config::Config;
use crate::stats::{self, Status};

/// Résultat d'une recherche corpus FTS.
#[derive(Debug, Clone)]
pub struct CorpusHit {
    pub path: PathBuf,
    pub file_stem: String,
    pub section: String,
    pub words_body: u64,
    pub snippet: String,
}

/// Stats d'un fichier indexé, publiées vers l'UI.
#[derive(Debug, Clone)]
pub struct FileStats {
    pub words_body: u64,
    /// Mots du frontmatter — exclus des stats littéraires, gardés pour la
    /// Phase 2 (exploration des métadonnées).
    #[allow(dead_code)]
    pub words_yaml: u64,
    pub chars: u64,
    pub goal: u64,
    pub status: Option<Status>,
    /// Section sémantique (architecture, texte, …) — déjà en base, l'UI
    /// par section arrive en Phase 2.
    #[allow(dead_code)]
    pub section: String,
}

/// Commandes UI → thread indexeur.
pub enum IndexerCmd {
    /// Re-parser un fichier précis (après une action du tree).
    Reindex(PathBuf),
    /// Retirer un fichier de l'index (suppression).
    Remove(PathBuf),
    /// Tout re-scanner.
    FullRescan,
    /// "C'est moi qui écris" : ignorer notify pendant la durée donnée.
    /// Pas encore émis en Cube 0 : c'est le futur module éditeur qui s'en
    /// servira à chaque sauvegarde (2s). La plomberie est posée.
    #[allow(dead_code)]
    Silence(Duration),
    Shutdown,
}

/// État partagé indexeur ↔ UI.
pub struct Shared {
    pub snapshot: RwLock<HashMap<PathBuf, FileStats>>,
    /// Incrémenté à chaque publication. L'UI compare et reconstruit l'arbre.
    pub generation: AtomicU64,
    /// Message de statut/progression à afficher (reconstruction d'index, erreurs).
    pub status_msg: Mutex<Option<String>>,
}

pub struct IndexerHandle {
    pub shared: Arc<Shared>,
    cmd_tx: Sender<Msg>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl IndexerHandle {
    pub fn send(&self, cmd: IndexerCmd) {
        let _ = self.cmd_tx.send(Msg::Cmd(cmd));
    }
}

impl Drop for IndexerHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Msg::Cmd(IndexerCmd::Shutdown));
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

enum Msg {
    Cmd(IndexerCmd),
    Fs(notify::Event),
}

/// Lance le thread indexeur pour un projet. `egui_ctx` sert uniquement à
/// demander un repaint quand une nouvelle génération est publiée.
pub fn spawn(root: PathBuf, cfg: Config, egui_ctx: egui::Context) -> Result<IndexerHandle, String> {
    let shared = Arc::new(Shared {
        snapshot: RwLock::new(HashMap::new()),
        generation: AtomicU64::new(0),
        status_msg: Mutex::new(None),
    });
    let (tx, rx) = std::sync::mpsc::channel::<Msg>();

    let watcher_tx = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            let _ = watcher_tx.send(Msg::Fs(ev));
        }
    })
    .map_err(|e| format!("Impossible de démarrer le watcher notify : {e}"))?;
    watcher
        .watch(&root, notify::RecursiveMode::Recursive)
        .map_err(|e| format!("Impossible de surveiller {} : {e}", root.display()))?;

    let shared2 = Arc::clone(&shared);
    let join = std::thread::Builder::new()
        .name("file_tree_indexer".into())
        .spawn(move || {
            // Le watcher doit vivre aussi longtemps que le thread.
            let _watcher = watcher;
            run(root, cfg, shared2, rx, egui_ctx);
        })
        .map_err(|e| format!("Impossible de lancer le thread indexeur : {e}"))?;

    Ok(IndexerHandle {
        shared,
        cmd_tx: tx,
        join: Some(join),
    })
}

/// Recherche corpus locale sur l'index SQLite du projet courant.
pub fn search_corpus(root: &Path, query: &str, limit: usize) -> Result<Vec<CorpusHit>, String> {
    let db_path = root.join(".engram").join("index.db");
    if !db_path.exists() {
        return Err(format!(
            "Aucune base d'index trouvée pour {}.",
            root.display()
        ));
    }
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "query_only", true)
        .map_err(|e| e.to_string())?;
    let query = normalize_fts_query(query);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "
            SELECT
                f.canonical_path,
                f.file_stem,
                f.section,
                f.word_count_body,
                snippet(fts_content, 1, '[', ']', ' … ', 12) AS excerpt
            FROM fts_content
            JOIN files f ON f.canonical_path = fts_content.canonical_path
            WHERE fts_content MATCH ?1
            ORDER BY bm25(fts_content)
            LIMIT ?2
            ",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![query, limit as i64], |row| {
            Ok(CorpusHit {
                path: PathBuf::from(row.get::<_, String>(0)?),
                file_stem: row.get(1)?,
                section: row.get(2)?,
                words_body: row.get::<_, i64>(3)? as u64,
                snippet: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut hits = Vec::new();
    for row in rows {
        hits.push(row.map_err(|e| e.to_string())?);
    }
    Ok(hits)
}

// ---------------------------------------------------------------------------
// Boucle principale du thread.
// ---------------------------------------------------------------------------

fn run(
    root: PathBuf,
    cfg: Config,
    shared: Arc<Shared>,
    rx: Receiver<Msg>,
    egui_ctx: egui::Context,
) {
    let db_path = root.join(".engram").join("index.db");
    let db_existed = db_path.exists();
    let mut conn = match open_db(&db_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!(
                "La base SQLite est corrompue ou inaccessible ({e}). J'aurais pu te le \
                 dire mais tu ne m'aurais pas écouté. Supprime .engram/index.db et \
                 relance : je reconstruis tout en 200ms."
            );
            tracing::error!(target: "file_tree", "{msg}");
            *shared
                .status_msg
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(msg);
            return;
        }
    };

    // Scan initial. Si la base n'existait pas, on le dit (critère 12 : index
    // supprimé → message visible de reconstruction).
    let t0 = Instant::now();
    let mut mem: HashMap<PathBuf, FileStats> = HashMap::new();
    let count = full_scan(&root, &cfg, &mut conn, &mut mem);
    let elapsed = t0.elapsed().as_millis();
    let msg = if db_existed {
        format!("Index à jour : {count} fichiers en {elapsed} ms.")
    } else {
        format!(
            "Index absent — reconstruit de zéro : {count} fichiers en {elapsed} ms. \
             Je t'avais dit que c'était rapide."
        )
    };
    tracing::info!(target: "file_tree", "{msg}");
    *shared
        .status_msg
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(msg);
    publish(&shared, &mem, &egui_ctx);

    let debounce = Duration::from_millis(cfg.vomi.notify_debounce_ms.max(10));
    let mut pending_changed: HashSet<PathBuf> = HashSet::new();
    let mut pending_removed: HashSet<PathBuf> = HashSet::new();
    let mut deadline: Option<Instant> = None;
    let mut silence_until: Option<Instant> = None;

    loop {
        let timeout = match deadline {
            Some(d) => d.saturating_duration_since(Instant::now()),
            None => Duration::from_secs(3600),
        };
        match rx.recv_timeout(timeout) {
            Ok(Msg::Cmd(IndexerCmd::Shutdown)) => break,
            Ok(Msg::Cmd(IndexerCmd::Silence(d))) => {
                silence_until = Some(Instant::now() + d);
            }
            Ok(Msg::Cmd(IndexerCmd::FullRescan)) => {
                mem.clear();
                let n = full_scan(&root, &cfg, &mut conn, &mut mem);
                *shared
                    .status_msg
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some(format!("Re-scan complet : {n} fichiers."));
                publish(&shared, &mem, &egui_ctx);
            }
            Ok(Msg::Cmd(IndexerCmd::Reindex(p))) => {
                pending_changed.insert(p);
                deadline = Some(Instant::now() + debounce);
            }
            Ok(Msg::Cmd(IndexerCmd::Remove(p))) => {
                pending_removed.insert(p);
                deadline = Some(Instant::now() + debounce);
            }
            Ok(Msg::Fs(ev)) => {
                // Flag silence : c'est Engram_Hive qui écrit, on ignore.
                if silence_until.map(|s| Instant::now() < s).unwrap_or(false) {
                    continue;
                }
                let removing = matches!(ev.kind, notify::EventKind::Remove(_));
                for p in ev.paths {
                    if !watchable(&root, &p, &cfg) {
                        continue;
                    }
                    if removing {
                        pending_removed.insert(p);
                    } else {
                        pending_changed.insert(p);
                    }
                }
                if !pending_changed.is_empty() || !pending_removed.is_empty() {
                    deadline = Some(Instant::now() + debounce);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if deadline.map(|d| Instant::now() >= d).unwrap_or(false) {
                    process_pending(
                        &root,
                        &cfg,
                        &mut conn,
                        &mut mem,
                        std::mem::take(&mut pending_changed),
                        std::mem::take(&mut pending_removed),
                    );
                    deadline = None;
                    publish(&shared, &mem, &egui_ctx);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Publie le snapshot et notifie l'UI.
fn publish(shared: &Shared, mem: &HashMap<PathBuf, FileStats>, egui_ctx: &egui::Context) {
    *shared
        .snapshot
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = mem.clone();
    shared.generation.fetch_add(1, Ordering::SeqCst);
    egui_ctx.request_repaint();
}

/// Un chemin nous intéresse-t-il ? (.typ ou extension extra, hors .engram/,
/// hors 06_en_cours/ — les liens pointent vers des originaux déjà indexés,
/// hors dossiers exclus par la config vomi.)
fn watchable(root: &Path, path: &Path, cfg: &Config) -> bool {
    let rel = match path.strip_prefix(root) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for comp in rel.components() {
        let c = comp.as_os_str().to_string_lossy();
        if c.starts_with(".engram") || crate::symlinks::is_session_folder(&c) || c.starts_with('.')
        {
            return false;
        }
        if cfg.vomi.exclude_dirs.iter().any(|d| d == c.as_ref()) {
            return false;
        }
    }
    stats::is_indexable(path, cfg)
}

// ---------------------------------------------------------------------------
// SQLite.
// ---------------------------------------------------------------------------

fn open_db(path: &Path) -> Result<rusqlite::Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
    // Migration douce : DB pré-existante sans la colonne date_marker (§5
    // Timeline). On absorbe "duplicate column" pour rester idempotent ;
    // toute autre erreur remonte normalement.
    if let Err(e) = conn.execute(
        "ALTER TABLE files ADD COLUMN date_marker TEXT NOT NULL DEFAULT ''",
        [],
    ) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") {
            return Err(format!("Migration date_marker : {msg}"));
        }
    }
    Ok(conn)
}

/// Schéma §3.1 du brief (+ colonne file_stem pour résoudre les wikilinks
/// orphelins en SQL, + FTS5 autonome — voir l'en-tête du fichier).
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files (
    canonical_path  TEXT PRIMARY KEY,
    file_stem       TEXT NOT NULL DEFAULT '',
    section         TEXT NOT NULL,
    word_count_body INTEGER DEFAULT 0,
    word_count_yaml INTEGER DEFAULT 0,
    character_count INTEGER DEFAULT 0,
    goal_words      INTEGER DEFAULT 0,
    last_modified   INTEGER NOT NULL,
    -- Marqueur de date discret extrait du corps : <!-- date:YYYY[-MM[-DD]] -->
    -- Normalisé zero-paddé pour tri lexicographique correct. Vide si absent.
    date_marker     TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS wikilinks (
    source_path TEXT,
    target_name TEXT,
    is_orphan   BOOLEAN DEFAULT TRUE,
    FOREIGN KEY(source_path) REFERENCES files(canonical_path)
);
CREATE TABLE IF NOT EXISTS tags (
    file_path TEXT,
    tag_name  TEXT,
    FOREIGN KEY(file_path) REFERENCES files(canonical_path)
);
CREATE VIRTUAL TABLE IF NOT EXISTS fts_content USING fts5(
    canonical_path UNINDEXED,
    body_text
);
-- Matrice Timeline : scènes (YAML date/lieu/ordre/personnages) et chronologies
-- internes des fiches personnages. date_sort = clé triable YYYYMMDDHHMM,
-- date_display = forme saisie (dd-mm-yyyy HHhMM). Tables alimentées en lecture
-- seule des fichiers ; absentes si l'auteur n'utilise pas ces champs YAML.
CREATE TABLE IF NOT EXISTS scenes (
    scene_path   TEXT PRIMARY KEY,
    date_sort    TEXT NOT NULL,
    date_display TEXT NOT NULL,
    lieu         TEXT NOT NULL DEFAULT '',
    ordre        INTEGER,
    FOREIGN KEY(scene_path) REFERENCES files(canonical_path)
);
CREATE TABLE IF NOT EXISTS scene_personnages (
    scene_path TEXT,
    perso      TEXT,
    FOREIGN KEY(scene_path) REFERENCES scenes(scene_path)
);
CREATE TABLE IF NOT EXISTS perso_chrono (
    perso_path   TEXT,
    perso_name   TEXT NOT NULL,
    date_sort    TEXT NOT NULL,
    date_display TEXT NOT NULL,
    note         TEXT NOT NULL DEFAULT '',
    FOREIGN KEY(perso_path) REFERENCES files(canonical_path)
);
-- §3.3 du brief 2/7/2026 : sources chronologie centralisées dans deux
-- fichiers globaux (biographies.typ, evenements.typ), une section == par
-- entrée. date_raw = tel qu'écrit dans le texte, date_sortable = YYYY-MM-DD
-- ou YYYY si parseable, NULL sinon (pas d'approximation inventée).
CREATE TABLE IF NOT EXISTS timeline_events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_path    TEXT NOT NULL,
    section_title  TEXT NOT NULL,
    date_raw       TEXT NOT NULL DEFAULT '',
    date_sortable  TEXT,
    body_excerpt   TEXT NOT NULL DEFAULT '',
    FOREIGN KEY(source_path) REFERENCES files(canonical_path)
);
CREATE TABLE IF NOT EXISTS timeline_links (
    event_id    INTEGER NOT NULL,
    target_name TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES timeline_events(id)
);
CREATE INDEX IF NOT EXISTS idx_timeline_events_source
    ON timeline_events(source_path);
CREATE INDEX IF NOT EXISTS idx_timeline_events_date
    ON timeline_events(date_sortable);
CREATE INDEX IF NOT EXISTS idx_timeline_links_event
    ON timeline_links(event_id);
";

/// Scan initial complet : vide les tables, re-parcourt tout le projet.
fn full_scan(
    root: &Path,
    cfg: &Config,
    conn: &mut rusqlite::Connection,
    mem: &mut HashMap<PathBuf, FileStats>,
) -> usize {
    let _ = conn.execute_batch(
        "DELETE FROM files; DELETE FROM wikilinks; DELETE FROM tags; DELETE FROM fts_content; \
         DELETE FROM timeline_links; DELETE FROM timeline_events;",
    );
    let mut paths = Vec::new();
    collect_typst(root, cfg, &mut paths);
    let tx = match conn.transaction() {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(target: "file_tree", "Transaction SQLite impossible : {e}");
            return 0;
        }
    };
    let mut n = 0;
    for p in &paths {
        if index_one(&tx, root, p, mem).is_ok() {
            n += 1;
        }
    }
    let _ = refresh_orphans(&tx);
    let _ = tx.commit();
    n
}

fn collect_typst(dir: &Path, cfg: &Config, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.')
            || crate::symlinks::is_session_folder(&name)
            || cfg.vomi.exclude_dirs.contains(&name)
        {
            continue;
        }
        // Ne pas suivre les symlinks pendant le scan (pas de doublons d'inode).
        let is_symlink = std::fs::symlink_metadata(&p)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true);
        if is_symlink {
            continue;
        }
        if p.is_dir() {
            collect_typst(&p, cfg, out);
        } else if stats::is_indexable(&p, cfg) {
            out.push(p);
        }
    }
}

/// Parse et indexe UN fichier (insert or replace). Cible < 5ms.
fn index_one(
    conn: &rusqlite::Connection,
    root: &Path,
    path: &Path,
    mem: &mut HashMap<PathBuf, FileStats>,
) -> Result<(), String> {
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let content = std::fs::read_to_string(&canonical).map_err(|e| e.to_string())?;
    let mtime = std::fs::metadata(&canonical)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let parsed = parse_typst(&content);
    let rel = canonical.strip_prefix(root).unwrap_or(&canonical);
    let section = stats::section_of(rel);
    let key = canonical.to_string_lossy().to_string();
    let stem = canonical
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    conn.execute(
        "INSERT OR REPLACE INTO files
         (canonical_path, file_stem, section, word_count_body, word_count_yaml,
          character_count, goal_words, last_modified, date_marker)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            key,
            stem,
            section,
            parsed.words_body as i64,
            parsed.words_yaml as i64,
            parsed.chars as i64,
            parsed.goal as i64,
            mtime,
            parsed.date_marker.clone().unwrap_or_default(),
        ],
    )
    .map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM wikilinks WHERE source_path = ?1", [&key])
        .map_err(|e| e.to_string())?;
    for target in &parsed.wikilinks {
        conn.execute(
            "INSERT INTO wikilinks (source_path, target_name, is_orphan) VALUES (?1, ?2, TRUE)",
            rusqlite::params![key, target],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.execute("DELETE FROM tags WHERE file_path = ?1", [&key])
        .map_err(|e| e.to_string())?;
    for tag in &parsed.tags {
        conn.execute(
            "INSERT INTO tags (file_path, tag_name) VALUES (?1, ?2)",
            rusqlite::params![key, tag],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.execute("DELETE FROM fts_content WHERE canonical_path = ?1", [&key])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO fts_content (canonical_path, body_text) VALUES (?1, ?2)",
        rusqlite::params![key, parsed.body_text],
    )
    .map_err(|e| e.to_string())?;

    // Matrice Timeline : scène (champ YAML `date`) + ses personnages.
    conn.execute("DELETE FROM scenes WHERE scene_path = ?1", [&key])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM scene_personnages WHERE scene_path = ?1",
        [&key],
    )
    .map_err(|e| e.to_string())?;
    if let Some(scene) = &parsed.scene {
        conn.execute(
            "INSERT INTO scenes (scene_path, date_sort, date_display, lieu, ordre)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                key,
                scene.date.sort,
                scene.date.display,
                scene.lieu.clone().unwrap_or_default(),
                scene.ordre,
            ],
        )
        .map_err(|e| e.to_string())?;
        for perso in &scene.personnages {
            conn.execute(
                "INSERT INTO scene_personnages (scene_path, perso) VALUES (?1, ?2)",
                rusqlite::params![key, perso],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    // Matrice Timeline : chronologie interne de la fiche personnage.
    conn.execute("DELETE FROM perso_chrono WHERE perso_path = ?1", [&key])
        .map_err(|e| e.to_string())?;
    for entry in &parsed.chrono {
        conn.execute(
            "INSERT INTO perso_chrono (perso_path, perso_name, date_sort, date_display, note)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![key, stem, entry.date.sort, entry.date.display, entry.note],
        )
        .map_err(|e| e.to_string())?;
    }

    // §3.3 — timeline_events / timeline_links pour les deux fichiers globaux
    // 01_architecture/chronologie/{biographies,evenements}.typ. Chaque section
    // ## devient un événement ; wikilinks internes = timeline_links.
    conn.execute(
        "DELETE FROM timeline_links WHERE event_id IN
            (SELECT id FROM timeline_events WHERE source_path = ?1)",
        [&key],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM timeline_events WHERE source_path = ?1", [&key])
        .map_err(|e| e.to_string())?;
    if is_chronologie_source(rel) {
        for section in parse_chronologie_sections(&parsed.body_text) {
            conn.execute(
                "INSERT INTO timeline_events
                    (source_path, section_title, date_raw, date_sortable, body_excerpt)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    key,
                    section.title,
                    section.date_raw,
                    section.date_sortable,
                    section.excerpt,
                ],
            )
            .map_err(|e| e.to_string())?;
            let event_id = conn.last_insert_rowid();
            for target in &section.wikilinks {
                conn.execute(
                    "INSERT INTO timeline_links (event_id, target_name) VALUES (?1, ?2)",
                    rusqlite::params![event_id, target],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    mem.insert(
        canonical,
        FileStats {
            words_body: parsed.words_body,
            words_yaml: parsed.words_yaml,
            chars: parsed.chars,
            goal: parsed.goal,
            status: parsed.status,
            section,
        },
    );
    Ok(())
}

fn remove_one(conn: &rusqlite::Connection, path: &Path, mem: &mut HashMap<PathBuf, FileStats>) {
    // Le fichier n'existe plus : on ne peut pas le canonicaliser. On retire
    // toute entrée mémoire/DB dont le chemin correspond (suffixe identique).
    let key_exact = path.to_string_lossy().to_string();
    let keys: Vec<PathBuf> = mem
        .keys()
        .filter(|k| k.as_path() == path || k.to_string_lossy() == key_exact)
        .cloned()
        .collect();
    for k in keys {
        mem.remove(&k);
    }
    for table in ["wikilinks", "tags"] {
        let col = if table == "wikilinks" {
            "source_path"
        } else {
            "file_path"
        };
        let _ = conn.execute(
            &format!("DELETE FROM {table} WHERE {col} = ?1"),
            [&key_exact],
        );
    }
    let _ = conn.execute("DELETE FROM files WHERE canonical_path = ?1", [&key_exact]);
    let _ = conn.execute(
        "DELETE FROM fts_content WHERE canonical_path = ?1",
        [&key_exact],
    );
}

/// Reclasse tous les wikilinks orphelins : un lien est orphelin si aucun
/// fichier indexé n'a un stem correspondant (insensible à la casse).
fn refresh_orphans(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE wikilinks SET is_orphan = NOT EXISTS (
             SELECT 1 FROM files f WHERE lower(f.file_stem) = lower(wikilinks.target_name)
         )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn process_pending(
    root: &Path,
    _cfg: &Config,
    conn: &mut rusqlite::Connection,
    mem: &mut HashMap<PathBuf, FileStats>,
    changed: HashSet<PathBuf>,
    removed: HashSet<PathBuf>,
) {
    let tx = match conn.transaction() {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(target: "file_tree", "Transaction SQLite impossible : {e}");
            return;
        }
    };
    for p in removed {
        remove_one(&tx, &p, mem);
    }
    for p in changed {
        if p.exists() {
            if let Err(e) = index_one(&tx, root, &p, mem) {
                tracing::warn!(target: "file_tree", "Indexation de {} ratée : {e}", p.display());
            }
        } else {
            // L'événement Modify d'un fichier déjà reparti (éditeurs à swap).
            remove_one(&tx, &p, mem);
        }
    }
    let _ = refresh_orphans(&tx);
    let _ = tx.commit();
}

// ---------------------------------------------------------------------------
// Parsing Typst : bloc de métadonnées en tête, corps, wikilinks, tags inline.
// ---------------------------------------------------------------------------

struct Parsed {
    words_yaml: u64,
    words_body: u64,
    chars: u64,
    goal: u64,
    status: Option<Status>,
    wikilinks: Vec<String>,
    tags: Vec<String>,
    body_text: String,
    /// Marqueur de date discret extrait du corps, premier `<!-- date:... -->`
    /// rencontré. Forme normalisée : "YYYY", "YYYY-MM" ou "YYYY-MM-DD".
    /// `None` si aucun marqueur valide.
    date_marker: Option<String>,
    /// Métadonnées scène (champ YAML `date`) — Matrice Timeline.
    scene: Option<crate::temporal::SceneMeta>,
    /// Chronologie interne d'une fiche personnage (champ YAML `chronologie`).
    chrono: Vec<crate::temporal::ChronoEntry>,
}

/// Sépare le bloc de métadonnées du corps. word_count_body = corps uniquement :
/// les mots des métadonnées ne polluent JAMAIS les stats littéraires.
fn parse_typst(content: &str) -> Parsed {
    let (yaml, body) = split_frontmatter(content);

    let words_yaml = yaml.split_whitespace().count() as u64;
    let words_body = body.split_whitespace().count() as u64;
    let chars = body.chars().count() as u64;

    let mut goal = 0u64;
    let mut status = None;
    for line in yaml.lines() {
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("goal:") {
            goal = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = trimmed.strip_prefix("statut:") {
            status = Status::from_yaml(v);
        } else if let Some(v) = trimmed.strip_prefix("status:") {
            status = Status::from_yaml(v);
        }
    }

    let temporal = crate::temporal::extract(yaml);

    Parsed {
        words_yaml,
        words_body,
        chars,
        goal,
        status,
        wikilinks: extract_wikilinks(body),
        tags: extract_tags(body),
        body_text: body.to_string(),
        date_marker: extract_date_marker(body),
        scene: temporal.scene,
        chrono: temporal.chrono,
    }
}

/// Extrait le premier marqueur `<!-- date:... -->` rencontré dans le corps.
/// Accepte trois formes (annéle seule, année + mois, date complète), avec
/// validation lâche : 4 chiffres an, 1–2 chiffres mois et jour. Espaces
/// autorisés autour de `date:`. Renvoie la forme normalisée zero-paddée
/// ("1942-03-15", "1942-03", "1942"). `None` si rien d'utilisable.
pub(crate) fn extract_date_marker(body: &str) -> Option<String> {
    // Recherche linéaire : on cible "<!--", on coupe au "-->", on regarde
    // si le contenu commence par "date:" (en tolérant espaces).
    let mut rest = body;
    while let Some(start) = rest.find("<!--") {
        let after = &rest[start + 4..];
        let Some(end) = after.find("-->") else { break };
        let inside = after[..end].trim();
        if let Some(value) = inside.strip_prefix("date:") {
            if let Some(norm) = normalize_date_marker(value.trim()) {
                return Some(norm);
            }
        }
        rest = &after[end + 3..];
    }
    None
}

fn normalize_date_marker(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.split('-').collect();
    let year: u32 = parts.first()?.parse().ok()?;
    if !(1..=9999).contains(&year) {
        return None;
    }
    match parts.len() {
        1 => Some(format!("{year:04}")),
        2 => {
            let month: u32 = parts[1].parse().ok()?;
            if !(1..=12).contains(&month) {
                return None;
            }
            Some(format!("{year:04}-{month:02}"))
        }
        3 => {
            let month: u32 = parts[1].parse().ok()?;
            let day: u32 = parts[2].parse().ok()?;
            if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
                return None;
            }
            Some(format!("{year:04}-{month:02}-{day:02}"))
        }
        _ => None,
    }
}

/// Retourne (métadonnées, corps). Sans bloc valide : métadonnées vides,
/// tout est corps. Le format accepté est soit un frontmatter YAML nu, soit un
/// frontmatter YAML encapsulé dans un commentaire Typst `/* ... */`.
fn split_frontmatter(content: &str) -> (&str, &str) {
    let trimmed = content.trim_start_matches('\u{feff}');
    let (prefix, comment_wrapped) = if let Some(rest) = trimmed.strip_prefix("---\n") {
        (rest, false)
    } else if let Some(rest) = trimmed.strip_prefix("/*\n---\n") {
        (rest, true)
    } else {
        return ("", content);
    };

    let Some(close_rel) = prefix.find("\n---\n").or_else(|| prefix.find("\n...\n")) else {
        return ("", content);
    };
    let yaml = &prefix[..close_rel];
    let after_close = &prefix[close_rel + 5..];
    let body = if comment_wrapped {
        after_close
            .strip_prefix("*/\n")
            .or_else(|| after_close.strip_prefix("*/"))
            .unwrap_or(after_close)
    } else {
        after_close
    };
    (yaml, body)
}

/// Extrait les cibles [[wikilink]] du corps. [[Svetlana|alias]] → "Svetlana".
fn extract_wikilinks(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end) = body[i + 2..].find("]]") {
                let inner = &body[i + 2..i + 2 + end];
                let target = inner.split('|').next().unwrap_or(inner).trim();
                if !target.is_empty() {
                    out.push(target.to_string());
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Extrait les tags inline (#climax, #mystere). Un tag commence par # suivi
/// d'un caractère alphanumérique, précédé d'un espace ou d'un début de ligne
/// (pour ne pas confondre avec une commande Typst `#let` ou `#table`).
fn extract_tags(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.lines() {
        let mut prev_is_boundary = true;
        let mut chars = line.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if c == '#' && prev_is_boundary {
                if let Some(&(_, next)) = chars.peek() {
                    if next.is_alphanumeric() {
                        let rest = &line[i + 1..];
                        let tag: String = rest
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                            .collect();
                        if !tag.is_empty() {
                            out.push(format!("#{tag}"));
                        }
                    }
                }
            }
            prev_is_boundary = c.is_whitespace();
        }
    }
    out
}

fn normalize_fts_query(input: &str) -> String {
    let mut terms = Vec::new();
    for raw in input.split_whitespace() {
        let term = raw
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '"' | '\''
                        | '`'
                        | ','
                        | ';'
                        | ':'
                        | '!'
                        | '?'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '='
                )
            })
            .replace('\"', "");
        if term.is_empty() {
            continue;
        }
        if term.len() == 1 {
            terms.push(term);
        } else {
            terms.push(format!("\"{term}\""));
        }
    }
    terms.join(" AND ")
}

// ---------------------------------------------------------------------------
// §3.3 — Parsing des deux fichiers globaux chronologie.
// ---------------------------------------------------------------------------

/// Une section `==` d'un fichier chronologie/{biographies,evenements}.typ.
pub(crate) struct ChronologieSection {
    pub title: String,
    pub date_raw: String,
    pub date_sortable: Option<String>,
    pub excerpt: String,
    pub wikilinks: Vec<String>,
}

/// Est-ce l'un des deux fichiers globaux chronologie ?
pub(crate) fn is_chronologie_source(rel: &Path) -> bool {
    let s = rel.to_string_lossy();
    // Compatible barres UNIX et Windows dans le chemin relatif.
    let s = s.replace('\\', "/");
    s == "01_architecture/chronologie/biographies.typ"
        || s == "01_architecture/chronologie/evenements.typ"
}

/// Découpe le corps d'un fichier chronologie en sections == et retourne
/// une entrée par section. La date, si parseable en YYYY-MM-DD ou YYYY,
/// remplit `date_sortable` ; sinon `None` (aucune approximation inventée,
/// brief §3.3). L'excerpt = 200 premiers caractères du corps de la section.
pub(crate) fn parse_chronologie_sections(body: &str) -> Vec<ChronologieSection> {
    let mut sections = Vec::new();
    let mut current: Option<(String, String)> = None;
    let mut buf = String::new();
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("== ") {
            if let Some((title, date_raw)) = current.take() {
                sections.push(build_section(&title, &date_raw, &buf));
                buf.clear();
            }
            let title = rest.trim().to_string();
            let date_raw = extract_first_date_in_title(&title);
            current = Some((title, date_raw));
        } else if current.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    if let Some((title, date_raw)) = current.take() {
        sections.push(build_section(&title, &date_raw, &buf));
    }
    sections
}

fn build_section(title: &str, date_raw: &str, body: &str) -> ChronologieSection {
    let date_sortable = parse_sortable_date(date_raw);
    let excerpt: String = body.trim().chars().take(200).collect();
    let wikilinks = extract_wikilinks(body);
    ChronologieSection {
        title: title.to_string(),
        date_raw: date_raw.to_string(),
        date_sortable,
        excerpt,
        wikilinks,
    }
}

/// Repère la première occurrence d'une date au format YYYY-MM-DD ou YYYY
/// dans le titre. Renvoie la sous-chaîne matchée telle quelle (le brief
/// impose date_raw stockée « telle qu'écrite »).
fn extract_first_date_in_title(title: &str) -> String {
    // Recherche linéaire d'une séquence 4-digits éventuellement suivie
    // de "-MM-DD". Retourne exactement ce qui a matché, sans normaliser.
    let bytes = title.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i..i + 4].iter().all(|b| b.is_ascii_digit()) {
            let mut end = i + 4;
            // "-MM"
            if end + 3 <= bytes.len()
                && bytes[end] == b'-'
                && bytes[end + 1..end + 3].iter().all(|b| b.is_ascii_digit())
            {
                end += 3;
                // "-DD"
                if end + 3 <= bytes.len()
                    && bytes[end] == b'-'
                    && bytes[end + 1..end + 3].iter().all(|b| b.is_ascii_digit())
                {
                    end += 3;
                }
            }
            return title[i..end].to_string();
        }
        i += 1;
    }
    String::new()
}

/// YYYY-MM-DD ou YYYY → normalisé pour tri lexicographique. Sinon `None`.
fn parse_sortable_date(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    normalize_date_marker(raw)
}

// ---------------------------------------------------------------------------
// Tests unitaires du parsing (la partie qui ne pardonne pas).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "/*\n---\ntags:\n  - personnage\ngoal: 1500\nstatut: Actif\n---\n*/\n\n= Svetlana Volkova\n\nCorps du texte ici avec [[Ivan]] et un tag #mystere.\n";

    #[test]
    fn frontmatter_separe_du_corps() {
        let p = parse_typst(SAMPLE);
        // Le YAML ne pollue JAMAIS le comptage littéraire.
        assert_eq!(p.words_yaml, 7); // tags: - personnage goal: 1500 statut: Actif
        assert_eq!(p.words_body, 13); // "=" du titre est un token whitespace-split
        assert_eq!(p.goal, 1500);
        // "Actif" n'est pas un statut connu : pas d'icône, pas d'erreur.
        assert_eq!(p.status, None);
    }

    #[test]
    fn sans_frontmatter_tout_est_corps() {
        let p = parse_typst("Juste trois mots.");
        assert_eq!(p.words_yaml, 0);
        assert_eq!(p.words_body, 3);
        assert_eq!(p.goal, 0);
    }

    #[test]
    fn date_marker_extrait_premier_match_valide() {
        assert_eq!(
            extract_date_marker("Avant <!-- date:1942-03-15 --> après"),
            Some("1942-03-15".to_string())
        );
        assert_eq!(
            extract_date_marker("<!--date:1942-3-7-->"),
            Some("1942-03-07".to_string())
        );
        assert_eq!(
            extract_date_marker("<!-- date: 1942 -->"),
            Some("1942".to_string())
        );
        assert_eq!(
            extract_date_marker("<!-- date:1942-12 -->"),
            Some("1942-12".to_string())
        );
        // Premier marqueur l'emporte
        assert_eq!(
            extract_date_marker("<!-- date:1900 --> et plus loin <!-- date:2000 -->"),
            Some("1900".to_string())
        );
    }

    #[test]
    fn date_marker_rejette_invalide() {
        assert_eq!(extract_date_marker("Pas de marqueur"), None);
        assert_eq!(extract_date_marker("<!-- date: -->"), None);
        assert_eq!(extract_date_marker("<!-- date:1942-13-01 -->"), None);
        assert_eq!(extract_date_marker("<!-- date:1942-12-32 -->"), None);
        assert_eq!(extract_date_marker("<!-- date:abc -->"), None);
    }

    #[test]
    fn statuts_reconnus() {
        for (v, expected) in [
            ("brouillon", Status::Brouillon),
            ("DRAFT", Status::Brouillon),
            ("wip", Status::EnCours),
            ("A_RELIRE", Status::ARelire),
            ("Ready", Status::Ready),
            ("blocked", Status::Bloque),
        ] {
            let md = format!("---\nstatut: {v}\n---\ncorps");
            assert_eq!(parse_typst(&md).status, Some(expected), "statut {v}");
        }
    }

    #[test]
    fn wikilinks_extraits_avec_alias() {
        let links =
            extract_wikilinks("Voir [[Svetlana]] et [[Bruxelles|la ville]]. [[]] vide ignoré.");
        assert_eq!(links, vec!["Svetlana".to_string(), "Bruxelles".to_string()]);
    }

    #[test]
    fn tags_inline_sans_titres_typst() {
        let tags = extract_tags("= Titre\nDu texte #climax et #mystere-2.\n== Sous-titre");
        assert_eq!(tags, vec!["#climax".to_string(), "#mystere-2".to_string()]);
    }
}
