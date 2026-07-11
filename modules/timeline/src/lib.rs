// ============================================================================
// modules/timeline/src/lib.rs — Matrice Temporelle Multi-Entités
//
// Fenêtre OS dédiée. Lit en LECTURE SEULE l'index SQLite produit par file_tree
// (<projet>/.engram/index.db) et présente une GRILLE :
//   - Lignes (axe Y) : toutes les dates détectées, triées chronologiquement.
//   - Colonnes (axe X) : les personnages cochés (1 à N).
//   - Cellule (date × perso) :
//       [Scène] (source : YAML scène)  → n° d'ordre, lieu, co-présents.
//       [Fiche] (source : YAML perso)  → note biographique de la fiche.
//
// Deux lectures d'analyse :
//   - Horizontale : cohérence état interne [Fiche] vs action [Scène] à une date.
//   - Verticale   : trou narratif = date avec [Fiche] mais aucune [Scène]
//                   parmi les personnages sélectionnés (ligne ⚠ surlignée).
//
// L'application n'écrit JAMAIS dans les fichiers sources. Le tri repose sur la
// clé `date_sort` (YYYYMMDDHHMM) calculée par l'indexeur ; l'affichage utilise
// `date_display` (format auteur dd-mm-yyyy HHhMM).
//
// Couplage : ne dépend PAS du crate file_tree. Seule convention partagée : le
// chemin DB et le schéma des tables scenes / scene_personnages / perso_chrono.
// ============================================================================

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse, RenderMode};

const COL_SCENE: egui::Color32 = egui::Color32::from_rgb(120, 180, 255);
const COL_FICHE: egui::Color32 = egui::Color32::from_rgb(255, 200, 120);
const COL_GAP: egui::Color32 = egui::Color32::from_rgb(255, 90, 120);

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellKind {
    Scene,
    Fiche,
}

#[derive(Clone)]
struct CellItem {
    kind: CellKind,
    text: String,
    /// Fichier source à ouvrir au clic.
    path: PathBuf,
}

struct DateRow {
    sort: String,
    display: String,
}

#[derive(Default)]
struct Matrix {
    /// Dates uniques, triées chronologiquement (par `sort`).
    dates: Vec<DateRow>,
    /// Personnages uniques (scènes + fiches), triés alphabétiquement.
    personnages: Vec<String>,
    /// (date_sort, personnage) → événements de la cellule.
    cells: HashMap<(String, String), Vec<CellItem>>,
}

/// §3.4 — Vue actuelle de la fenêtre timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimelineView {
    /// Matrice historique (scènes × personnages × dates), voir en-tête du fichier.
    Matrix,
    /// Liste chronologique des événements des deux fichiers globaux
    /// chronologie/{biographies,evenements}.typ, triée par date_sortable
    /// (NULL en bas).
    Chronology,
}

/// Un événement à afficher dans la vue chronologique (§3.4).
#[derive(Debug, Clone)]
struct ChronologyEvent {
    source_path: PathBuf,
    section_title: String,
    date_raw: String,
    date_sortable: Option<String>,
    body_excerpt: String,
    linked_targets: Vec<String>,
}

pub struct TimelineModule {
    project_root: Option<PathBuf>,
    matrix: Matrix,
    /// Personnages cochés (colonnes affichées).
    selected: BTreeSet<String>,
    status: Vec<String>,
    closed: bool,
    view: TimelineView,
    chronology: Vec<ChronologyEvent>,
}

impl Default for TimelineModule {
    fn default() -> Self {
        Self {
            project_root: None,
            matrix: Matrix::default(),
            selected: BTreeSet::new(),
            status: Vec::new(),
            closed: true,
            view: TimelineView::Matrix,
            chronology: Vec::new(),
        }
    }
}

impl Module for TimelineModule {
    fn name(&self) -> &'static str {
        "timeline"
    }

    fn init(&mut self, _ctx: &CoreContext) -> Result<(), String> {
        Ok(())
    }

    fn render_mode(&self) -> RenderMode {
        RenderMode::OwnViewport
    }

    fn handle_event(&mut self, event: &CoreEvent) {
        match event {
            // file_tree publie l'index → on rafraîchit le cache sans ouvrir
            // la fenêtre (rafraîchir n'ouvre pas).
            CoreEvent::FileIndexUpdated { project_root, .. } => {
                self.project_root = Some(project_root.clone());
                self.reload();
                self.reload_chronology();
            }
            CoreEvent::OpenModuleWindowRequested(name) if name == self.name() => {
                self.closed = false;
            }
            CoreEvent::TimelineChronologyRequested => {
                self.closed = false;
                self.view = TimelineView::Chronology;
                self.reload_chronology();
            }
            _ => {}
        }
    }

    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        if self.closed {
            return;
        }
        let viewport_id = egui::ViewportId::from_hash_of("timeline");
        let builder = egui::ViewportBuilder::default()
            .with_title("Engram Hive — Matrice temporelle")
            .with_inner_size([900.0, 600.0]);
        let mut open_palette = false;
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                self.closed = true;
                return;
            }
            // §7 — Ctrl+Shift+P depuis la fenêtre Timeline.
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                    egui::Key::P,
                )
            }) {
                open_palette = true;
            }
            self.draw(ctx, out);
        });
        if open_palette {
            out.push(ModuleResponse::OpenPaletteRequested);
        }
    }

    fn active_viewport_count(&self) -> usize {
        if self.closed {
            0
        } else {
            1
        }
    }

    fn shutdown(&mut self) {}
}

impl TimelineModule {
    fn reload(&mut self) {
        let Some(root) = self.project_root.as_ref() else {
            self.matrix = Matrix::default();
            return;
        };
        let db_path = root.join(".engram").join("index.db");
        if !db_path.exists() {
            self.status.push(format!(
                "Index SQLite absent ({}). Ouvre le projet dans le file_tree pour qu'il l'indexe.",
                db_path.display()
            ));
            self.matrix = Matrix::default();
            return;
        }
        match load_matrix(&db_path) {
            Ok(m) => {
                // Conserve la sélection encore valide ; si rien ne reste,
                // tout sélectionner (matrice non vide au premier affichage).
                self.selected.retain(|p| m.personnages.contains(p));
                if self.selected.is_empty() {
                    self.selected = m.personnages.iter().cloned().collect();
                }
                self.matrix = m;
                self.status.clear();
            }
            Err(e) => {
                self.status.push(format!("Lecture index : {e}"));
                self.matrix = Matrix::default();
            }
        }
    }

    fn draw(&mut self, ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        egui::TopBottomPanel::top("timeline_header").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let title = match self.view {
                    TimelineView::Matrix => "Matrice temporelle",
                    TimelineView::Chronology => "Chronologie",
                };
                ui.heading(title);
                ui.add_space(8.0);
                if ui.button("⟳ Rafraîchir").clicked() {
                    self.reload();
                    self.reload_chronology();
                }
                ui.add_space(8.0);
                // §3.4 — bascule matrice ↔ chronologie.
                ui.selectable_value(&mut self.view, TimelineView::Matrix, "Matrice");
                ui.selectable_value(&mut self.view, TimelineView::Chronology, "Chronologie");
                ui.add_space(12.0);
                match self.view {
                    TimelineView::Matrix => {
                        ui.colored_label(COL_SCENE, "■ Scène");
                        ui.colored_label(COL_FICHE, "■ Fiche");
                        ui.colored_label(COL_GAP, "⚠ Trou narratif");
                        ui.add_space(12.0);
                        ui.weak(format!(
                            "{} dates · {} personnages",
                            self.matrix.dates.len(),
                            self.matrix.personnages.len()
                        ));
                    }
                    TimelineView::Chronology => {
                        ui.weak(format!("{} événements", self.chronology.len()));
                    }
                }
            });
            self.draw_status(ui);
            ui.add_space(2.0);
        });

        if matches!(self.view, TimelineView::Chronology) {
            self.draw_chronology(ctx, out);
            return;
        }

        if self.project_root.is_none() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label(
                    "Ouvre un projet dans le file_tree pour activer la matrice. \
                     Les scènes (YAML `date`) et les fiches personnages (YAML \
                     `chronologie`) y apparaîtront, croisées par date et personnage.",
                );
            });
            return;
        }

        if self.matrix.dates.is_empty() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.weak("Aucune scène ni chronologie datée dans l'index.");
                ui.add_space(8.0);
                ui.label("Fichier SCÈNE — frontmatter YAML :");
                ui.monospace("date: 15-03-1942 08h30");
                ui.monospace("lieu: Berlin");
                ui.monospace("personnages: [Anna, Karl]");
                ui.add_space(6.0);
                ui.label("Fichier PERSONNAGE — frontmatter YAML :");
                ui.monospace("chronologie:");
                ui.monospace("  - 15-03-1942 - doute, veut fuir");
                ui.monospace("  - 1943 - rupture définitive");
            });
            return;
        }

        // Colonnes : sélecteur de personnages.
        egui::SidePanel::left("timeline_persos")
            .resizable(true)
            .default_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.strong("Personnages");
                });
                ui.horizontal(|ui| {
                    if ui.small_button("Tout").clicked() {
                        self.selected = self.matrix.personnages.iter().cloned().collect();
                    }
                    if ui.small_button("Aucun").clicked() {
                        self.selected.clear();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let persos = self.matrix.personnages.clone();
                        for p in persos {
                            let mut on = self.selected.contains(&p);
                            if ui.checkbox(&mut on, &p).changed() {
                                if on {
                                    self.selected.insert(p.clone());
                                } else {
                                    self.selected.remove(&p);
                                }
                            }
                        }
                    });
            });

        // Grille dates × personnages cochés.
        let cols: Vec<String> = self
            .matrix
            .personnages
            .iter()
            .filter(|p| self.selected.contains(*p))
            .cloned()
            .collect();

        let mut to_open: Option<PathBuf> = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            if cols.is_empty() {
                ui.weak("Coche au moins un personnage à gauche pour afficher ses colonnes.");
                return;
            }
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("timeline_matrix")
                        .striped(true)
                        .spacing([14.0, 6.0])
                        .show(ui, |ui| {
                            ui.strong("Date");
                            for c in &cols {
                                ui.strong(c);
                            }
                            ui.end_row();

                            for date in &self.matrix.dates {
                                // Trou narratif : ≥1 [Fiche] et 0 [Scène] parmi
                                // les colonnes affichées, à cette date.
                                let mut has_scene = false;
                                let mut has_fiche = false;
                                for c in &cols {
                                    if let Some(items) =
                                        self.matrix.cells.get(&(date.sort.clone(), c.clone()))
                                    {
                                        for it in items {
                                            match it.kind {
                                                CellKind::Scene => has_scene = true,
                                                CellKind::Fiche => has_fiche = true,
                                            }
                                        }
                                    }
                                }
                                let gap = has_fiche && !has_scene;
                                if gap {
                                    ui.colored_label(COL_GAP, format!("⚠ {}", date.display));
                                } else {
                                    ui.label(&date.display);
                                }

                                for c in &cols {
                                    let key = (date.sort.clone(), c.clone());
                                    if let Some(items) = self.matrix.cells.get(&key) {
                                        ui.vertical(|ui| {
                                            for it in items {
                                                let color = match it.kind {
                                                    CellKind::Scene => COL_SCENE,
                                                    CellKind::Fiche => COL_FICHE,
                                                };
                                                let label = egui::Label::new(
                                                    egui::RichText::new(it.text.as_str())
                                                        .color(color),
                                                )
                                                .sense(egui::Sense::click())
                                                .wrap();
                                                if ui.add(label).clicked() {
                                                    to_open = Some(it.path.clone());
                                                }
                                            }
                                        });
                                    } else {
                                        ui.label("");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    ui.add_space(6.0);
                    ui.weak("Clic sur un événement pour ouvrir le fichier source dans l'éditeur.");
                });
        });

        if let Some(path) = to_open {
            out.push(ModuleResponse::OpenFile(path));
        }
    }

    // §3.4 — Recharge la liste des événements des deux fichiers globaux
    // chronologie/{biographies,evenements}.typ depuis timeline_events +
    // timeline_links. Tri : date_sortable croissant, NULL en bas.
    fn reload_chronology(&mut self) {
        let Some(root) = self.project_root.as_ref() else {
            self.chronology.clear();
            return;
        };
        let db_path = root.join(".engram").join("index.db");
        if !db_path.exists() {
            self.chronology.clear();
            return;
        }
        match load_chronology(&db_path) {
            Ok(list) => {
                self.chronology = list;
            }
            Err(e) => {
                self.status.push(format!("Lecture chronologie : {e}"));
                self.chronology.clear();
            }
        }
    }

    fn draw_chronology(&mut self, ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        let mut to_open = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.chronology.is_empty() {
                ui.weak(
                    "Aucun événement indexé. Les deux fichiers globaux \
                     01_architecture/chronologie/biographies.typ et \
                     01_architecture/chronologie/evenements.typ doivent exister \
                     et le file_tree doit les avoir indexés.",
                );
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for evt in &self.chronology {
                    ui.horizontal_top(|ui| {
                        // Colonne date : sortable si dispo, sinon raw en gris.
                        let (date_txt, color) = match &evt.date_sortable {
                            Some(d) => (d.clone(), egui::Color32::LIGHT_GREEN),
                            None => (evt.date_raw.clone(), egui::Color32::GRAY),
                        };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(date_txt).color(color).monospace(),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                        ui.separator();
                        ui.vertical(|ui| {
                            let title_resp = ui.add(
                                egui::Label::new(egui::RichText::new(&evt.section_title).strong())
                                    .sense(egui::Sense::click()),
                            );
                            if title_resp.clicked() {
                                to_open = Some(evt.source_path.clone());
                            }
                            if !evt.linked_targets.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.weak("Liens :");
                                    for t in &evt.linked_targets {
                                        ui.colored_label(COL_SCENE, format!("[[{t}]]"));
                                    }
                                });
                            }
                            ui.weak(&evt.body_excerpt);
                        });
                    });
                    ui.separator();
                }
            });
        });
        if let Some(path) = to_open {
            out.push(ModuleResponse::OpenFile(path));
        }
    }

    fn draw_status(&mut self, ui: &mut egui::Ui) {
        if self.status.is_empty() {
            return;
        }
        let mut dismiss = None;
        for (i, msg) in self.status.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(COL_GAP, "⚠");
                ui.label(msg.as_str());
                if ui.small_button("✕").clicked() {
                    dismiss = Some(i);
                }
            });
        }
        if let Some(i) = dismiss {
            self.status.remove(i);
        }
    }
}

/// Charge la matrice depuis l'index SQLite (lecture seule). Une requête par
/// table (scenes, scene_personnages, perso_chrono), puis fusion en mémoire.
fn load_matrix(db_path: &Path) -> Result<Matrix, String> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("open {} : {e}", db_path.display()))?;

    // 1) Personnages par scène (pour lister les co-présents dans la cellule).
    let mut by_scene: HashMap<String, Vec<String>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT scene_path, perso FROM scene_personnages")
            .map_err(|e| format!("prepare scene_personnages : {e}"))?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| format!("query scene_personnages : {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("rows scene_personnages : {e}"))?;
        for (path, perso) in rows {
            by_scene.entry(path).or_default().push(perso);
        }
    }

    let mut cells: HashMap<(String, String), Vec<CellItem>> = HashMap::new();
    let mut dates: HashMap<String, String> = HashMap::new();
    let mut persos: BTreeSet<String> = BTreeSet::new();

    // 2) Scènes.
    {
        let mut stmt = conn
            .prepare("SELECT scene_path, date_sort, date_display, lieu, ordre FROM scenes")
            .map_err(|e| format!("prepare scenes : {e}"))?;
        let rows: Vec<(String, String, String, String, Option<i64>)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| format!("query scenes : {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("rows scenes : {e}"))?;
        for (path, sort, display, lieu, ordre) in rows {
            dates.insert(sort.clone(), display);
            let members = by_scene.get(&path).cloned().unwrap_or_default();
            for perso in &members {
                persos.insert(perso.clone());
                let copres: Vec<&str> = members
                    .iter()
                    .filter(|m| *m != perso)
                    .map(String::as_str)
                    .collect();
                let mut text = match ordre {
                    Some(o) => format!("Scène #{o}"),
                    None => "Scène".to_string(),
                };
                if !lieu.is_empty() {
                    text.push_str(&format!(" · {lieu}"));
                }
                if !copres.is_empty() {
                    text.push_str(&format!(" · avec {}", copres.join(", ")));
                }
                cells
                    .entry((sort.clone(), perso.clone()))
                    .or_default()
                    .push(CellItem {
                        kind: CellKind::Scene,
                        text,
                        path: PathBuf::from(&path),
                    });
            }
        }
    }

    // 3) Chronologies internes des fiches personnages.
    {
        let mut stmt = conn
            .prepare(
                "SELECT perso_name, date_sort, date_display, note, perso_path FROM perso_chrono",
            )
            .map_err(|e| format!("prepare perso_chrono : {e}"))?;
        let rows: Vec<(String, String, String, String, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| format!("query perso_chrono : {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("rows perso_chrono : {e}"))?;
        for (name, sort, display, note, path) in rows {
            dates.insert(sort.clone(), display);
            persos.insert(name.clone());
            cells
                .entry((sort.clone(), name.clone()))
                .or_default()
                .push(CellItem {
                    kind: CellKind::Fiche,
                    text: note,
                    path: PathBuf::from(&path),
                });
        }
    }

    let mut dates_vec: Vec<DateRow> = dates
        .into_iter()
        .map(|(sort, display)| DateRow { sort, display })
        .collect();
    dates_vec.sort_by(|a, b| a.sort.cmp(&b.sort));

    Ok(Matrix {
        dates: dates_vec,
        personnages: persos.into_iter().collect(),
        cells,
    })
}

/// §3.4 — Charge la liste chronologique des événements des deux fichiers
/// globaux. Tri : date_sortable croissant (NULL en bas via CASE).
fn load_chronology(db_path: &Path) -> Result<Vec<ChronologyEvent>, String> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("open {} : {e}", db_path.display()))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, source_path, section_title, date_raw, date_sortable, body_excerpt
             FROM timeline_events
             ORDER BY CASE WHEN date_sortable IS NULL THEN 1 ELSE 0 END,
                      date_sortable,
                      id",
        )
        .map_err(|e| format!("prepare timeline_events : {e}"))?;
    let rows: Vec<(i64, String, String, String, Option<String>, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .map_err(|e| format!("query timeline_events : {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("rows timeline_events : {e}"))?;

    // Liens par event_id, joints en mémoire.
    let mut by_event: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT event_id, target_name FROM timeline_links")
            .map_err(|e| format!("prepare timeline_links : {e}"))?;
        let link_rows: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| format!("query timeline_links : {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("rows timeline_links : {e}"))?;
        for (eid, target) in link_rows {
            by_event.entry(eid).or_default().push(target);
        }
    }

    let events = rows
        .into_iter()
        .map(
            |(id, source, title, date_raw, date_sortable, excerpt)| ChronologyEvent {
                source_path: PathBuf::from(source),
                section_title: title,
                date_raw,
                date_sortable,
                body_excerpt: excerpt,
                linked_targets: by_event.remove(&id).unwrap_or_default(),
            },
        )
        .collect();
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Crée un index minimal avec le schéma temporel et y insère des données.
    fn seed_db(path: &Path) -> rusqlite::Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE scenes (
                scene_path TEXT PRIMARY KEY, date_sort TEXT NOT NULL,
                date_display TEXT NOT NULL, lieu TEXT NOT NULL DEFAULT '', ordre INTEGER
            );
            CREATE TABLE scene_personnages (scene_path TEXT, perso TEXT);
            CREATE TABLE perso_chrono (
                perso_path TEXT, perso_name TEXT NOT NULL, date_sort TEXT NOT NULL,
                date_display TEXT NOT NULL, note TEXT NOT NULL DEFAULT ''
            );",
        )?;
        Ok(conn)
    }

    #[test]
    fn matrice_croise_scene_et_fiche_par_date_et_perso() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let db = dir.path().join("index.db");
        let conn = seed_db(&db)?;
        // Scène du 15-03-1942 avec Anna et Karl.
        conn.execute(
            "INSERT INTO scenes VALUES ('/p/sc1.typ', '194203150830', '15-03-1942 08h30', 'Berlin', 12)",
            [],
        )?;
        conn.execute(
            "INSERT INTO scene_personnages VALUES ('/p/sc1.typ', 'Anna')",
            [],
        )?;
        conn.execute(
            "INSERT INTO scene_personnages VALUES ('/p/sc1.typ', 'Karl')",
            [],
        )?;
        // Fiche d'Anna : événement biographique en 1943 (sans scène → trou).
        conn.execute(
            "INSERT INTO perso_chrono VALUES ('/p/anna.typ', 'Anna', '194300000000', '1943', 'rupture')",
            [],
        )?;
        drop(conn);

        let m = load_matrix(&db)?;

        // Deux dates, triées chronologiquement.
        assert_eq!(m.dates.len(), 2);
        assert_eq!(m.dates[0].sort, "194203150830");
        assert_eq!(m.dates[1].sort, "194300000000");
        // Deux personnages.
        assert_eq!(m.personnages, vec!["Anna", "Karl"]);

        // Cellule (1942 scène, Anna) = une scène mentionnant Karl comme co-présent.
        let cell = m
            .cells
            .get(&("194203150830".into(), "Anna".into()))
            .ok_or("cellule Anna/scène attendue")?;
        assert_eq!(cell.len(), 1);
        assert_eq!(cell[0].kind as u8, CellKind::Scene as u8);
        assert!(cell[0].text.contains("Karl"));
        assert!(cell[0].text.contains("Berlin"));

        // Cellule (1943, Anna) = une fiche.
        let cell = m
            .cells
            .get(&("194300000000".into(), "Anna".into()))
            .ok_or("cellule Anna/fiche attendue")?;
        assert_eq!(cell[0].kind as u8, CellKind::Fiche as u8);
        assert_eq!(cell[0].text, "rupture");
        Ok(())
    }
}
