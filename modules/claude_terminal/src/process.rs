// ============================================================================
// modules/claude_terminal/src/process.rs — Spawn du CLI `claude`
//
// Invocation one-shot via `claude --print` (pas de pty interactif). Le
// process tourne sur un thread dédié ; le résultat revient par channel.
// Env CI=true supprime les animations. Mode read-only via permission flag.
// ============================================================================

use std::path::Path;
use std::process::Command;
use std::sync::mpsc;

use crate::config::{AssistantCliProvider, AuthMode, ClaudeTerminalConfig};

#[derive(Debug, Clone)]
pub struct ClaudeResponse {
    pub text: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub model: Option<String>,
}

pub fn spawn_query(
    question: String,
    cwd: std::path::PathBuf,
    config: ClaudeTerminalConfig,
) -> mpsc::Receiver<Result<ClaudeResponse, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = run_query(&question, &cwd, &config);
        let _ = tx.send(result);
    });
    rx
}

fn run_query(
    question: &str,
    cwd: &Path,
    config: &ClaudeTerminalConfig,
) -> Result<ClaudeResponse, String> {
    let (default_binary, default_args) = config.provider.defaults();
    let binary = if config.command.trim().is_empty() {
        default_binary
    } else {
        config.command.trim()
    };
    let args = if config.args.trim().is_empty() {
        default_args
    } else {
        config.args.trim()
    };

    let mut cmd = Command::new(binary);
    for arg in split_args(args) {
        cmd.arg(arg);
    }
    cmd.arg(question)
        .current_dir(cwd)
        .env("CI", "true")
        .env("NO_COLOR", "1");

    if config.auth_mode == AuthMode::ApiKey {
        if let Some(ref key) = config.api_key {
            cmd.env("ANTHROPIC_API_KEY", key);
        }
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Impossible de lancer `{binary}` : {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "{binary} a retourné le code {} : {}{}",
            output.status.code().unwrap_or(-1),
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!("\nSortie brute : {}", stdout.trim())
            }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_response(&stdout, config.provider)
}

fn parse_response(raw: &str, provider: AssistantCliProvider) -> Result<ClaudeResponse, String> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        return Ok(extract_json_response(v));
    }
    Ok(ClaudeResponse {
        text: strip_ansi(raw).trim().to_string(),
        input_tokens: None,
        output_tokens: None,
        model: Some(provider.label().to_string()),
    })
}

fn extract_json_response(v: serde_json::Value) -> ClaudeResponse {
    let text = extract_text_from_json(&v);
    let input_tokens = v
        .get("usage")
        .and_then(|u| u.get("input_tokens"))
        .and_then(|t| t.as_u64())
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("usage"))
                .and_then(|u| u.get("input_tokens"))
                .and_then(|t| t.as_u64())
        });

    let output_tokens = v
        .get("usage")
        .and_then(|u| u.get("output_tokens"))
        .and_then(|t| t.as_u64())
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("usage"))
                .and_then(|u| u.get("output_tokens"))
                .and_then(|t| t.as_u64())
        });

    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("model"))
                .and_then(|m| m.as_str())
        })
        .map(|s| s.to_string());

    ClaudeResponse {
        text,
        input_tokens,
        output_tokens,
        model,
    }
}

fn extract_text_from_json(v: &serde_json::Value) -> String {
    if let Some(result) = v.get("result") {
        if let Some(text) = result.as_str() {
            return strip_ansi(text);
        }
    }

    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        return strip_ansi(text);
    }

    if let Some(content) = v.get("content").and_then(|c| c.as_array()) {
        let mut parts = Vec::new();
        for item in content {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                parts.push(text);
            }
        }
        if !parts.is_empty() {
            return strip_ansi(&parts.join("\n"));
        }
    }

    strip_ansi(&v.to_string())
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn split_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in raw.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_sequences() {
        assert_eq!(strip_ansi("\x1b[31mhello\x1b[0m"), "hello");
        assert_eq!(strip_ansi("no ansi here"), "no ansi here");
    }

    #[test]
    fn parse_simple_json_response() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{"result": "Hello world", "usage": {"input_tokens": 10, "output_tokens": 5}, "model": "claude-sonnet-4-6"}"#;
        let resp = parse_response(json, AssistantCliProvider::Claude)?;
        assert_eq!(resp.text, "Hello world");
        assert_eq!(resp.input_tokens, Some(10));
        assert_eq!(resp.output_tokens, Some(5));
        assert_eq!(resp.model.as_deref(), Some("claude-sonnet-4-6"));
        Ok(())
    }

    #[test]
    fn split_args_handles_quotes() {
        assert_eq!(
            split_args(r#"--json --message "hello world" --flag"#),
            vec!["--json", "--message", "hello world", "--flag"]
        );
    }
}
