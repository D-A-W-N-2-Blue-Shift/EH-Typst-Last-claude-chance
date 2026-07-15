// ============================================================================
// tools/nexus_inspect/src/main.rs — nexus_inspect (doc §5.8)
//
// Binaire SÉPARÉ, lecture seule (doc §5.8 : « Ouvre nexus.db et expose 5
// onglets... Règle no black box : toute donnée stockée doit être lisible
// ici sans SQL »). N'est PAS un module Nexus : pas de trait Module, pas de
// CoreContext, pas de registre — un outil d'audit autonome (doc §2, arbre
// « tools/nexus_inspect/ », sibling de modules/, pas dedans).
//
// Ouvre nexus.db via nexus_db::open_ro_db (SQLITE_OPEN_READ_ONLY +
// query_only=true, nexus_db/src/open.rs) : aucune écriture possible depuis
// cet outil, garanti au niveau SQLite, pas seulement une convention.
//
// Comment me virer : supprimer tools/nexus_inspect/ + retirer
// "tools/nexus_inspect" des `members` du Cargo.toml du workspace. Aucun
// autre binaire n'en dépend (outil autonome).
// ============================================================================

mod correlations;
mod integrity;

use std::path::PathBuf;

const APP_TITLE: &str = "Hive-RBMK-mod-Tcherenkov — nexus_inspect (lecture seule)";

fn main() {
    tracing_subscriber::fmt::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size([900.0, 660.0]),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|_cc| Ok(Box::new(InspectApp::default()))),
    ) {
        eprintln!("eframe a refusé de démarrer : {e}");
        std::process::exit(1);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Fichiers,
    Sante,
    Taches,
    Correlations,
    Integrite,
}

struct OpenProject {
    root: PathBuf,
    db: nexus_db::Connection,
}

#[derive(Default)]
struct InspectApp {
    path_input: String,
    proj: Option<OpenProject>,
    tab: Tab,
    error: Option<String>,
}

impl eframe::App for InspectApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(255, 46, 136), err.as_str());
                ui.separator();
            }
            if self.proj.is_some() {
                self.draw_project(ui);
            } else {
                self.draw_opener(ui);
            }
        });
    }
}

impl InspectApp {
    fn draw_opener(&mut self, ui: &mut egui::Ui) {
        ui.heading(APP_TITLE);
        ui.weak("Lecture seule : aucune écriture n'est possible depuis cet outil.");
        ui.add_space(8.0);
        ui.label("Chemin du projet Nexus :");
        ui.text_edit_singleline(&mut self.path_input);
        if ui.button("📂 Ouvrir").clicked() {
            self.open();
        }
    }

    fn open(&mut self) {
        self.error = None;
        let raw = self.path_input.trim();
        if raw.is_empty() {
            self.error = Some("Indique un chemin de projet.".into());
            return;
        }
        let root = match std::fs::canonicalize(raw) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(format!("Dossier introuvable : '{raw}' ({e})."));
                return;
            }
        };
        let db_path = root.join(".engram").join("nexus.db");
        match nexus_db::open_ro_db(&db_path) {
            Ok(db) => self.proj = Some(OpenProject { root, db }),
            Err(e) => {
                self.error = Some(format!(
                    "Impossible d'ouvrir {} en lecture seule ({e}).",
                    db_path.display()
                ));
            }
        }
    }

    fn draw_project(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        let mut selected_tab = self.tab;
        if let Some(proj) = &self.proj {
            ui.horizontal(|ui| {
                ui.heading("nexus_inspect");
                ui.weak(proj.root.display().to_string());
                if ui.small_button("Fermer le projet").clicked() {
                    close = true;
                }
            });
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Fichiers, "Fichiers"),
                    (Tab::Sante, "Santé"),
                    (Tab::Taches, "Tâches"),
                    (Tab::Correlations, "Corrélations"),
                    (Tab::Integrite, "Intégrité"),
                ] {
                    if ui.selectable_label(selected_tab == tab, label).clicked() {
                        selected_tab = tab;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match selected_tab {
                    Tab::Fichiers => draw_fichiers(ui, &proj.db),
                    Tab::Sante => draw_sante(ui, &proj.db),
                    Tab::Taches => draw_taches(ui, &proj.db),
                    Tab::Correlations => draw_correlations(ui, &proj.db),
                    Tab::Integrite => draw_integrite(ui, &proj.db, &proj.root),
                });
        }
        self.tab = selected_tab;
        if close {
            self.proj = None;
        }
    }
}

fn error_label(ui: &mut egui::Ui, msg: impl AsRef<str>) {
    ui.colored_label(egui::Color32::from_rgb(255, 46, 136), msg.as_ref());
}

fn draw_fichiers(ui: &mut egui::Ui, db: &nexus_db::Connection) {
    ui.strong("journal_entries");
    match nexus_db::list_journal_entries(db) {
        Ok(entries) => {
            if entries.is_empty() {
                ui.weak("Aucune entrée.");
            }
            for e in &entries {
                ui.label(format!(
                    "{} · {} · {} mot(s) · tags: {}",
                    e.date,
                    e.file_path,
                    e.word_count,
                    e.tags.join(", ")
                ));
            }
        }
        Err(err) => error_label(ui, format!("Lecture journal_entries impossible : {err}")),
    }
    ui.add_space(10.0);
    ui.strong("articles");
    match nexus_db::list_articles(db) {
        Ok(articles) => {
            if articles.is_empty() {
                ui.weak("Aucun article.");
            }
            for a in &articles {
                ui.label(format!(
                    "{} · {} · statut={} · cible={} · dest={} · {} mot(s) · maj {} · tags: {}",
                    a.titre,
                    a.file_path,
                    a.statut,
                    a.date_cible,
                    a.destination,
                    a.word_count,
                    a.updated_at,
                    a.tags.join(", ")
                ));
            }
        }
        Err(err) => error_label(ui, format!("Lecture articles impossible : {err}")),
    }
}

fn draw_sante(ui: &mut egui::Ui, db: &nexus_db::Connection) {
    ui.strong("sleep_log");
    match nexus_db::list_sleep_logs(db) {
        Ok(logs) => {
            if logs.is_empty() {
                ui.weak("Aucune nuit enregistrée.");
            }
            for l in &logs {
                ui.label(format!(
                    "{} · {}h · qualité {}/5 · {}",
                    l.date,
                    l.duration_h,
                    l.quality,
                    l.notes.as_deref().unwrap_or("")
                ));
            }
        }
        Err(err) => error_label(ui, format!("Lecture sleep_log impossible : {err}")),
    }
    ui.add_space(10.0);
    ui.strong("med_doses");
    match nexus_db::list_med_doses(db) {
        Ok(doses) => {
            if doses.is_empty() {
                ui.weak("Aucune prise enregistrée.");
            }
            for d in &doses {
                ui.label(format!(
                    "{} · med_id={} · {}mg · ressenti={}",
                    d.taken_at,
                    d.med_id,
                    d.dose_mg,
                    d.ressenti
                        .map(|r| r.to_string())
                        .unwrap_or_else(|| "—".into())
                ));
            }
        }
        Err(err) => error_label(ui, format!("Lecture med_doses impossible : {err}")),
    }
    ui.add_space(10.0);
    ui.strong("mood_log");
    match nexus_db::list_mood_logs(db) {
        Ok(logs) => {
            if logs.is_empty() {
                ui.weak("Aucune saisie d'état psy.");
            }
            for m in &logs {
                ui.label(format!(
                    "{} · épuisement={} cognitif={} sensoriel={} masquage={} fonctionnement={}",
                    m.logged_at,
                    m.epuisement,
                    m.cognitif,
                    m.sensoriel,
                    m.masquage,
                    m.fonctionnement
                ));
            }
        }
        Err(err) => error_label(ui, format!("Lecture mood_log impossible : {err}")),
    }
}

fn load_tasks_and_history(
    db: &nexus_db::Connection,
) -> Result<(Vec<nexus_db::Task>, Vec<nexus_db::TaskHistory>), nexus_db::NexusDbError> {
    let tasks = nexus_db::list_tasks(db)?;
    let mut history = Vec::new();
    for t in &tasks {
        history.extend(nexus_db::list_task_history(db, &t.id)?);
    }
    Ok((tasks, history))
}

fn draw_taches(ui: &mut egui::Ui, db: &nexus_db::Connection) {
    match load_tasks_and_history(db) {
        Ok((tasks, history)) => {
            ui.strong("tasks");
            if tasks.is_empty() {
                ui.weak("Aucune tâche.");
            }
            for t in &tasks {
                ui.label(format!(
                    "{} · [{}] {} · énergie={} contexte={} durée={} échéance={} · créée {} maj {}",
                    t.id,
                    t.statut,
                    t.titre,
                    t.energie.as_deref().unwrap_or("—"),
                    t.contexte.as_deref().unwrap_or("—"),
                    t.duree_min
                        .map(|d| format!("{d}min"))
                        .unwrap_or_else(|| "—".into()),
                    t.echeance.as_deref().unwrap_or("—"),
                    t.created_at,
                    t.updated_at
                ));
            }
            ui.add_space(10.0);
            ui.strong("task_history");
            if history.is_empty() {
                ui.weak("Aucune transition enregistrée.");
            }
            for h in &history {
                ui.label(format!(
                    "{} : {} → {} le {}",
                    h.task_id, h.ancien_statut, h.nouveau_statut, h.changed_at
                ));
            }
        }
        Err(err) => error_label(ui, format!("Lecture tasks/task_history impossible : {err}")),
    }
}

fn draw_correlations(ui: &mut egui::Ui, db: &nexus_db::Connection) {
    ui.weak("Les 4 vues nommées par le doc §4.2 sont listées ci-dessous (points bruts).");
    ui.add_space(6.0);
    let (tasks, history) = match load_tasks_and_history(db) {
        Ok(v) => v,
        Err(err) => {
            error_label(ui, format!("Lecture tasks/task_history impossible : {err}"));
            return;
        }
    };
    let mood = match nexus_db::list_mood_logs(db) {
        Ok(m) => m,
        Err(err) => {
            error_label(ui, format!("Lecture mood_log impossible : {err}"));
            Vec::new()
        }
    };
    ui.strong("weekly_load");
    let load = correlations::weekly_load(&tasks, &history, &mood);
    if load.is_empty() {
        ui.weak("Aucune donnée.");
    }
    for w in &load {
        ui.label(format!(
            "{} · créées={} terminées={} épuisement moyen={}",
            correlations::week_label(w.week),
            w.created,
            w.done,
            w.avg_epuisement
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "—".into())
        ));
    }
    ui.add_space(10.0);
    ui.strong("med_observance");
    let medications = match nexus_db::list_medications(db) {
        Ok(m) => m,
        Err(err) => {
            error_label(ui, format!("Lecture medications impossible : {err}"));
            Vec::new()
        }
    };
    let doses = match nexus_db::list_med_doses(db) {
        Ok(d) => d,
        Err(err) => {
            error_label(ui, format!("Lecture med_doses impossible : {err}"));
            Vec::new()
        }
    };
    let obs = correlations::med_observance(&doses, &medications, &mood);
    if obs.is_empty() {
        ui.weak("Aucune donnée.");
    }
    for w in &obs {
        let by_med = w
            .doses_by_med
            .iter()
            .map(|(nom, n)| format!("{nom}={n}"))
            .collect::<Vec<_>>()
            .join(", ");
        ui.label(format!(
            "{} · prises: {by_med} · fonctionnement moyen={}",
            correlations::week_label(w.week),
            w.avg_fonctionnement
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "—".into())
        ));
    }
    ui.add_space(10.0);
    ui.strong("Tâches bloquées depuis plus de 7 jours");
    let now = chrono::Local::now().naive_local();
    let blocked = correlations::blocked_over(&tasks, now, 7);
    if blocked.is_empty() {
        ui.weak("Aucune.");
    }
    for t in &blocked {
        ui.label(format!("{} (bloquée depuis {})", t.titre, t.updated_at));
    }

    ui.add_space(10.0);
    ui.strong("corr_sommeil_cognition (sommeil J-1 → cognitif J)");
    let sleep = match nexus_db::list_sleep_logs(db) {
        Ok(s) => s,
        Err(err) => {
            error_label(ui, format!("Lecture sleep_log impossible : {err}"));
            Vec::new()
        }
    };
    let sommeil_cognitif = correlations::corr_sommeil_cognition(&sleep, &mood);
    if sommeil_cognitif.is_empty() {
        ui.weak("Aucune donnée.");
    } else {
        let r2_points: Vec<(f64, f64)> = sommeil_cognitif
            .iter()
            .map(|p| (p.duration_h, p.cognitif as f64))
            .collect();
        let r2_label = correlations::r_squared(&r2_points)
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "non calculable".into());
        ui.weak(format!(
            "{} point(s) · r² (durée vs cognitif) = {r2_label}",
            sommeil_cognitif.len()
        ));
        for p in &sommeil_cognitif {
            ui.label(format!(
                "durée={:.1}h qualité={} cognitif={} épuisement={}",
                p.duration_h, p.quality, p.cognitif, p.epuisement
            ));
        }
    }

    ui.add_space(10.0);
    ui.strong("corr_medication_etat (courbe empirique médication)");
    let med_etat = correlations::corr_medication_etat(&doses, &mood);
    if med_etat.is_empty() {
        ui.weak("Aucune donnée (aucune saisie d'état psy dans les 12h suivant une prise).");
    } else {
        ui.weak(format!(
            "{} point(s) brut(s) — pas de lissage LOESS ici (voir README_MODULE.md).",
            med_etat.len()
        ));
        for p in &med_etat {
            ui.label(format!(
                "+{} min · cognitif={} fonctionnement={}",
                p.delta_minutes, p.cognitif, p.fonctionnement
            ));
        }
    }
}

fn draw_integrite(ui: &mut egui::Ui, db: &nexus_db::Connection, root: &std::path::Path) {
    let tasks = match nexus_db::list_tasks(db) {
        Ok(t) => t,
        Err(err) => {
            error_label(ui, format!("Lecture tasks impossible : {err}"));
            return;
        }
    };
    let mut task_history = Vec::new();
    for t in &tasks {
        match nexus_db::list_task_history(db, &t.id) {
            Ok(h) => task_history.extend(h),
            Err(err) => error_label(ui, format!("Lecture task_history impossible : {err}")),
        }
    }
    let medications = nexus_db::list_medications(db).unwrap_or_default();
    let med_doses = nexus_db::list_med_doses(db).unwrap_or_default();
    let journal_entries = nexus_db::list_journal_entries(db).unwrap_or_default();
    let articles = nexus_db::list_articles(db).unwrap_or_default();

    let inputs = integrity::Inputs {
        root,
        tasks: &tasks,
        task_history: &task_history,
        medications: &medications,
        med_doses: &med_doses,
        journal_entries: &journal_entries,
        articles: &articles,
    };
    let issues = integrity::run_all_checks(&inputs);
    if issues.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(120, 255, 180),
            "✓ Aucun problème détecté.",
        );
        return;
    }
    ui.colored_label(
        egui::Color32::from_rgb(255, 46, 136),
        format!("{} problème(s) détecté(s).", issues.len()),
    );
    ui.add_space(6.0);
    for i in &issues {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(egui::Color32::from_rgb(255, 200, 0), i.category);
            ui.label(&i.detail);
        });
    }
}
