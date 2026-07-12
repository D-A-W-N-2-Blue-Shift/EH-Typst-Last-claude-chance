use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
#[cfg(test)]
use engram_core::atomic_write;
use getrandom::getrandom;
use rusqlite::{params, Connection, OpenFlags};

#[derive(Debug, Clone)]
pub struct NoteTag {
    pub type_: String,
    pub valeur: String,
}

#[derive(Debug, Clone)]
pub struct NoteLink {
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct NoteRecord {
    pub id: String,
    pub contenu: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_path: Option<PathBuf>,
    pub anchor_line: Option<i64>,
    pub tags: Vec<NoteTag>,
    pub links: Vec<NoteLink>,
    pub orphan: bool,
}

#[derive(Debug, Clone)]
pub struct NoteDraft {
    pub id: String,
    pub contenu: String,
    pub source_path: Option<PathBuf>,
    pub anchor_line: Option<i64>,
    pub tags: Vec<NoteTag>,
    pub links: Vec<NoteLink>,
}

impl NoteDraft {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            id: new_uuid_v4()?,
            contenu: String::new(),
            source_path: None,
            anchor_line: Some(1),
            tags: Vec::new(),
            links: Vec::new(),
        })
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Newline {
    Lf,
    Crlf,
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct TextSnapshot {
    lines: Vec<String>,
    newline: Newline,
    trailing_newline: bool,
}

#[cfg(test)]
impl TextSnapshot {
    fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Impossible de lire {} : {e}", path.display()))?;
        Ok(Self::from_text(&text))
    }

    fn from_text(text: &str) -> Self {
        let newline = if text.contains("\r\n") {
            Newline::Crlf
        } else {
            Newline::Lf
        };
        let trailing_newline = text.ends_with('\n');
        let mut lines = text
            .split('\n')
            .map(|line| {
                if matches!(newline, Newline::Crlf) {
                    line.strip_suffix('\r').unwrap_or(line).to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>();
        if trailing_newline {
            let _ = lines.pop();
        }
        Self {
            lines,
            newline,
            trailing_newline,
        }
    }

    fn insert_line(&mut self, index: usize, line: String) {
        let index = index.min(self.lines.len());
        self.lines.insert(index, line);
    }

    fn remove_matching_line(&mut self, marker: &str) -> bool {
        let before = self.lines.len();
        self.lines.retain(|line| !line.contains(marker));
        before != self.lines.len()
    }

    fn to_text(&self) -> String {
        let delim = match self.newline {
            Newline::Lf => "\n",
            Newline::Crlf => "\r\n",
        };
        let mut out = self.lines.join(delim);
        if self.trailing_newline && (!out.is_empty() || !self.lines.is_empty()) {
            out.push_str(delim);
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub notes: Vec<NoteRecord>,
    pub note_counts: BTreeMap<PathBuf, usize>,
}

pub fn db_path(project_root: &Path) -> PathBuf {
    project_root.join(".engram").join("index.db")
}

fn open_db(project_root: &Path) -> Result<Connection, String> {
    let path = db_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Impossible de créer {} : {e}", parent.display()))?;
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(2000))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS notes (
            id          TEXT PRIMARY KEY,
            contenu     TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            source_path TEXT,
            anchor_line INTEGER
        );
        CREATE TABLE IF NOT EXISTS note_tags (
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            type    TEXT NOT NULL,
            valeur  TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS note_links (
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            target  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_note_tags_note ON note_tags(note_id);
        CREATE INDEX IF NOT EXISTS idx_note_links_note ON note_links(note_id);
        CREATE INDEX IF NOT EXISTS idx_note_links_target ON note_links(target);
        ",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

pub fn load_notes(project_root: &Path) -> Result<Vec<NoteRecord>, String> {
    let conn = open_db(project_root)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, contenu, created_at, updated_at, source_path, anchor_line
             FROM notes
             ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(NoteRecord {
                id: row.get(0)?,
                contenu: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                source_path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
                anchor_line: row.get(5)?,
                tags: Vec::new(),
                links: Vec::new(),
                orphan: false,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut notes = Vec::new();
    for row in rows {
        let mut note = row.map_err(|e| e.to_string())?;
        note.tags = load_tags(&conn, &note.id)?;
        note.links = load_links(&conn, &note.id)?;
        notes.push(note);
    }
    Ok(notes)
}

pub fn load_note(project_root: &Path, id: &str) -> Result<Option<NoteRecord>, String> {
    Ok(load_notes(project_root)?.into_iter().find(|n| n.id == id))
}

pub fn load_counts(project_root: &Path) -> Result<BTreeMap<PathBuf, usize>, String> {
    let conn = open_db(project_root)?;
    let mut stmt = conn
        .prepare(
            "SELECT source_path, COUNT(*)
             FROM notes
             WHERE source_path IS NOT NULL AND source_path <> ''
             GROUP BY source_path",
        )
        .map_err(|e| e.to_string())?;
    let mut out = BTreeMap::new();
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (path, count) = row.map_err(|e| e.to_string())?;
        if let Some(path) = path {
            out.insert(PathBuf::from(path), count.max(0) as usize);
        }
    }
    Ok(out)
}

pub fn save_note(
    project_root: &Path,
    draft: &NoteDraft,
    original: Option<&NoteRecord>,
) -> Result<(), String> {
    let mut conn = open_db(project_root)?;
    if let Some(path) = draft.source_path.as_ref() {
        if !path.exists() {
            return Err(format!(
                "Le fichier source {} est introuvable.",
                path.display()
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    let created_at = original
        .map(|n| n.created_at.clone())
        .unwrap_or_else(|| now.clone());
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO notes (id, contenu, created_at, updated_at, source_path, anchor_line)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            contenu=excluded.contenu,
            updated_at=excluded.updated_at,
            source_path=excluded.source_path,
            anchor_line=excluded.anchor_line",
        params![
            draft.id,
            draft.contenu,
            created_at,
            now,
            draft
                .source_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            draft.anchor_line,
        ],
    )
    .map_err(|e| e.to_string())?;

    tx.execute(
        "DELETE FROM note_tags WHERE note_id = ?1",
        params![draft.id],
    )
    .map_err(|e| e.to_string())?;
    for tag in &draft.tags {
        tx.execute(
            "INSERT INTO note_tags (note_id, type, valeur) VALUES (?1, ?2, ?3)",
            params![draft.id, tag.type_, tag.valeur],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.execute(
        "DELETE FROM note_links WHERE note_id = ?1",
        params![draft.id],
    )
    .map_err(|e| e.to_string())?;
    for link in &draft.links {
        tx.execute(
            "INSERT INTO note_links (note_id, target) VALUES (?1, ?2)",
            params![draft.id, link.target],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_note(project_root: &Path, id: &str) -> Result<(), String> {
    let conn = open_db(project_root)?;
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn sync_from_project(project_root: &Path) -> Result<ScanResult, String> {
    let conn = open_db(project_root)?;
    let files = collect_text_files(project_root)?;
    let notes = load_notes(project_root)?;
    let current_files: HashSet<PathBuf> = files.iter().cloned().collect();
    let mut by_id: HashMap<String, (PathBuf, usize)> = HashMap::new();
    for file in &files {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(_) => continue,
        };
        for (line_no, line) in text.lines().enumerate() {
            if let Some(id) = parse_marker_id(line) {
                by_id.insert(id, (file.clone(), line_no + 1));
            }
        }
    }

    reconcile_link_targets(&conn, &files)?;

    for note in &notes {
        if let Some((path, line)) = by_id.get(&note.id) {
            conn.execute(
                "UPDATE notes SET source_path = ?1, anchor_line = ?2 WHERE id = ?3",
                params![path.to_string_lossy().to_string(), *line as i64, note.id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    let counts = load_counts(project_root)?;
    Ok(ScanResult {
        notes: {
            let mut notes = load_notes(project_root)?;
            mark_orphans(&mut notes, &current_files);
            notes
        },
        note_counts: counts,
    })
}

pub fn mark_orphans(notes: &mut [NoteRecord], current_files: &HashSet<PathBuf>) {
    for note in notes {
        note.orphan = match note.source_path.as_ref() {
            Some(path) => !current_files.contains(path),
            None => true,
        };
    }
}

pub fn publish_counts(
    counts: &BTreeMap<PathBuf, usize>,
    previous: &mut BTreeMap<PathBuf, usize>,
    out: &mut Vec<engram_core::ModuleResponse>,
) {
    for (path, count) in counts {
        if previous.get(path).copied().unwrap_or_default() != *count {
            out.push(engram_core::ModuleResponse::PublishNoteIndex {
                file_path: path.clone(),
                note_count: *count,
            });
            previous.insert(path.clone(), *count);
        }
    }
    let stale: Vec<PathBuf> = previous
        .keys()
        .filter(|path| !counts.contains_key(*path))
        .cloned()
        .collect();
    for path in stale {
        out.push(engram_core::ModuleResponse::PublishNoteIndex {
            file_path: path.clone(),
            note_count: 0,
        });
        previous.remove(&path);
    }
}

#[cfg(test)]
pub fn note_marker(path: &Path, id: &str) -> String {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("typ"))
    {
        format!("/* note:{id} */")
    } else {
        format!("<!-- note:{id} -->")
    }
}

#[cfg(test)]
pub fn insert_marker(path: &Path, anchor_line: i64, id: &str) -> Result<(), String> {
    let mut snapshot = TextSnapshot::read(path)?;
    let marker = note_marker(path, id);
    if snapshot.lines.iter().any(|line| line.contains(&marker)) {
        return Ok(());
    }
    let pos = anchor_line.max(1) as usize;
    let insert_at = pos.saturating_sub(1).min(snapshot.lines.len());
    snapshot.insert_line(insert_at, marker);
    atomic_write(path, snapshot.to_text().as_bytes())
        .map_err(|e| format!("Impossible d'écrire {} : {e}", path.display()))
}

#[cfg(test)]
pub fn remove_marker(path: &Path, id: &str) -> Result<(), String> {
    let mut snapshot = match TextSnapshot::read(path) {
        Ok(snapshot) => snapshot,
        Err(e) if e.contains("No such file") || e.contains("not found") => return Ok(()),
        Err(e) => return Err(e),
    };
    let marker = note_marker(path, id);
    if snapshot.remove_matching_line(&marker) {
        atomic_write(path, snapshot.to_text().as_bytes())
            .map_err(|e| format!("Impossible d'écrire {} : {e}", path.display()))?;
    }
    Ok(())
}

fn collect_text_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect_text_files_rec(root, &mut out)?;
    Ok(out)
}

fn collect_text_files_rec(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let rd = match std::fs::read_dir(root) {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') || name == "target" || name == "render-cache" {
            continue;
        }
        if path.is_dir() {
            collect_text_files_rec(&path, out)?;
            continue;
        }
        if is_text_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_text_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "typ" | "md" | "txt"
    )
}

fn parse_marker_id(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let marker = trimmed
        .strip_prefix("<!-- note:")
        .and_then(|s| s.strip_suffix("-->"))
        .or_else(|| {
            trimmed
                .strip_prefix("/* note:")
                .and_then(|s| s.strip_suffix("*/"))
        })?;
    let id = marker.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn new_uuid_v4() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom(&mut bytes).map_err(|e| format!("UUID Sticky Notes indisponible : {e}"))?;
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

fn load_tags(conn: &Connection, note_id: &str) -> Result<Vec<NoteTag>, String> {
    let mut stmt = conn
        .prepare("SELECT type, valeur FROM note_tags WHERE note_id = ?1 ORDER BY rowid")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![note_id], |row| {
            Ok(NoteTag {
                type_: row.get(0)?,
                valeur: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn load_links(conn: &Connection, note_id: &str) -> Result<Vec<NoteLink>, String> {
    let mut stmt = conn
        .prepare("SELECT target FROM note_links WHERE note_id = ?1 ORDER BY rowid")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![note_id], |row| {
            Ok(NoteLink {
                target: row.get(0)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn reconcile_link_targets(conn: &Connection, files: &[PathBuf]) -> Result<(), String> {
    let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
    for file in files {
        if let Some(name) = file.file_name().map(|n| n.to_string_lossy().to_string()) {
            by_name
                .entry(name)
                .or_default()
                .push(file.to_string_lossy().to_string());
        }
    }

    let mut stmt = conn
        .prepare("SELECT rowid, target FROM note_links")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (rowid, target) = row.map_err(|e| e.to_string())?;
        let target_path = Path::new(&target);
        if target_path.exists() {
            continue;
        }
        let Some(name) = target_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
        else {
            continue;
        };
        if let Some(candidates) = by_name.get(&name) {
            if candidates.len() == 1 {
                conn.execute(
                    "UPDATE note_links SET target = ?1 WHERE rowid = ?2",
                    params![candidates[0], rowid],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(ext: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("sticky_notes_{stamp}.{ext}"))
    }

    #[test]
    fn marker_format_depends_on_extension() {
        let md = note_marker(Path::new("note.md"), "abc");
        let typ = note_marker(Path::new("note.typ"), "abc");
        assert!(md.contains("<!-- note:abc -->"));
        assert!(typ.contains("/* note:abc */"));
    }

    #[test]
    fn insert_then_remove_marker_roundtrip() {
        let path = temp_file("md");
        atomic_write(&path, "ligne 1\nligne 2\n").unwrap();
        insert_marker(&path, 2, "abc").unwrap();
        let after_insert = std::fs::read_to_string(&path).unwrap();
        assert!(after_insert.contains("<!-- note:abc -->"));
        assert!(after_insert.ends_with('\n'));
        remove_marker(&path, "abc").unwrap();
        let after_remove = std::fs::read_to_string(&path).unwrap();
        assert!(!after_remove.contains("note:abc"));
        assert_eq!(after_remove, "ligne 1\nligne 2\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn insert_and_remove_preserve_crlf_and_final_newline() {
        let path = temp_file("typ");
        atomic_write(&path, "ligne 1\r\nligne 2\r\n").unwrap();
        insert_marker(&path, 1, "abc").unwrap();
        let after_insert = std::fs::read_to_string(&path).unwrap();
        assert!(after_insert.contains("/* note:abc */"));
        assert!(after_insert.contains("\r\n"));
        assert!(after_insert.ends_with("\r\n"));
        remove_marker(&path, "abc").unwrap();
        let after_remove = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after_remove, "ligne 1\r\nligne 2\r\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn note_draft_new_generates_v4_uuid() {
        let draft = NoteDraft::new().unwrap();
        assert_eq!(draft.id.len(), 36);
        assert_eq!(&draft.id[14..15], "4");
        assert!(matches!(
            draft.id.chars().nth(19),
            Some('8' | '9' | 'a' | 'b')
        ));
    }
}
