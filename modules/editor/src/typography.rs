// ============================================================================
// modules/editor/src/typography.rs — Smart typography
//
// Conversions à la frappe : guillemets français/anglais, apostrophes
// courbes, tiret cadratin, points de suspension, espaces insécables avant
// la ponctuation double (français).
//
// CONTRAT UNDO : l'appelant (input.rs) a déjà ouvert la transaction de la
// frappe. Les mutations faites ici la rejoignent → Ctrl+Z annule la frappe
// ET la conversion en une seule fois. C'est la raison d'être du batching
// de buffer.rs — pas un patch après coup.
// ============================================================================

use crate::buffer::Buffer;
use crate::config::EditorVomi;

/// Réglages effectifs (config globale, surchargée par le frontmatter du
/// fichier : `smart_typography:` et `language:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("en") {
            Lang::En
        } else {
            Lang::Fr
        }
    }
}

/// Espace insécable (avant : ; ? ! en français).
const NBSP: char = '\u{00A0}';

/// Appelé APRÈS l'insertion du caractère `c` (le curseur est juste derrière).
/// Retourne la nouvelle position du curseur si une conversion a eu lieu.
pub fn after_insert(
    buf: &mut Buffer,
    cursor: usize,
    c: char,
    lang: Lang,
    vomi: &EditorVomi,
) -> Option<usize> {
    match c {
        '-' if vomi.auto_emdash => emdash(buf, cursor),
        '.' if vomi.auto_ellipsis => ellipsis(buf, cursor),
        '"' => quotes(buf, cursor, lang, vomi),
        '\'' => apostrophe(buf, cursor),
        ':' | ';' | '?' | '!' if lang == Lang::Fr && vomi.auto_nbsp => nbsp_before(buf, cursor),
        _ => None,
    }
}

fn char_at(buf: &Buffer, i: usize) -> Option<char> {
    if i < buf.rope.len_chars() {
        Some(buf.rope.char(i))
    } else {
        None
    }
}

/// `--` → `—`, sauf en tête de ligne (ne pas casser `---` du frontmatter
/// et des règles horizontales).
fn emdash(buf: &mut Buffer, cursor: usize) -> Option<usize> {
    if cursor < 2 || char_at(buf, cursor - 2)? != '-' {
        return None;
    }
    let line = buf.rope.char_to_line(cursor - 2);
    let line_start = buf.rope.line_to_char(line);
    let before: String = buf.rope.slice(line_start..cursor - 2).to_string();
    if before.trim().is_empty() || before.trim_end().ends_with('-') {
        return None; // début de ligne ou suite de tirets : on n'y touche pas.
    }
    buf.delete(cursor - 2, cursor);
    buf.insert(cursor - 2, "—");
    Some(cursor - 1)
}

/// `...` → `…` (pas `....` : on ne convertit que le triplet exact).
fn ellipsis(buf: &mut Buffer, cursor: usize) -> Option<usize> {
    if cursor < 3
        || char_at(buf, cursor - 2)? != '.'
        || char_at(buf, cursor - 3)? != '.'
        || (cursor >= 4 && char_at(buf, cursor - 4) == Some('.'))
    {
        return None;
    }
    buf.delete(cursor - 3, cursor);
    buf.insert(cursor - 3, "…");
    Some(cursor - 2)
}

/// `"` → « ou » (fr), “ ou ” (en). Ouvrant si précédé de rien, d'un blanc
/// ou d'une parenthèse ouvrante ; fermant sinon.
fn quotes(buf: &mut Buffer, cursor: usize, lang: Lang, vomi: &EditorVomi) -> Option<usize> {
    if lang == Lang::Fr && !vomi.french_quotes {
        return None;
    }
    let prev = if cursor >= 2 {
        char_at(buf, cursor - 2)
    } else {
        None
    };
    let opening = match prev {
        None => true,
        Some(p) => p.is_whitespace() || matches!(p, '(' | '[' | '{' | '«' | '“' | '\n'),
    };
    let replacement = match (lang, opening) {
        (Lang::Fr, true) => "«",
        (Lang::Fr, false) => "»",
        (Lang::En, true) => "“",
        (Lang::En, false) => "”",
    };
    buf.delete(cursor - 1, cursor);
    buf.insert(cursor - 1, replacement);
    Some(cursor)
}

/// `'` → `’` (apostrophe courbe, toutes langues).
fn apostrophe(buf: &mut Buffer, cursor: usize) -> Option<usize> {
    buf.delete(cursor - 1, cursor);
    buf.insert(cursor - 1, "’");
    Some(cursor)
}

/// Espace simple avant `: ; ? !` → espace insécable (français).
fn nbsp_before(buf: &mut Buffer, cursor: usize) -> Option<usize> {
    if cursor < 2 || char_at(buf, cursor - 2)? != ' ' {
        return None;
    }
    buf.delete(cursor - 2, cursor - 1);
    buf.insert(cursor - 2, &NBSP.to_string());
    Some(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::TxnKind;

    fn buf(text: &str) -> Result<Buffer, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("t.typ");
        std::fs::write(&path, text)?;
        Ok(crate::buffer::open_standalone(&path)?)
    }

    /// Simule la frappe de `c` à la fin du texte + smart typography,
    /// comme input.rs le fait : une seule transaction.
    fn type_char(b: &mut Buffer, c: char) -> usize {
        let cur = b.rope.len_chars();
        b.begin_txn(TxnKind::Typing, cur);
        b.insert(cur, &c.to_string());
        let mut cursor = cur + 1;
        if let Some(nc) = after_insert(b, cursor, c, Lang::Fr, &EditorVomi::default()) {
            cursor = nc;
        }
        b.end_txn(cursor);
        cursor
    }

    #[test]
    fn emdash_mid_sentence_not_line_start() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = buf("un mot -")?;
        type_char(&mut b, '-');
        assert_eq!(b.rope.to_string(), "un mot —");
        // Début de ligne : --- doit rester --- (frontmatter, hr).
        let mut b = buf("-")?;
        type_char(&mut b, '-');
        assert_eq!(b.rope.to_string(), "--");
        type_char(&mut b, '-');
        assert_eq!(b.rope.to_string(), "---");
        Ok(())
    }

    #[test]
    fn ellipsis_and_undo_in_one() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = buf("attends..")?;
        type_char(&mut b, '.');
        assert_eq!(b.rope.to_string(), "attends…");
        // Ctrl+Z : la frappe ET la conversion partent ensemble.
        b.undo();
        assert_eq!(b.rope.to_string(), "attends..");
        Ok(())
    }

    #[test]
    fn french_quotes_open_close() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = buf("")?;
        type_char(&mut b, '"');
        assert_eq!(b.rope.to_string(), "«");
        let mut b = buf("«bonjour")?;
        type_char(&mut b, '"');
        assert_eq!(b.rope.to_string(), "«bonjour»");
        Ok(())
    }

    #[test]
    fn english_quotes() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = buf("")?;
        let cur = b.rope.len_chars();
        b.begin_txn(TxnKind::Typing, cur);
        b.insert(cur, "\"");
        after_insert(&mut b, cur + 1, '"', Lang::En, &EditorVomi::default());
        b.end_txn(cur + 1);
        assert_eq!(b.rope.to_string(), "“");
        Ok(())
    }

    #[test]
    fn curly_apostrophe() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = buf("l")?;
        type_char(&mut b, '\'');
        assert_eq!(b.rope.to_string(), "l’");
        Ok(())
    }

    #[test]
    fn nbsp_before_double_punctuation() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = buf("Quoi ")?;
        type_char(&mut b, '?');
        assert_eq!(b.rope.to_string(), "Quoi\u{00A0}?");
        // Pas d'espace tapé : on n'invente pas d'insécable.
        let mut b = buf("Quoi")?;
        type_char(&mut b, '!');
        assert_eq!(b.rope.to_string(), "Quoi!");
        Ok(())
    }
}
