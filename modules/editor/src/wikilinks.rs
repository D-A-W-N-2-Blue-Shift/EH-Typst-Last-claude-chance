// ============================================================================
// modules/editor/src/wikilinks.rs — Détection et auto-complétion [[liens]]
//
// - find_in_line : détecte les [[liens]] d'une ligne (corps ET frontmatter,
//   y compris dans les valeurs YAML : Père: "[[Konstantin Volkova]]").
// - WikilinkIndex : la liste des .typ du projet, FOURNIE par le core
//   (CoreEvent::FileIndexUpdated, publiée par file_tree). L'éditeur ne
//   scanne JAMAIS le filesystem lui-même.
// - AutocompleteState : popup sous le curseur dès [[ + N caractères,
//   fuzzy insensible à la casse, flèches + Entrée, Echap ferme.
// ============================================================================

use std::path::PathBuf;
use std::sync::Arc;

/// Les [[liens]] d'une ligne : (octet début, octet fin — crochets inclus,
/// nom entre les crochets). Les liens imbriqués/malformés sont ignorés.
pub fn find_in_line(line: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() + 1 {
        if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // Chercher ]] en s'arrêtant si un nouveau [[ commence.
            let mut j = i + 2;
            let mut end = None;
            while j + 1 < bytes.len() {
                if bytes[j] == b']' && bytes[j + 1] == b']' {
                    end = Some(j + 2);
                    break;
                }
                if bytes[j] == b'[' && bytes[j + 1] == b'[' {
                    break;
                }
                j += 1;
            }
            if let Some(end) = end {
                let name = line[i + 2..end - 2].trim();
                if !name.is_empty() {
                    out.push((i, end, name.to_string()));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// L'index des fichiers du projet, reçu du core. Les noms sont les
/// `file_stem` (sans extension), la résolution est insensible à la casse.
#[derive(Default, Clone)]
pub struct WikilinkIndex {
    pub project_root: PathBuf,
    pub files: Arc<Vec<PathBuf>>,
}

impl WikilinkIndex {
    /// Le lien résout-il vers un fichier connu ?
    pub fn resolve(&self, name: &str) -> Option<PathBuf> {
        let target = name.trim().to_lowercase();
        self.files
            .iter()
            .find(|f| {
                f.file_stem()
                    .map(|s| s.to_string_lossy().to_lowercase() == target)
                    .unwrap_or(false)
            })
            .cloned()
    }

    /// Chemin de création pour un lien orphelin : <projet>/orphelins/<nom>.typ.
    pub fn orphan_path(&self, name: &str) -> PathBuf {
        self.project_root
            .join("orphelins")
            .join(format!("{}.typ", name.trim()))
    }

    /// Candidats fuzzy pour l'auto-complétion : sous-séquence insensible à
    /// la casse, les meilleurs scores d'abord. O(nombre de fichiers) — appelé
    /// à la frappe dans un popup, pas par frame de rendu.
    pub fn complete(&self, query: &str, max: usize) -> Vec<String> {
        let q = query.to_lowercase();
        let mut scored: Vec<(i64, String)> = self
            .files
            .iter()
            .filter_map(|f| {
                let stem = f.file_stem()?.to_string_lossy().to_string();
                let score = fuzzy_score(&stem.to_lowercase(), &q)?;
                Some((score, stem))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        scored.dedup_by(|a, b| a.1 == b.1);
        scored.into_iter().take(max).map(|(_, s)| s).collect()
    }
}

/// Score fuzzy minimal : None si `query` n'est pas une sous-séquence de
/// `candidate`. Bonus pour préfixe et caractères contigus.
fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let mut score = 0i64;
    let mut last_match: Option<usize> = None;
    let mut chars = candidate.char_indices();
    for qc in query.chars() {
        loop {
            let (i, cc) = chars.next()?;
            if cc == qc {
                score += match last_match {
                    Some(prev) if i == prev + cc.len_utf8() => 3, // contigu
                    None if i == 0 => 5,                          // préfixe
                    _ => 1,
                };
                last_match = Some(i);
                break;
            }
        }
    }
    // Les candidats courts gagnent à score égal.
    Some(score * 100 - candidate.len() as i64)
}

/// État du popup d'auto-complétion d'UNE fenêtre éditeur.
#[derive(Default)]
pub struct AutocompleteState {
    pub open: bool,
    /// Indice caractère (buffer) du début de la requête (juste après [[).
    pub query_start: usize,
    pub candidates: Vec<String>,
    pub selected: usize,
}

impl AutocompleteState {
    pub fn close(&mut self) {
        self.open = false;
        self.candidates.clear();
        self.selected = 0;
    }
}

/// Si le curseur est juste après `[[…` (sans fermeture ni saut de ligne),
/// retourne (début de requête en chars, requête).
pub fn query_at_cursor(rope: &ropey::Rope, cursor: usize) -> Option<(usize, String)> {
    let line_idx = rope.char_to_line(cursor.min(rope.len_chars()));
    let line_start = rope.line_to_char(line_idx);
    let before: String = rope.slice(line_start..cursor).to_string();
    let open = before.rfind("[[")?;
    let q = &before[open + 2..];
    if q.contains("]]") || q.contains('[') {
        return None;
    }
    // Position en CARACTÈRES : before[..open+2] compte en octets → reconvertir.
    let prefix_chars = before[..open + 2].chars().count();
    Some((line_start + prefix_chars, q.to_string()))
}

/// L'éditeur a-t-il un projet pour ranger les orphelins ? Sinon on refuse
/// la création (pas de projet = pas de dossier orphelins/).
pub fn can_create_orphan(index: &WikilinkIndex) -> bool {
    !index.project_root.as_os_str().is_empty() && index.project_root.exists()
}

/// Trouve le wikilink contenant éventuellement `byte_pos` dans `line`.
pub fn link_at(line: &str, byte_pos: usize) -> Option<(usize, usize, String)> {
    find_in_line(line)
        .into_iter()
        .find(|(s, e, _)| *s <= byte_pos && byte_pos < *e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_links_in_body_and_yaml() {
        let l = "Elle pense à [[Konstantin Volkova]] souvent.";
        let links = find_in_line(l);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].2, "Konstantin Volkova");

        // Wikilink dans une valeur YAML du frontmatter.
        let y = r#"Père: "[[Konstantin Dmitrievitch Volkova]]""#;
        let links = find_in_line(y);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].2, "Konstantin Dmitrievitch Volkova");
    }

    #[test]
    fn ignores_malformed() {
        assert!(find_in_line("[[sans fermeture").is_empty());
        assert!(find_in_line("[[]]").is_empty());
        assert_eq!(find_in_line("[[a]] et [[b]]").len(), 2);
    }

    #[test]
    fn fuzzy_matches_case_insensitive() {
        let idx = WikilinkIndex {
            project_root: PathBuf::from("/tmp/projet"),
            files: Arc::new(vec![
                PathBuf::from("/tmp/projet/02/svetlana_volkova.typ"),
                PathBuf::from("/tmp/projet/02/konstantin.typ"),
                PathBuf::from("/tmp/projet/05/scene_1.typ"),
            ]),
        };
        let c = idx.complete("Svet", 8);
        assert_eq!(c, vec!["svetlana_volkova".to_string()]);
        assert!(idx.resolve("SVETLANA_VOLKOVA").is_some());
        assert!(idx.resolve("inconnue").is_none());
        assert_eq!(
            idx.orphan_path("Nouveau Perso"),
            PathBuf::from("/tmp/projet/orphelins/Nouveau Perso.typ")
        );
    }

    #[test]
    fn query_detection() -> Result<(), Box<dyn std::error::Error>> {
        let rope = ropey::Rope::from_str("voir [[Svet");
        let (start, q) = query_at_cursor(&rope, 11).ok_or("requête wikilink attendue")?;
        assert_eq!(q, "Svet");
        assert_eq!(start, 7);
        // Lien déjà fermé : pas de popup.
        let rope = ropey::Rope::from_str("voir [[Svet]] x");
        assert!(query_at_cursor(&rope, 15).is_none());
        Ok(())
    }
}
