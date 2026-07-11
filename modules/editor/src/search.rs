// ============================================================================
// modules/editor/src/search.rs — Recherche / remplacement dans le fichier
//
// Panel Ctrl+F (recherche) / Ctrl+H (+ remplacement). Occurrences
// surlignées, compteur "3/12", précédent/suivant, case/mot entier/regex.
//
// Règle concurrence : la recherche (regex comprise) tourne dans un thread
// séparé sur un CLONE du rope (clone O(1) chez ropey) et répond par mpsc.
// Si une fenêtre lance une regex pathologique sur 200 pages, les autres
// fenêtres ne freezent pas. Un compteur de génération écarte les résultats
// périmés.
//
// Le REMPLACEMENT, lui, mute le buffer : il se fait sur le thread UI en
// une transaction (O(occurrences), action explicite de l'utilisateur).
// ============================================================================

use std::sync::mpsc::{Receiver, Sender};

use ropey::Rope;

/// Résultat d'un thread de recherche.
struct SearchResult {
    generation: u64,
    ranges: Vec<(usize, usize)>, // chars globaux, triés
    error: Option<String>,
}

#[derive(Default)]
pub struct SearchState {
    pub open: bool,
    pub replace_open: bool,
    pub query: String,
    pub replace: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    /// Occurrences (indices caractères globaux), triées.
    pub ranges: Vec<(usize, usize)>,
    /// Occurrence courante (index dans ranges).
    pub current: usize,
    /// Regex invalide : message GLaDOS doux affiché dans le panel.
    pub error: Option<String>,
    /// Le champ de recherche doit prendre le focus (ouverture, Ctrl+F).
    pub want_focus: bool,
    // --- plomberie du thread ---
    generation: u64,
    last_sent: Option<(String, bool, bool, bool, u64)>,
    rx: Option<Receiver<SearchResult>>,
    tx_keep: Option<Sender<SearchResult>>,
}

impl SearchState {
    pub fn open_search(&mut self) {
        self.open = true;
        self.want_focus = true;
    }

    pub fn open_replace(&mut self) {
        self.open = true;
        self.replace_open = true;
        self.want_focus = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.replace_open = false;
        self.ranges.clear();
        self.error = None;
        self.last_sent = None;
    }

    /// À appeler chaque frame quand le panel est ouvert : relance la
    /// recherche si la requête, les options ou le buffer ont changé,
    /// et récolte les résultats arrivés.
    pub fn tick(&mut self, rope: &Rope, version: u64) {
        if !self.open {
            return;
        }
        let key = (
            self.query.clone(),
            self.case_sensitive,
            self.whole_word,
            self.use_regex,
            version,
        );
        if self.last_sent.as_ref() != Some(&key) {
            self.last_sent = Some(key);
            self.generation += 1;
            if self.query.is_empty() {
                self.ranges.clear();
                self.error = None;
            } else {
                self.spawn_search(rope.clone(), version);
            }
        }
        // Récolte (uniquement la génération courante).
        if let Some(rx) = &self.rx {
            while let Ok(res) = rx.try_recv() {
                if res.generation == self.generation {
                    self.ranges = res.ranges;
                    self.error = res.error;
                    if self.current >= self.ranges.len() {
                        self.current = 0;
                    }
                }
            }
        }
    }

    fn spawn_search(&mut self, rope: Rope, _version: u64) {
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
        let generation = self.generation;
        let pattern = build_pattern(
            &self.query,
            self.case_sensitive,
            self.whole_word,
            self.use_regex,
        );
        std::thread::Builder::new()
            .name("editor-search".into())
            .spawn(move || {
                let result = match pattern {
                    Err(e) => SearchResult {
                        generation,
                        ranges: Vec::new(),
                        error: Some(format!(
                            "Regex illisible : {e}. Même moi je ne peux pas chercher ça."
                        )),
                    },
                    Ok(re) => {
                        // O(N) ASSUMÉ : on est dans un thread, pas sur l'UI.
                        let hay = rope.to_string();
                        let ranges = re
                            .find_iter(&hay)
                            .map(|m| (rope.byte_to_char(m.start()), rope.byte_to_char(m.end())))
                            .collect();
                        SearchResult {
                            generation,
                            ranges,
                            error: None,
                        }
                    }
                };
                let _ = tx.send(result);
            })
            .ok();
    }

    /// Occurrence suivante/précédente. Retourne l'intervalle à montrer.
    pub fn step(&mut self, forward: bool) -> Option<(usize, usize)> {
        if self.ranges.is_empty() {
            return None;
        }
        let n = self.ranges.len();
        self.current = if forward {
            (self.current + 1) % n
        } else {
            (self.current + n - 1) % n
        };
        self.ranges.get(self.current).copied()
    }

    pub fn current_range(&self) -> Option<(usize, usize)> {
        self.ranges.get(self.current).copied()
    }

    /// Remplace l'occurrence courante (une transaction). Retourne la
    /// position curseur après remplacement.
    pub fn replace_current(&mut self, buf: &mut crate::buffer::Buffer) -> Option<usize> {
        let (s, e) = self.current_range()?;
        buf.begin_txn(crate::buffer::TxnKind::Other, s);
        buf.delete(s, e);
        buf.insert(s, &self.replace);
        let cursor = s + self.replace.chars().count();
        buf.end_txn(cursor);
        buf.commit_txn();
        // Le tick suivant relancera la recherche (version changée).
        Some(cursor)
    }

    /// Remplace TOUTES les occurrences (UNE transaction, annulable d'un coup).
    pub fn replace_all(&mut self, buf: &mut crate::buffer::Buffer) -> usize {
        if self.ranges.is_empty() {
            return 0;
        }
        let count = self.ranges.len();
        let first = self.ranges[0].0;
        buf.begin_txn(crate::buffer::TxnKind::Other, first);
        // De la fin vers le début : les indices amont restent valides.
        for (s, e) in self.ranges.clone().into_iter().rev() {
            buf.delete(s, e);
            buf.insert(s, &self.replace);
        }
        buf.end_txn(first + self.replace.chars().count());
        buf.commit_txn();
        count
    }
}

/// Construit la regex selon les options (texte littéral échappé sinon).
fn build_pattern(
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    use_regex: bool,
) -> Result<regex::Regex, regex::Error> {
    let mut pat = if use_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    if whole_word {
        pat = format!(r"\b(?:{pat})\b");
    }
    regex::RegexBuilder::new(&pat)
        .case_insensitive(!case_sensitive)
        .multi_line(true)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn search_blocking(state: &mut SearchState, rope: &Rope, version: u64) {
        state.tick(rope, version);
        let deadline = Instant::now() + Duration::from_secs(2);
        while state.ranges.is_empty() && state.error.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            state.tick(rope, version);
        }
    }

    #[test]
    fn finds_case_insensitive_by_default() {
        let rope = Rope::from_str("Neige neige NEIGE et enneigement");
        let mut s = SearchState {
            open: true,
            query: "neige".into(),
            ..Default::default()
        };
        search_blocking(&mut s, &rope, 1);
        assert_eq!(s.ranges.len(), 4); // y compris dans "enneigement"
    }

    #[test]
    fn whole_word_and_regex() {
        let rope = Rope::from_str("la neige neigeuse neige.");
        let mut s = SearchState {
            open: true,
            query: "neige".into(),
            whole_word: true,
            ..Default::default()
        };
        search_blocking(&mut s, &rope, 1);
        assert_eq!(s.ranges.len(), 2);

        let mut s = SearchState {
            open: true,
            query: "nei\\w+".into(),
            use_regex: true,
            ..Default::default()
        };
        search_blocking(&mut s, &rope, 1);
        assert_eq!(s.ranges.len(), 3);
    }

    #[test]
    fn invalid_regex_reports_glados() {
        let rope = Rope::from_str("texte");
        let mut s = SearchState {
            open: true,
            query: "[".into(),
            use_regex: true,
            ..Default::default()
        };
        s.tick(&rope, 1);
        let deadline = Instant::now() + Duration::from_secs(2);
        while s.error.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            s.tick(&rope, 1);
        }
        assert!(s.error.is_some());
    }

    #[test]
    fn replace_all_is_one_undo() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("t.typ");
        std::fs::write(&path, "un chat, deux chat, trois chat")?;
        let mut buf = crate::buffer::open_standalone(&path)?;
        let mut s = SearchState {
            open: true,
            query: "chat".into(),
            replace: "chien".into(),
            ..Default::default()
        };
        search_blocking(&mut s, &buf.rope.clone(), buf.version);
        assert_eq!(s.ranges.len(), 3);
        let n = s.replace_all(&mut buf);
        assert_eq!(n, 3);
        assert_eq!(buf.rope.to_string(), "un chien, deux chien, trois chien");
        buf.undo();
        assert_eq!(buf.rope.to_string(), "un chat, deux chat, trois chat");
        Ok(())
    }

    #[test]
    fn unicode_offsets_are_chars_not_bytes() {
        let rope = Rope::from_str("été — l’aurore était là");
        let mut s = SearchState {
            open: true,
            query: "aurore".into(),
            ..Default::default()
        };
        search_blocking(&mut s, &rope, 1);
        assert_eq!(s.ranges, vec![(8, 14)]);
    }
}
