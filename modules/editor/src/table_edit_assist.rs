// ============================================================================
// modules/editor/src/table_edit_assist.rs — Assistance frappe tableaux (§6)
//
// Décision actée du brief : pas de rendu graphique des tableaux (risque
// architectural trop élevé en headless). Fallback : assistance de frappe
// qui rend le Typst brut supportable à éditer dans le format `#table(...)` :
//
//   #table(
//     columns: 3,
//     [Date], [Personnage], [Statut],
//     [1942-03-15], [Svetlana], [Vivant],
//   )
//
// Comportements (curseur dans un bloc tableau détecté) :
//   - Tab        : cellule suivante. Si dernière cellule de la dernière
//                  ligne, crée une nouvelle ligne (curseur en première
//                  cellule). Si dernière cellule mais pas dernière ligne :
//                  passe à la première cellule de la ligne suivante (style
//                  Excel/Google Docs).
//   - Shift+Tab  : cellule précédente.
//   - Entrée     : uniquement traité si dernière cellule de dernière ligne
//                  → insère une nouvelle ligne avec le bon nombre de cellules,
//                  curseur dans la première cellule. Sinon on laisse
//                  l'Entrée par défaut faire son boulot (retour à la ligne).
//
// Volontairement non fait en v1 : création de COLONNE quand Tab est pressé
// au-delà de la dernière cellule (le brief le proposait mais c'est une
// transformation destructive qui touche toutes les lignes — préférable
// d'attendre validation utilisateur avant de l'activer). L'alignement
// visuel automatique au rendu est aussi laissé pour une passe ultérieure.
// ============================================================================

use ropey::Rope;

/// Description d'un bloc tableau repéré dans le rope (indépendant du curseur).
#[derive(Debug, Clone)]
pub struct TableBlock {
    pub first_line: usize,
    pub last_line: usize,
    pub cells_per_row: usize,
    pub separator_line: Option<usize>,
}

/// Description d'un bloc tableau autour du curseur.
#[derive(Debug, Clone)]
pub struct TableContext {
    /// Index char de la première ligne du tableau dans le rope.
    pub block_start: usize,
    /// Index char juste après la fin de la dernière ligne du tableau.
    pub block_end: usize,
    /// Numéro de la ligne du curseur dans le rope (0-indexed).
    pub cursor_line: usize,
    /// Première ligne du tableau (numéro de ligne dans le rope).
    pub first_line: usize,
    /// Dernière ligne du tableau (incluse).
    pub last_line: usize,
    /// Nombre de cellules par ligne (déduit de la première ligne).
    pub cells_per_row: usize,
    /// Index char absolus des positions de pipe `|` sur la ligne du curseur,
    /// ordonnés. Sert à calculer la cellule courante et les sauts.
    pub pipe_positions: Vec<usize>,
}

impl TableContext {
    /// Index de la cellule du curseur (0 = première cellule entre le 1er et
    /// le 2e pipe). Retourne `None` si le curseur est hors d'une cellule.
    pub fn cell_index(&self, cursor: usize) -> Option<usize> {
        if self.pipe_positions.len() < 2 {
            return None;
        }
        for (i, w) in self.pipe_positions.windows(2).enumerate() {
            if cursor > w[0] && cursor <= w[1] {
                return Some(i);
            }
        }
        None
    }
}

/// Une édition à appliquer côté caller (l'appelant connaît les transactions
/// du buffer et les met à jour).
#[derive(Debug, Clone)]
pub enum TableEdit {
    /// Repositionne le curseur (pas d'insertion / suppression).
    MoveCursor(usize),
    /// Insère du texte à la position donnée puis repositionne le curseur.
    Insert {
        at: usize,
        text: String,
        new_cursor: usize,
    },
}

/// Détecte un bloc tableau autour du curseur. Une "ligne de tableau" est
/// une ligne dont le **premier caractère non blanc** est `|` ET qui
/// contient au moins 2 pipes. Le bloc s'étend en haut et en bas tant que
/// les lignes satisfont cette règle. Renvoie `None` si le curseur n'est
/// pas dans un tel bloc.
pub fn detect_table_at(rope: &Rope, cursor: usize) -> Option<TableContext> {
    let cursor_line = rope.char_to_line(cursor.min(rope.len_chars()));
    if !is_table_row(rope, cursor_line) {
        return None;
    }

    let mut first_line = cursor_line;
    while first_line > 0 && is_table_row(rope, first_line - 1) {
        first_line -= 1;
    }
    let total_lines = rope.len_lines();
    let mut last_line = cursor_line;
    while last_line + 1 < total_lines && is_table_row(rope, last_line + 1) {
        last_line += 1;
    }

    let block_start = rope.line_to_char(first_line);
    let block_end_line = if last_line + 1 < total_lines {
        rope.line_to_char(last_line + 1)
    } else {
        rope.len_chars()
    };

    // Cellules par ligne : on prend la première ligne (on suppose un tableau
    // bien formé ; sinon le caller voit juste un comportement dégradé).
    let cells_per_row = count_cells_on_line(rope, first_line);
    if cells_per_row == 0 {
        return None;
    }

    let pipe_positions = pipe_positions_on_line(rope, cursor_line);
    Some(TableContext {
        block_start,
        block_end: block_end_line,
        cursor_line,
        first_line,
        last_line,
        cells_per_row,
        pipe_positions,
    })
}

/// Calcule l'action Tab : cellule suivante, ou nouvelle ligne en bas du
/// tableau si on était dans la dernière cellule de la dernière ligne.
pub fn handle_tab(rope: &Rope, cursor: usize) -> Option<TableEdit> {
    let ctx = detect_table_at(rope, cursor)?;
    let cell = ctx.cell_index(cursor)?;
    let is_last_cell_of_row = cell + 1 >= ctx.pipe_positions.len() - 1;

    if !is_last_cell_of_row {
        // Saut à la cellule suivante : juste après le pipe + 1 char.
        let next_pipe = ctx.pipe_positions[cell + 1];
        return Some(TableEdit::MoveCursor(jump_into_cell_after_pipe(
            rope, next_pipe,
        )));
    }

    // Dernière cellule de la ligne. Deux cas :
    //  - pas la dernière ligne du tableau : on va à la 1re cellule de la ligne suivante
    //  - dernière ligne : on insère une nouvelle ligne avec le bon nombre de pipes
    if ctx.cursor_line < ctx.last_line {
        let next_pipes = pipe_positions_on_line(rope, ctx.cursor_line + 1);
        if next_pipes.len() >= 2 {
            return Some(TableEdit::MoveCursor(jump_into_cell_after_pipe(
                rope,
                next_pipes[0],
            )));
        }
    }
    // Nouvelle ligne en bas du tableau.
    let new_row = format!("\n{}", empty_row_for(ctx.cells_per_row));
    let line_end = if ctx.cursor_line + 1 < rope.len_lines() {
        rope.line_to_char(ctx.cursor_line + 1).saturating_sub(1)
    } else {
        rope.len_chars()
    };
    // Curseur dans la 1re cellule de la nouvelle ligne (= juste après le 1er `[` + espace).
    let new_cursor = line_end + 1 /* '\n' */ + 2 /* "[ " */;
    Some(TableEdit::Insert {
        at: line_end,
        text: new_row,
        new_cursor,
    })
}

/// Shift+Tab : cellule précédente.
pub fn handle_shift_tab(rope: &Rope, cursor: usize) -> Option<TableEdit> {
    let ctx = detect_table_at(rope, cursor)?;
    let cell = ctx.cell_index(cursor)?;
    if cell == 0 {
        // Première cellule : si pas la première ligne, on remonte sur la dernière cellule.
        if ctx.cursor_line > ctx.first_line {
            let prev_pipes = pipe_positions_on_line(rope, ctx.cursor_line - 1);
            if prev_pipes.len() >= 2 {
                let last_cell_start = prev_pipes[prev_pipes.len() - 2];
                return Some(TableEdit::MoveCursor(jump_into_cell_after_pipe(
                    rope,
                    last_cell_start,
                )));
            }
        }
        return None;
    }
    let prev_pipe = ctx.pipe_positions[cell - 1];
    Some(TableEdit::MoveCursor(jump_into_cell_after_pipe(
        rope, prev_pipe,
    )))
}

/// Entrée dans la dernière cellule de la dernière ligne d'un tableau : crée
/// une nouvelle ligne avec le bon nombre de pipes. Sinon retourne None (le
/// caller laisse l'Entrée par défaut faire son boulot).
pub fn handle_enter(rope: &Rope, cursor: usize) -> Option<TableEdit> {
    let ctx = detect_table_at(rope, cursor)?;
    let cell = ctx.cell_index(cursor)?;
    let is_last_cell_of_row = cell + 1 >= ctx.pipe_positions.len() - 1;
    if !is_last_cell_of_row || ctx.cursor_line != ctx.last_line {
        return None;
    }
    let line_end = if ctx.cursor_line + 1 < rope.len_lines() {
        rope.line_to_char(ctx.cursor_line + 1).saturating_sub(1)
    } else {
        rope.len_chars()
    };
    let new_row = format!("\n{}", empty_row_for(ctx.cells_per_row));
    let new_cursor = line_end + 1 + 2; // \n + "| "
    Some(TableEdit::Insert {
        at: line_end,
        text: new_row,
        new_cursor,
    })
}

// ---------- helpers ----------

fn is_table_row(rope: &Rope, line: usize) -> bool {
    if line >= rope.len_lines() {
        return false;
    }
    let s = rope.line(line);
    let mut found_cell = false;
    let mut cell_count = 0usize;
    for ch in s.chars() {
        if ch == '\n' {
            break;
        }
        if !found_cell {
            if ch.is_whitespace() {
                continue;
            }
            if ch != '[' {
                return false;
            }
            found_cell = true;
            cell_count += 1;
        } else if ch == '[' {
            cell_count += 1;
        }
    }
    found_cell && cell_count >= 2
}

fn count_cells_on_line(rope: &Rope, line: usize) -> usize {
    let cells = pipe_positions_on_line(rope, line);
    if cells.len() < 2 {
        0
    } else {
        cells.len() - 1
    }
}

fn pipe_positions_on_line(rope: &Rope, line: usize) -> Vec<usize> {
    if line >= rope.len_lines() {
        return Vec::new();
    }
    let line_start = rope.line_to_char(line);
    let line_end = if line + 1 < rope.len_lines() {
        rope.line_to_char(line + 1).saturating_sub(1)
    } else {
        rope.len_chars()
    };
    let mut out = Vec::new();
    for (i, ch) in rope.line(line).chars().enumerate() {
        if ch == '\n' {
            break;
        }
        if ch == '[' {
            out.push(line_start + i);
        }
    }
    out.push(line_end);
    out
}

/// Curseur après un `[` : on saute le crochet + 1 char d'espace si présent
/// (convention de tableau bien formé). Sinon juste après le crochet.
fn jump_into_cell_after_pipe(rope: &Rope, pipe_pos: usize) -> usize {
    let after = pipe_pos + 1;
    if after < rope.len_chars() && rope.char(after) == ' ' {
        after + 1
    } else {
        after
    }
}

fn empty_row_for(cells: usize) -> String {
    let mut s = String::with_capacity(cells * 5 + 1);
    for i in 0..cells {
        s.push_str("[ ]");
        if i + 1 < cells {
            s.push_str(", ");
        }
    }
    s
}

/// Détecte tous les blocs tableau dans le rope.
pub fn detect_all_tables(rope: &Rope) -> Vec<TableBlock> {
    let total = rope.len_lines();
    let mut blocks = Vec::new();
    let mut line = 0;
    while line < total {
        if is_table_row(rope, line) {
            let first = line;
            let mut last = line;
            while last + 1 < total && is_table_row(rope, last + 1) {
                last += 1;
            }
            let cells = count_cells_on_line(rope, first);
            let sep = (first..=last).find(|&l| is_separator_row(rope, l));
            if cells > 0 {
                blocks.push(TableBlock {
                    first_line: first,
                    last_line: last,
                    cells_per_row: cells,
                    separator_line: sep,
                });
            }
            line = last + 1;
        } else {
            line += 1;
        }
    }
    blocks
}

/// Vérifie si une ligne marque la fin du bloc Typst.
pub fn is_separator_row(rope: &Rope, line: usize) -> bool {
    if line >= rope.len_lines() {
        return false;
    }
    let s = rope.line(line);
    let text: String = s.chars().take_while(|c| *c != '\n').collect();
    let trimmed = text.trim();
    trimmed == ")"
}

/// Extrait le contenu des cellules d'un bloc tableau en 2D.
pub fn parse_table_cells(rope: &Rope, block: &TableBlock) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in block.first_line..=block.last_line {
        let s = rope.line(line);
        let text: String = s.chars().take_while(|c| *c != '\n').collect();
        let trimmed = text.trim();
        if !trimmed.starts_with('[') {
            continue;
        }
        let cells: Vec<String> = trimmed
            .split("],")
            .map(|c| c.trim().trim_matches(&['[', ']'][..]).trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        rows.push(cells);
    }
    rows
}

/// Reconstruit un tableau Typst à partir d'une grille de cellules 2D.
pub fn serialize_table(cells: &[Vec<String>], cols: usize) -> String {
    if cells.is_empty() || cols == 0 {
        return String::new();
    }
    let mut col_widths = vec![1usize; cols];
    for row in cells {
        for (i, cell) in row.iter().enumerate() {
            if i < cols {
                col_widths[i] = col_widths[i].max(cell.len());
            }
        }
    }

    let mut out = String::new();
    out.push_str("#table(\n");
    out.push_str(&format!("  columns: {},\n", cols));
    for row in cells {
        for col in 0..cols {
            let cell = row.get(col).map(|s| s.as_str()).unwrap_or("");
            out.push_str(&format!("  [{}]", cell.trim()));
            if col + 1 < cols {
                out.push(',');
            }
            if col + 1 < cols {
                out.push(' ');
            }
        }
        out.push(',');
        out.push('\n');
    }
    out.push_str(")\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    fn rope(s: &str) -> Rope {
        Rope::from_str(s)
    }

    #[test]
    fn detects_simple_table() -> Result<(), Box<dyn std::error::Error>> {
        let r = rope("  [a], [b],\n  [1], [2],\n");
        let ctx = detect_table_at(&r, 3).ok_or("table attendue dans la 1re cellule")?;
        assert_eq!(ctx.first_line, 0);
        assert_eq!(ctx.last_line, 1);
        assert_eq!(ctx.cells_per_row, 2);
        Ok(())
    }

    #[test]
    fn no_detection_outside_table() {
        let r = rope("Paragraphe.\n\n  [a], [b],\n");
        assert!(detect_table_at(&r, 0).is_none());
    }

    #[test]
    fn tab_jumps_to_next_cell() -> Result<(), Box<dyn std::error::Error>> {
        let r = rope("  [a], [b],\n");
        // curseur après "a", avant la prochaine cellule.
        let edit = handle_tab(&r, 3).ok_or("édition attendue dans la 1re cellule")?;
        match edit {
            TableEdit::MoveCursor(c) => {
                // Saut après `[ ` du 2e champ → index du 'b'
                assert_eq!(r.char(c), 'b');
            }
            other => return Err(format!("attendu MoveCursor, eu {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn tab_in_last_cell_of_last_row_creates_new_row() -> Result<(), Box<dyn std::error::Error>> {
        let r = rope("  [a], [b],\n  [1], [2],");
        // dernière cellule de la 2e (et dernière) ligne, curseur sur '2'
        let cursor = r.line_to_char(1) + 8;
        let edit = handle_tab(&r, cursor).ok_or("édition attendue dans la dernière cellule")?;
        match edit {
            TableEdit::Insert {
                at,
                text,
                new_cursor,
            } => {
                assert!(text.starts_with("\n[ ]"));
                assert!(at >= cursor, "insertion en fin de ligne");
                // new_cursor doit pointer juste après "[ " de la nouvelle ligne
                let mut tmp = r.clone();
                tmp.insert(at, &text);
                assert_eq!(tmp.char(new_cursor - 1), ' ');
            }
            other => return Err(format!("attendu Insert, eu {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn shift_tab_jumps_to_previous_cell() -> Result<(), Box<dyn std::error::Error>> {
        let r = rope("  [a], [b],\n");
        // curseur sur 'b'
        let edit = handle_shift_tab(&r, 8).ok_or("édition attendue dans la 2e cellule")?;
        match edit {
            TableEdit::MoveCursor(c) => assert_eq!(r.char(c), 'a'),
            other => return Err(format!("attendu MoveCursor, eu {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn enter_outside_last_cell_returns_none() {
        let r = rope("  [a], [b],\n  [1], [2],");
        // curseur sur 'a' : pas la dernière cellule de la dernière ligne
        assert!(handle_enter(&r, 3).is_none());
    }

    #[test]
    fn enter_in_last_cell_of_last_row_creates_new_row() -> Result<(), Box<dyn std::error::Error>> {
        let r = rope("  [a], [b],\n  [1], [2],");
        let cursor = r.line_to_char(1) + 8; // sur '2'
        let edit = handle_enter(&r, cursor).ok_or("doit créer une nouvelle ligne")?;
        assert!(matches!(edit, TableEdit::Insert { .. }));
        Ok(())
    }

    #[test]
    fn detect_all_tables_finds_blocks() {
        let r = rope("Hello\n\n  [a], [b],\n  [1], [2],\n\nBye\n");
        let tables = detect_all_tables(&r);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].first_line, 2);
        assert_eq!(tables[0].last_line, 3);
        assert_eq!(tables[0].cells_per_row, 2);
        assert_eq!(tables[0].separator_line, None);
    }

    #[test]
    fn is_separator_row_works() {
        let r = rope("#table(\n)\n");
        assert!(!is_separator_row(&r, 0));
        assert!(is_separator_row(&r, 1));
    }

    #[test]
    fn parse_and_serialize_roundtrip() {
        let r = rope("  [a], [b],\n  [1], [2],\n");
        let tables = detect_all_tables(&r);
        let cells = parse_table_cells(&r, &tables[0]);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0], vec!["a", "b"]);
        assert_eq!(cells[1], vec!["1", "2"]);
        let ser = serialize_table(&cells, 2);
        assert!(ser.contains("#table("));
        assert!(ser.contains("[a]"));
        assert!(ser.contains("[1]"));
    }
}
