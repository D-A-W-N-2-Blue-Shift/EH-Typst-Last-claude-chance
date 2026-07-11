// ============================================================================
// modules/editor/src/stats.rs — Comptage de mots, objectifs, session
//
// Status bar :
//   mots: 1247 | caractères: 7843 | L42 C18 | session: +312 | [=====>  ] 1247/2000
//
// - Le comptage du fichier ENTIER est O(N) → débouncé (200 ms après la
//   dernière frappe) et exécuté dans un thread, résultat par mpsc. Le
//   thread UI ne compte jamais tout le fichier.
// - Le frontmatter est exclu du compte littéraire ; il est parsé (YAML)
//   pour en sortir goal:, language:, smart_typography:.
// - Les lignes commençant par le préfixe commentaire (%) sont exclues.
// - session: +N = mots écrits depuis l'ouverture de la fenêtre.
// - Objectif atteint → barre verte + "Objectif atteint." pendant 3 s.
// ============================================================================

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use ropey::Rope;

use crate::highlight::detect_frontmatter;

/// Métadonnées extraites du frontmatter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileMeta {
    pub goal: Option<u64>,
    pub language: Option<String>,
    pub smart_typography: Option<bool>,
}

struct StatsResult {
    generation: u64,
    words: u64,
    meta: FileMeta,
}

pub struct StatsState {
    pub words: u64,
    pub meta: FileMeta,
    /// Mots au moment de l'ouverture (référence du compteur de session).
    pub session_start: Option<u64>,
    /// L'objectif vient d'être atteint (pour le message 3 s).
    pub goal_reached_at: Option<Instant>,
    // --- débounce + thread ---
    last_version: u64,
    dirty_since: Option<Instant>,
    generation: u64,
    rx: Option<Receiver<StatsResult>>,
    tx_keep: Option<Sender<StatsResult>>,
    // --- cache du comptage de sélection ---
    sel_cache: Option<((usize, usize), u64, u64)>, // (range, version, mots)
}

impl Default for StatsState {
    fn default() -> Self {
        Self {
            words: 0,
            meta: FileMeta::default(),
            session_start: None,
            goal_reached_at: None,
            last_version: u64::MAX,
            dirty_since: None,
            generation: 0,
            rx: None,
            tx_keep: None,
            sel_cache: None,
        }
    }
}

impl StatsState {
    /// À appeler chaque frame. Lance un comptage débouncé si le buffer a
    /// changé, récolte les résultats. `debounce_ms` vient d'licorne-a-gerber_editor.ron.
    pub fn tick(&mut self, rope: &Rope, version: u64, comment_prefix: &str, debounce_ms: u64) {
        if version != self.last_version {
            self.last_version = version;
            self.dirty_since = Some(Instant::now());
        }
        let due = self
            .dirty_since
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(debounce_ms));
        if due {
            self.dirty_since = None;
            self.spawn_count(rope.clone(), comment_prefix.to_string());
        }
        if let Some(rx) = &self.rx {
            while let Ok(res) = rx.try_recv() {
                if res.generation != self.generation {
                    continue;
                }
                let was_below = self.meta.goal.map(|g| self.words < g);
                self.words = res.words;
                self.meta = res.meta;
                if self.session_start.is_none() {
                    self.session_start = Some(res.words);
                }
                if let (Some(goal), Some(true)) = (self.meta.goal, was_below) {
                    if res.words >= goal {
                        self.goal_reached_at = Some(Instant::now());
                    }
                }
            }
        }
    }

    fn spawn_count(&mut self, rope: Rope, comment_prefix: String) {
        self.generation += 1;
        let generation = self.generation;
        let (tx, rx) = match (&self.tx_keep, &self.rx) {
            (Some(tx), Some(_)) => (tx.clone(), None),
            _ => {
                let (tx, rx) = std::sync::mpsc::channel();
                self.tx_keep = Some(tx.clone());
                (tx, Some(rx))
            }
        };
        if let Some(rx) = rx {
            self.rx = Some(rx);
        }
        std::thread::Builder::new()
            .name("editor-stats".into())
            .spawn(move || {
                // O(N) ASSUMÉ : thread dédié, jamais l'UI.
                let fm_end = detect_frontmatter(&rope);
                let meta = parse_meta(&rope, fm_end);
                let words = count_words(&rope, fm_end, &comment_prefix);
                let _ = tx.send(StatsResult {
                    generation,
                    words,
                    meta,
                });
            })
            .ok();
    }

    /// Mots écrits depuis l'ouverture (peut être négatif : on coupe aussi).
    pub fn session_delta(&self) -> i64 {
        self.words as i64 - self.session_start.unwrap_or(self.words) as i64
    }

    /// Mots de la sélection — caché par (intervalle, version) : O(sélection)
    /// au premier calcul, O(1) tant que rien ne bouge.
    pub fn selection_words(&mut self, rope: &Rope, sel: (usize, usize), version: u64) -> u64 {
        if let Some((r, v, w)) = self.sel_cache {
            if r == sel && v == version {
                return w;
            }
        }
        let (s, e) = (sel.0.min(rope.len_chars()), sel.1.min(rope.len_chars()));
        let text: String = rope.slice(s..e).to_string();
        let w = text.split_whitespace().count() as u64;
        self.sel_cache = Some((sel, version, w));
        w
    }

    /// Le message "Objectif atteint." est-il encore d'actualité ?
    pub fn goal_banner(&self) -> bool {
        self.goal_reached_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(3))
    }
}

/// Compte les mots hors frontmatter et hors lignes commentaire.
fn count_words(rope: &Rope, fm_end: usize, comment_prefix: &str) -> u64 {
    let mut words = 0u64;
    for (i, line) in rope.lines().enumerate() {
        if i < fm_end {
            continue;
        }
        let s: String = line.to_string();
        let t = s.trim_start();
        if !comment_prefix.is_empty() && t.starts_with(comment_prefix) {
            continue;
        }
        words += s.split_whitespace().count() as u64;
    }
    words
}

/// Parse le frontmatter YAML : goal, language, smart_typography.
/// Tolérant : un YAML cassé = pas de méta, pas de drame (le fichier est
/// peut-être en cours de frappe).
fn parse_meta(rope: &Rope, fm_end: usize) -> FileMeta {
    if fm_end < 2 {
        return FileMeta::default();
    }
    let start = rope.line_to_char(1);
    let end = rope.line_to_char(fm_end - 1);
    let yaml: String = rope.slice(start..end).to_string();
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&yaml) else {
        return FileMeta::default();
    };
    let goal = value.get("goal").and_then(|v| v.as_u64());
    let language = value
        .get("language")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let smart_typography = value.get("smart_typography").and_then(|v| v.as_bool());
    FileMeta {
        goal,
        language,
        smart_typography,
    }
}

/// La barre de progression texte : [=====>    ] 1247/2000.
pub fn progress_bar(words: u64, goal: u64) -> String {
    let width = 10usize;
    let ratio = if goal == 0 {
        1.0
    } else {
        (words as f64 / goal as f64).min(1.0)
    };
    let filled = (ratio * width as f64).floor() as usize;
    let mut bar = String::with_capacity(width + 2);
    bar.push('[');
    for i in 0..width {
        bar.push(if i < filled {
            '='
        } else if i == filled && ratio < 1.0 {
            '>'
        } else {
            ' '
        });
    }
    bar.push(']');
    format!("{bar} {words}/{goal}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_exclude_frontmatter_and_comments() {
        let rope = Rope::from_str(
            "---\ngoal: 2000\nlanguage: en\n---\nDeux mots ici.\n% commentaire pas compté\nEt trois mots là\n",
        );
        let fm = detect_frontmatter(&rope);
        assert_eq!(fm, 4);
        assert_eq!(count_words(&rope, fm, "%"), 7);
        let meta = parse_meta(&rope, fm);
        assert_eq!(meta.goal, Some(2000));
        assert_eq!(meta.language.as_deref(), Some("en"));
    }

    #[test]
    fn meta_tolerates_broken_yaml() {
        let rope = Rope::from_str("---\n: : :\n---\ncorps\n");
        let fm = detect_frontmatter(&rope);
        assert_eq!(parse_meta(&rope, fm), FileMeta::default());
    }

    #[test]
    fn progress_bar_shape() {
        assert_eq!(progress_bar(0, 2000), "[>         ] 0/2000");
        assert_eq!(progress_bar(1000, 2000), "[=====>    ] 1000/2000");
        assert_eq!(progress_bar(2400, 2000), "[==========] 2400/2000");
    }

    #[test]
    fn debounced_count_arrives() {
        let rope = Rope::from_str("un deux trois quatre");
        let mut s = StatsState::default();
        s.tick(&rope, 1, "%", 0);
        let deadline = Instant::now() + Duration::from_secs(2);
        while s.words == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            s.tick(&rope, 1, "%", 0);
        }
        assert_eq!(s.words, 4);
        assert_eq!(s.session_delta(), 0);
    }
}
