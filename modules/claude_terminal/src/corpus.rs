use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CorpusHit {
    pub path: PathBuf,
    pub file_stem: String,
    pub section: String,
    pub words_body: u64,
    pub snippet: String,
}

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
            r#"
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
            "#,
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

fn normalize_fts_query(input: &str) -> String {
    let mut out = Vec::new();
    for term in input.split_whitespace() {
        let cleaned: String = term
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !cleaned.is_empty() {
            out.push(cleaned);
        }
    }
    out.join(" ")
}

pub fn read_excerpt(path: &Path, limit: usize) -> Result<String, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("Lecture {} impossible : {e}", path.display()))?;
    Ok(truncate_chars(&raw, limit))
}

pub fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("…");
            break;
        }
        out.push(ch);
    }
    out
}
