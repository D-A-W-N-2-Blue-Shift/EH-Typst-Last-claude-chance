// ============================================================================
// modules/dashboard/src/lib.rs — Point d'entrée du module Dashboard
//
// Ce que je fais (doc §5.6) : vue Sommeil (barres durée + ligne moyenne
// glissante 28j + qualité en overlay, fenêtre 30/60/90 jours), vue État psy
// (radar semaine courante vs précédente, tendance 30 jours par dimension,
// alerte si une dimension < 2 sur 3 jours consécutifs), et vue Corrélations
// — les 4 vues nommées par le doc §4.2 sont TOUTES représentées : charge
// tâches/vélocité/énergie/bloquées, observance médication, sommeil→cognitif
// (scatter + r²), courbe empirique médication (scatter brut ; SANS lissage
// LOESS — voir correlations.rs et README_MODULE.md pour l'écart documenté).
// Aucune donnée inventée : sous le seuil minimal, j'affiche "référentiel
// insuffisant" plutôt qu'une courbe extrapolée. Export CSV/.ics des données
// brutes de chaque vue.
//
// Comment je marche : OwnViewport, fenêtre à la demande. J'apprends la
// racine du projet actif via CoreEvent::ProjectRootUpdated (même mécanisme
// que health/journal/todo) et j'ouvre ma PROPRE connexion à nexus.db.
//
// Comment me virer : supprimer modules/dashboard/ + retirer "dashboard" du
// registre du binaire nexus (app_nexus/src/main.rs) + la dépendance de
// app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module), nexus_db (couche de
// données), chrono (dates/fenêtres glissantes).
// ============================================================================

mod charts;
mod config;
mod correlations;
mod ics;
mod logic;

use std::path::PathBuf;

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse};

struct UserError {
    user_message: String,
    #[allow(dead_code)]
    technical: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Sommeil,
    EtatPsy,
    Correlations,
}

pub struct DashboardModule {
    closed: bool,
    project_root: Option<PathBuf>,
    db: Option<nexus_db::Connection>,
    tab: Tab,
    sleep_window_days: i64,
    /// Confirmation d'export affichée dans la fenêtre — distincte de
    /// `glados` (erreurs) : un export réussi n'est pas une erreur.
    last_export: Option<String>,
    /// Section "dashboard" de Hive_RBMK.ron (doc §5.7), chargée à `init()`.
    cfg: config::DashboardConfig,
    glados: Vec<UserError>,
    pending: Vec<ModuleResponse>,
}

impl Default for DashboardModule {
    fn default() -> Self {
        let cfg = config::DashboardConfig::default();
        Self {
            closed: true,
            project_root: None,
            db: None,
            tab: Tab::default(),
            sleep_window_days: cfg.default_sleep_window_days,
            last_export: None,
            cfg,
            glados: Vec::new(),
            pending: Vec::new(),
        }
    }
}

impl DashboardModule {
    fn glados(&mut self, user_message: impl Into<String>, technical: impl Into<String>) {
        let user_message = user_message.into();
        let technical = technical.into();
        tracing::error!(target: "dashboard", "{user_message} ({technical})");
        self.pending.push(ModuleResponse::Error {
            module: "dashboard".into(),
            message: user_message.clone(),
        });
        self.glados.push(UserError {
            user_message,
            technical,
        });
        if self.glados.len() > 4 {
            self.glados.remove(0);
        }
    }

    fn set_project_root(&mut self, root: Option<PathBuf>) {
        self.db = None;
        self.project_root = root.clone();
        let Some(root) = root else { return };
        let db_path = root.join(".engram").join("nexus.db");
        match nexus_db::open_db(&db_path) {
            Ok(c) => self.db = Some(c),
            Err(e) => self.glados(
                format!(
                    "Impossible d'ouvrir la base de données ({}).",
                    db_path.display()
                ),
                e.to_string(),
            ),
        }
    }

    /// Écrit `content` dans `.engram/exports/<name>` (doc §5.6 : export CSV
    /// des données brutes ; doc §7 : export .ics des tâches — même
    /// mécanisme générique, seul le contenu diffère). Chemin fixe et
    /// prévisible — pas de sélecteur de fichier natif, même limite
    /// documentée que nexus_hub.
    fn export_file(&mut self, name: &str, content: &str) {
        let Some(root) = &self.project_root else {
            self.glados(
                "Aucun projet ouvert : rien à exporter.",
                "project_root=None",
            );
            return;
        };
        let dir = root.join(".engram").join("exports");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.glados(
                format!("Impossible de créer {}.", dir.display()),
                e.to_string(),
            );
            return;
        }
        let path = dir.join(name);
        match engram_core::atomic_write(&path, content.as_bytes()) {
            Ok(()) => self.last_export = Some(format!("Exporté : {}", path.display())),
            Err(e) => self.glados(format!("Export {} impossible.", path.display()), e),
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        if !self.glados.is_empty() {
            let mut dismiss = None;
            for (i, err) in self.glados.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(255, 46, 136), "⚠");
                    ui.label(err.user_message.as_str());
                    if ui.small_button("✕").clicked() {
                        dismiss = Some(i);
                    }
                });
            }
            if let Some(i) = dismiss {
                self.glados.remove(i);
            }
            ui.separator();
        }

        if self.project_root.is_none() {
            ui.heading("Dashboard");
            ui.add_space(6.0);
            ui.weak("Aucun projet ouvert. Ouvre un projet depuis le hub Nexus d'abord.");
            return;
        }
        let Some(db) = self.db.take() else {
            ui.heading("Dashboard");
            ui.add_space(6.0);
            ui.weak("Base de données indisponible.");
            return;
        };

        if let Some(msg) = &self.last_export {
            ui.colored_label(egui::Color32::from_rgb(120, 255, 180), msg.as_str());
            ui.separator();
        }

        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.tab == Tab::Sommeil, "🛏 Sommeil")
                .clicked()
            {
                self.tab = Tab::Sommeil;
            }
            if ui
                .selectable_label(self.tab == Tab::EtatPsy, "🧠 État psy")
                .clicked()
            {
                self.tab = Tab::EtatPsy;
            }
            if ui
                .selectable_label(self.tab == Tab::Correlations, "🔗 Corrélations")
                .clicked()
            {
                self.tab = Tab::Correlations;
            }
        });
        ui.separator();

        match self.tab {
            Tab::Sommeil => self.draw_sleep(ui, &db),
            Tab::EtatPsy => self.draw_mood(ui, &db),
            Tab::Correlations => self.draw_correlations(ui, &db),
        }

        self.db = Some(db);
    }

    /// Doc §5.6, périmètre exact de la session 7 du §10 (« corrélations
    /// médication + tâches ») : charge tâches → épuisement, vélocité,
    /// répartition par énergie, tâches bloquées, observance médication →
    /// fonctionnement. La corrélation sommeil→cognitif (même sous-section
    /// du doc) n'est PAS ici — hors du titre littéral de cette session,
    /// voir README_MODULE.md.
    fn draw_correlations(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        ui.heading("Corrélations — médication & tâches");

        let tasks = match nexus_db::list_tasks(db) {
            Ok(t) => t,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture des tâches impossible : {e}"),
                );
                return;
            }
        };

        if ui
            .button("📅 Exporter les tâches à échéance en .ics")
            .on_hover_text(
                "Doc §7 : fichier iCalendar standard, importable dans n'importe quel agenda \
                 (iCloud, Google Calendar, Nextcloud). Une tâche sans échéance n'apparaît pas.",
            )
            .clicked()
        {
            let ics = ics::tasks_to_ics(&tasks, chrono::Utc::now());
            self.export_file("taches.ics", &ics);
        }
        ui.separator();
        let mood = match nexus_db::list_mood_logs(db) {
            Ok(m) => m,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture de l'état psy impossible : {e}"),
                );
                Vec::new()
            }
        };
        let mut history = Vec::new();
        for t in &tasks {
            match nexus_db::list_task_history(db, &t.id) {
                Ok(h) => history.extend(h),
                Err(e) => {
                    self.glados(
                        format!("Historique de la tâche '{}' illisible.", t.titre),
                        e.to_string(),
                    );
                }
            }
        }

        ui.add_space(6.0);
        ui.strong("Charge tâches → épuisement");
        let load = correlations::weekly_load(&tasks, &history, &mood);
        if load.is_empty() {
            ui.weak("Référentiel insuffisant (aucune tâche ou saisie datée).");
        } else {
            let width = ui.available_width().min(700.0);
            charts::draw_weekly_load_chart(ui, egui::vec2(width, 180.0), &load);
            ui.weak("Violet = créées · vert = terminées · ligne = épuisement moyen (0-5).");
        }

        ui.add_space(10.0);
        ui.strong("Vélocité hebdomadaire (8 dernières semaines)");
        let velocity = correlations::velocity(&load, 8);
        if velocity.is_empty() {
            ui.weak("Référentiel insuffisant.");
        } else {
            let width = ui.available_width().min(700.0);
            charts::draw_velocity_chart(ui, egui::vec2(width, 120.0), &velocity);
        }

        ui.add_space(10.0);
        ui.strong("Répartition par énergie des tâches terminées");
        let (low, medium, high, none) = correlations::done_by_energy(&tasks);
        if low + medium + high + none == 0 {
            ui.weak("Aucune tâche terminée pour l'instant.");
        } else {
            charts::draw_energy_breakdown(ui, egui::vec2(280.0, 140.0), low, medium, high, none);
        }

        ui.add_space(10.0);
        ui.strong(format!(
            "Tâches bloquées depuis plus de {} jours",
            self.cfg.blocked_days_threshold
        ));
        let now = chrono::Local::now().naive_local();
        let blocked = correlations::blocked_over(&tasks, now, self.cfg.blocked_days_threshold);
        if blocked.is_empty() {
            ui.weak("Aucune.");
        } else {
            for t in &blocked {
                ui.label(format!(
                    "• {} (bloquée depuis le {})",
                    t.titre, t.updated_at
                ));
            }
        }

        ui.add_space(10.0);
        ui.strong("Observance médication → fonctionnement");
        let medications = match nexus_db::list_medications(db) {
            Ok(m) => m,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture des médicaments impossible : {e}"),
                );
                Vec::new()
            }
        };
        let doses = match nexus_db::list_med_doses(db) {
            Ok(d) => d,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture des prises impossible : {e}"),
                );
                Vec::new()
            }
        };
        let obs = correlations::med_observance(&doses, &medications, &mood);
        if obs.is_empty() {
            ui.weak("Référentiel insuffisant (aucune prise enregistrée).");
        } else {
            let width = ui.available_width().min(700.0);
            charts::draw_med_observance_chart(ui, egui::vec2(width, 160.0), &obs);
            ui.weak(
                "Violet = nombre de prises réelles par semaine · ligne = fonctionnement moyen \
                 (0-5). Pas de taux vs fréquence attendue : ce champ n'existe pas dans le \
                 registre médicaments (doc §8) — voir README_MODULE.md.",
            );
        }

        ui.add_space(10.0);
        ui.strong("Sommeil J-1 → cognitif J");
        let sleep_logs = match nexus_db::list_sleep_logs(db) {
            Ok(s) => s,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture du sommeil impossible : {e}"),
                );
                Vec::new()
            }
        };
        let sommeil_cognitif = correlations::corr_sommeil_cognition(&sleep_logs, &mood);
        if sommeil_cognitif.is_empty() {
            ui.weak("Référentiel insuffisant (aucune nuit appariée à une saisie d'état psy).");
        } else {
            let points: Vec<(f64, f64)> = sommeil_cognitif
                .iter()
                .map(|p| (p.duration_h, p.cognitif as f64))
                .collect();
            let width = ui.available_width().min(700.0);
            charts::draw_scatter(
                ui,
                egui::vec2(width, 160.0),
                &points,
                egui::Color32::from_rgb(123, 0, 255),
            );
            match correlations::r_squared(&points) {
                Some(r2) => ui.weak(format!(
                    "Axe X = durée de sommeil (h) · axe Y = cognitif (1-5) · r² = {r2:.3} \
                     ({} point(s)).",
                    points.len()
                )),
                None => ui.weak(format!(
                    "Axe X = durée de sommeil (h) · axe Y = cognitif (1-5) · r² non calculable \
                     ({} point(s), variance insuffisante).",
                    points.len()
                )),
            };
        }

        ui.add_space(10.0);
        ui.strong("Courbe empirique médication");
        let med_etat = correlations::corr_medication_etat(&doses, &mood);
        if med_etat.is_empty() {
            ui.weak(
                "Référentiel insuffisant (aucune saisie d'état psy dans les 12h suivant une \
                 prise).",
            );
        } else {
            let cognitif_points: Vec<(f64, f64)> = med_etat
                .iter()
                .map(|p| (p.delta_minutes as f64, p.cognitif as f64))
                .collect();
            let fonctionnement_points: Vec<(f64, f64)> = med_etat
                .iter()
                .map(|p| (p.delta_minutes as f64, p.fonctionnement as f64))
                .collect();
            let width = ui.available_width().min(700.0);
            ui.weak("cognitif (1-5) vs minutes post-prise :");
            charts::draw_scatter(
                ui,
                egui::vec2(width, 140.0),
                &cognitif_points,
                egui::Color32::from_rgb(255, 46, 136),
            );
            ui.weak("fonctionnement (1-5) vs minutes post-prise :");
            charts::draw_scatter(
                ui,
                egui::vec2(width, 140.0),
                &fonctionnement_points,
                egui::Color32::from_rgb(0, 200, 255),
            );
            ui.weak(format!(
                "{} point(s) brut(s) — doc §5.6 : axe Y = score cognitif/fonctionnement. Pas de \
                 courbe de tendance LOESS ici (algorithme sans précédent dans ce dépôt, voir \
                 README_MODULE.md) — les points bruts sont déjà ce que le doc prescrit sous \
                 N=20.",
                med_etat.len()
            ));
        }
    }

    fn draw_sleep(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        ui.heading("Sommeil");
        ui.horizontal(|ui| {
            ui.label("Fenêtre :");
            for w in [30, 60, 90] {
                ui.selectable_value(&mut self.sleep_window_days, w, format!("{w}j"));
            }
        });
        let all_logs = match nexus_db::list_sleep_logs(db) {
            Ok(l) => l,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture impossible : {e}"),
                );
                return;
            }
        };
        let today = chrono::Local::now().date_naive();
        let displayed = logic::nights_in_window(&all_logs, today, self.sleep_window_days);
        if displayed.len() < logic::MIN_SAMPLE_SLEEP {
            ui.weak(format!(
                "Référentiel insuffisant (N={}, minimum requis : {}).",
                displayed.len(),
                logic::MIN_SAMPLE_SLEEP
            ));
        }
        if displayed.is_empty() {
            ui.weak("Aucune donnée de sommeil sur cette fenêtre.");
        } else {
            let rolling = logic::rolling_average_series(&all_logs, &displayed);
            let width = ui.available_width().min(700.0);
            charts::draw_sleep_chart(ui, egui::vec2(width, 220.0), &displayed, &rolling);
            ui.weak("Barres = durée · ligne = moyenne glissante 28j · points = qualité (1-5).");
        }
        ui.add_space(6.0);
        if ui.button("Exporter en CSV").clicked() {
            let csv = logic::sleep_logs_to_csv(&all_logs);
            self.export_file("sommeil.csv", &csv);
        }
    }

    fn draw_mood(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        ui.heading("État psy");
        let all_logs = match nexus_db::list_mood_logs(db) {
            Ok(l) => l,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture impossible : {e}"),
                );
                return;
            }
        };
        let today = chrono::Local::now().date_naive();

        let alerts = logic::low_dimension_alerts(
            &all_logs,
            today,
            self.cfg.mood_alert_threshold,
            self.cfg.mood_alert_consecutive_days,
        );
        for a in &alerts {
            ui.colored_label(
                egui::Color32::from_rgb(255, 46, 136),
                format!(
                    "⚠ {a} < {} sur {} jours consécutifs.",
                    self.cfg.mood_alert_threshold, self.cfg.mood_alert_consecutive_days
                ),
            );
        }

        ui.columns(2, |cols| {
            let current = logic::week_average(&all_logs, today);
            let previous = logic::week_average(&all_logs, today - chrono::Duration::days(7));
            cols[0].label("Semaine courante vs précédente :");
            match current {
                Some(c) => charts::draw_week_radar(&mut cols[0], 240.0, c, previous),
                None => {
                    cols[0].weak("Référentiel insuffisant pour la semaine courante.");
                }
            }

            cols[1].label("Tendance 30 jours :");
            let trend = logic::daily_trend(&all_logs, today, 30);
            if trend.len() < 2 {
                cols[1].weak(format!(
                    "Référentiel insuffisant (N={}, minimum requis : 2).",
                    trend.len()
                ));
            } else {
                let width = cols[1].available_width().min(420.0);
                charts::draw_dimension_trend(&mut cols[1], egui::vec2(width, 220.0), &trend);
            }
        });

        ui.add_space(6.0);
        if ui.button("Exporter en CSV").clicked() {
            let csv = logic::mood_logs_to_csv(&all_logs);
            self.export_file("etat_psy.csv", &csv);
        }
    }
}

impl Module for DashboardModule {
    fn name(&self) -> &'static str {
        "dashboard"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        let mut errors = Vec::new();
        self.cfg = ctx.licorne.section("dashboard", &mut errors);
        self.sleep_window_days = self.cfg.default_sleep_window_days;
        for e in errors {
            self.glados(
                "Configuration 'dashboard' invalide : valeurs par défaut utilisées.",
                e,
            );
        }
        Ok(())
    }

    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        if self.closed {
            out.append(&mut self.pending);
            return;
        }
        let viewport_id = egui::ViewportId::from_hash_of("dashboard");
        let builder = egui::ViewportBuilder::default()
            .with_title("Hive-RBMK-mod-Tcherenkov — Dashboard")
            .with_inner_size([820.0, 640.0]);
        let mut open_palette = false;
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                self.closed = true;
                return;
            }
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                    egui::Key::P,
                )
            }) {
                open_palette = true;
            }
            egui::CentralPanel::default().show(ctx, |ui| self.draw(ui));
        });
        if open_palette {
            out.push(ModuleResponse::OpenPaletteRequested);
        }
        out.append(&mut self.pending);
    }

    fn active_viewport_count(&self) -> usize {
        if self.closed {
            0
        } else {
            1
        }
    }

    fn handle_event(&mut self, event: &CoreEvent) {
        match event {
            CoreEvent::ProjectRootUpdated(root) => {
                self.set_project_root(root.clone());
            }
            CoreEvent::OpenModuleWindowRequested(name) if name == self.name() => {
                self.closed = false;
            }
            CoreEvent::ToggleModuleWindowRequested(name) if name == self.name() => {
                self.closed = !self.closed;
            }
            _ => {}
        }
    }

    fn shutdown(&mut self) {
        self.db = None;
    }
}
