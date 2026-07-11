// ============================================================================
// modules/editor/src/table_dialog.rs — Dialogue « Insérer un tableau »
//
// Popup egui (pas une fenêtre OS) ouvert par le menu contextuel « Insérer
// tableau » ou le snippet ;tab. L'utilisateur choisit le nombre de colonnes
// et de lignes (1..=10) et les en-têtes ; le dialogue génère le Typst et le
// fait insérer à la position du curseur, qui atterrit dans la première cellule.
//
// Pas de rendu graphique du tableau : ce dialogue ne fait que produire du
// Typst propre.
// ============================================================================

/// État du dialogue, porté par EditorWindow (`Option<TableDialogState>`).
pub struct TableDialogState {
    pub cols: usize,
    pub rows: usize,
    pub headers: Vec<String>,
}

impl Default for TableDialogState {
    fn default() -> Self {
        Self {
            cols: 3,
            rows: 3,
            headers: (1..=3).map(|n| format!("Col {n}")).collect(),
        }
    }
}

impl TableDialogState {
    /// Garde `headers` aligné sur `cols` : complète avec « Col N », tronque le
    /// surplus. Appelé chaque frame après l'édition des spinners.
    fn sync_headers(&mut self) {
        self.cols = self.cols.clamp(1, 10);
        self.rows = self.rows.clamp(1, 10);
        while self.headers.len() < self.cols {
            let n = self.headers.len() + 1;
            self.headers.push(format!("Col {n}"));
        }
        self.headers.truncate(self.cols);
    }
}

/// Issue d'une frame du dialogue.
pub enum TableOutcome {
    /// Insérer : (Typst, décalage curseur en chars dans le bloc inséré).
    Insert { typst: String, cursor_offset: usize },
    /// Annuler : fermer sans rien insérer.
    Cancel,
}

/// Dessine le dialogue. Retourne Some(..) quand l'utilisateur valide/annule
/// (Entrée insère, Échap annule), None tant qu'il édite.
pub fn show(ctx: &egui::Context, state: &mut TableDialogState) -> Option<TableOutcome> {
    state.sync_headers();
    let mut outcome = None;

    egui::Window::new("Insérer un tableau")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            spinner_row(ui, "Colonnes :", &mut state.cols);
            spinner_row(ui, "Lignes   :", &mut state.rows);
            state.sync_headers();

            ui.separator();
            ui.label("En-têtes :");
            ui.horizontal_wrapped(|ui| {
                for h in state.headers.iter_mut() {
                    ui.add(egui::TextEdit::singleline(h).desired_width(120.0));
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Insérer").clicked() {
                    outcome = Some(insert_outcome(state));
                }
                if ui.button("Annuler").clicked() {
                    outcome = Some(TableOutcome::Cancel);
                }
            });
        });

    // Clavier : Entrée valide, Échap annule (priorité à l'annulation).
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        return Some(TableOutcome::Cancel);
    }
    if outcome.is_none()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
    {
        return Some(insert_outcome(state));
    }
    outcome
}

fn insert_outcome(state: &TableDialogState) -> TableOutcome {
    let (typst, cursor_offset) = generate(state.cols, state.rows, &state.headers);
    TableOutcome::Insert {
        typst,
        cursor_offset,
    }
}

/// Ligne « label [−] [valeur] [+] » : DragValue cliquable/glissable encadré de
/// boutons (egui 0.31 n'a pas de spinner natif). Bornage 1..=10.
fn spinner_row(ui: &mut egui::Ui, label: &str, value: &mut usize) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.button("−").clicked() && *value > 1 {
            *value -= 1;
        }
        ui.add(egui::DragValue::new(value).range(1..=10).speed(0.1));
        if ui.button("+").clicked() && *value < 10 {
            *value += 1;
        }
    });
}

/// Génère le Typst du tableau et le décalage (en chars) où placer le curseur
/// après insertion : dans la première cellule vide de la première ligne de corps.
///
/// ```typst
/// #table(
///   columns: 3,
///   [Col 1], [Col 2], [Col 3],
///   [ ], [ ], [ ],
/// )
/// ```
pub fn generate(cols: usize, rows: usize, headers: &[String]) -> (String, usize) {
    let cols = cols.clamp(1, 10);
    let rows = rows.clamp(1, 10);

    // En-tête : `#table(` + `columns: N,`.
    let mut header = String::from("#table(\n");
    header.push_str(&format!("  columns: {},\n", cols));
    header.push_str("  ");
    for c in 0..cols {
        let h = headers.get(c).map(String::as_str).unwrap_or("").trim();
        let h = if h.is_empty() {
            format!("Col {}", c + 1)
        } else {
            h.to_string()
        };
        header.push_str(&format!("[{h}]"));
        if c + 1 < cols {
            header.push_str(", ");
        }
    }
    header.push('\n');
    // Lignes de corps : `[ ]` séparés par des virgules.
    let mut body_row = String::from("  ");
    for c in 0..cols {
        body_row.push_str("[ ]");
        if c + 1 < cols {
            body_row.push_str(", ");
        }
    }
    body_row.push('\n');

    let mut out = String::new();
    out.push_str(&header);
    let cursor_offset = out.chars().count() + 4;
    for _ in 0..rows {
        out.push_str(&body_row);
    }
    out.push_str(")\n");

    // Curseur : début de la première cellule de corps.
    (out, cursor_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_3x3_table() {
        let headers = vec!["Col 1".into(), "Col 2".into(), "Col 3".into()];
        let (typst, _) = generate(3, 3, &headers);
        let lines: Vec<&str> = typst.lines().collect();
        assert_eq!(lines[0], "#table(");
        assert_eq!(lines[1], "  columns: 3,");
        assert!(lines[2].contains("[Col 1]"));
        assert!(lines[3].contains("[ ]"));
        assert_eq!(lines.last().copied(), Some(")"));
    }

    #[test]
    fn cursor_lands_in_first_body_cell() {
        let headers = vec!["A".into(), "B".into()];
        let (typst, off) = generate(2, 2, &headers);
        // Le caractère juste avant le curseur est le « | » + espace du 1er corps.
        let upto: String = typst.chars().take(off).collect();
        assert!(
            upto.ends_with("[ "),
            "curseur attendu après « [ », eu : {upto:?}"
        );
        // Et il reste dans la première ligne de corps (pas au-delà).
        assert!(off <= typst.chars().count());
    }

    #[test]
    fn empty_header_falls_back_to_col_n() -> Result<(), Box<dyn std::error::Error>> {
        let (typst, _) = generate(2, 1, &["".into(), "X".into()]);
        assert!(typst.lines().any(|l| l.contains("[Col 1]")));
        Ok(())
    }

    #[test]
    fn clamps_out_of_range() -> Result<(), Box<dyn std::error::Error>> {
        let (typst, _) = generate(0, 99, &[]);
        // 1 colonne minimum, 10 lignes maximum.
        assert!(typst.lines().any(|l| l.contains("columns: 1")));
        assert!(typst.lines().count() >= 4);
        Ok(())
    }
}
