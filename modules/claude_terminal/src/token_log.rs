// ============================================================================
// modules/claude_terminal/src/token_log.rs — Journal JSONL consommation tokens
//
// Chaque appel CLI claude réussi ajoute une ligne JSON dans
// ~/.local/share/engram_hive/claude_terminal_tokens.jsonl.
// Lecture cumulative pour affichage en bannière session.
// ============================================================================

use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenEntry {
    pub timestamp: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model: Option<String>,
}

fn tokens_path(data_dir: &Path) -> PathBuf {
    data_dir.join("claude_terminal_tokens.jsonl")
}

pub fn log_tokens(data_dir: &Path, entry: &TokenEntry) -> Result<(), String> {
    fs::create_dir_all(data_dir)
        .map_err(|e| format!("Création {} impossible : {e}", data_dir.display()))?;
    let path = tokens_path(data_dir);
    let line =
        serde_json::to_string(entry).map_err(|e| format!("Sérialisation token entry : {e}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Ouverture {} : {e}", path.display()))?;
    writeln!(file, "{line}").map_err(|e| format!("Écriture {} : {e}", path.display()))?;
    Ok(())
}

pub fn read_cumulative(data_dir: &Path) -> (u64, u64) {
    let path = tokens_path(data_dir);
    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return (0, 0),
    };
    let reader = std::io::BufReader::new(file);
    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<TokenEntry>(&line) {
            total_in = total_in.saturating_add(entry.input_tokens);
            total_out = total_out.saturating_add(entry.output_tokens);
        }
    }
    (total_in, total_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_and_read_cumulative() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join("engram_token_log_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;

        let e1 = TokenEntry {
            timestamp: "2026-06-30T10:00:00Z".into(),
            input_tokens: 100,
            output_tokens: 50,
            model: Some("claude-sonnet-4-6".into()),
        };
        let e2 = TokenEntry {
            timestamp: "2026-06-30T10:01:00Z".into(),
            input_tokens: 200,
            output_tokens: 80,
            model: None,
        };
        log_tokens(&dir, &e1)?;
        log_tokens(&dir, &e2)?;

        let (i, o) = read_cumulative(&dir);
        assert_eq!(i, 300);
        assert_eq!(o, 130);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
