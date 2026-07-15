// ============================================================================
// modules/health/src/lib.rs — Point d'entrée du module Santé
//
// Ce que je fais : les 4 sous-vues Santé — Sommeil, État psy, Médication
// (doc §5.3), et Notes santé libres (doc §3, fichier `notes_sante.md` —
// jusqu'ici scaffoldé vide par le hub mais jamais lu/écrit par personne).
// Formulaires minimaux (boutons radio 1-5, pas de slider — friction
// minimale), écriture dans nexus.db, affichage immédiat des moyennes
// glissantes (28 jours pour le sommeil, 7 jours pour l'état psy),
// recalculées depuis la DB à chaque saisie, jamais stockées en dur. Le
// rappel de ressenti différé (3h post-prise) est activable/désactivable
// (doc §5.3 : « configurable off ») via la section "health" de Hive_RBMK.ron
// (config.rs) — le délai de 3h lui-même reste fixe, le doc ne donnant qu'un
// point de config (on/off), pas une plage (§A2, pas de donnée inventée).
//
// Comment je marche : OwnViewport (comme cockpit côté écrivain), fenêtre à
// la demande (fermée par défaut — doctrine "on n'impose rien au tiling").
// J'apprends la racine du projet actif via CoreEvent::ProjectRootUpdated
// (relayé par le core depuis nexus_hub — les modules ne se parlent jamais
// entre eux) et j'ouvre ma PROPRE connexion à nexus.db (SQLite + WAL
// supporte plusieurs connexions concurrentes au même fichier).
//
// Comment me virer : supprimer modules/health/ + retirer "health" du
// registre du binaire nexus (app_nexus/src/main.rs) + la dépendance de
// app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module), nexus_db (couche de
// données), chrono (date/heure des saisies — déjà dépendance du workspace).
// ============================================================================

mod config;
mod medication;
mod mood;
mod notes;
mod sleep;

use std::path::PathBuf;

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse};

/// Erreur destinée à l'utilisateur (doctrine §8.3) : message clair affiché
/// dans la fenêtre, séparé de la cause technique qui va au log.
struct UserError {
    user_message: String,
    #[allow(dead_code)] // conservée pour un futur panneau "détails techniques"
    technical: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Sleep,
    Mood,
    Medication,
    Notes,
}

pub struct HealthModule {
    /// Fenêtre masquée jusqu'à une commande explicite (doctrine "fenêtre à
    /// la demande").
    closed: bool,
    project_root: Option<PathBuf>,
    db: Option<nexus_db::Connection>,
    tab: Tab,
    sleep_form: sleep::SleepForm,
    mood_form: mood::MoodForm,
    medication_form: medication::MedicationForm,
    notes_state: notes::NotesState,
    cfg: config::HealthConfig,
    /// Popup « Prise » ouvert pour un médicament donné (timestamp éditable
    /// + dose modifiable — distinct de Redrop, doc §5.3 vs §6).
    dose_dialog: Option<medication::DoseForm>,
    glados: Vec<UserError>,
    pending: Vec<ModuleResponse>,
}

impl Default for HealthModule {
    fn default() -> Self {
        Self {
            closed: true,
            project_root: None,
            db: None,
            tab: Tab::default(),
            sleep_form: sleep::SleepForm::default(),
            mood_form: mood::MoodForm::default(),
            medication_form: medication::MedicationForm::default(),
            notes_state: notes::NotesState::default(),
            cfg: config::HealthConfig::default(),
            dose_dialog: None,
            glados: Vec::new(),
            pending: Vec::new(),
        }
    }
}

impl HealthModule {
    fn glados(&mut self, user_message: impl Into<String>, technical: impl Into<String>) {
        let user_message = user_message.into();
        let technical = technical.into();
        tracing::error!(target: "health", "{user_message} ({technical})");
        self.pending.push(ModuleResponse::Error {
            module: "health".into(),
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

    /// Ferme l'ancienne connexion (s'il y en a une) et ouvre celle du
    /// nouveau projet, s'il y en a un.
    fn set_project_root(&mut self, root: Option<PathBuf>) {
        self.db = None;
        self.notes_state = notes::NotesState::default();
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

    fn draw(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
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
                ui.heading("Santé");
                ui.add_space(6.0);
                ui.weak("Aucun projet ouvert. Ouvre un projet depuis le hub Nexus d'abord.");
                return;
            }
            let Some(db) = self.db.take() else {
                ui.heading("Santé");
                ui.add_space(6.0);
                ui.weak("Base de données indisponible.");
                return;
            };

            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.tab == Tab::Sleep, "🛏 Sommeil")
                    .clicked()
                {
                    self.tab = Tab::Sleep;
                }
                if ui
                    .selectable_label(self.tab == Tab::Mood, "🧠 État psy")
                    .clicked()
                {
                    self.tab = Tab::Mood;
                }
                if ui
                    .selectable_label(self.tab == Tab::Medication, "💊 Médication")
                    .clicked()
                {
                    self.tab = Tab::Medication;
                }
                if ui
                    .selectable_label(self.tab == Tab::Notes, "📝 Notes santé")
                    .clicked()
                {
                    self.tab = Tab::Notes;
                }
            });
            ui.separator();

            match self.tab {
                Tab::Sleep => self.draw_sleep(ui, &db),
                Tab::Mood => self.draw_mood(ui, &db),
                Tab::Medication => self.draw_medication(ui, &db),
                Tab::Notes => self.draw_notes(ui, &db),
            }
            self.db = Some(db);
        });
    }

    fn draw_sleep(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        ui.heading("Sommeil");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Durée (heures) :");
            ui.add(
                egui::DragValue::new(&mut self.sleep_form.duration_h)
                    .speed(0.1)
                    .range(0.0..=24.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Qualité :");
            for q in 1..=5u8 {
                ui.selectable_value(&mut self.sleep_form.quality, q, q.to_string());
            }
        });
        ui.label("Notes (optionnel) :");
        ui.text_edit_multiline(&mut self.sleep_form.notes);
        ui.add_space(6.0);
        if ui.button("Enregistrer").clicked() {
            self.save_sleep(db);
        }
        ui.separator();

        match nexus_db::list_sleep_logs(db) {
            Ok(logs) => {
                let today = chrono::Local::now().date_naive();
                let avg = sleep::rolling_average_duration(&logs, today, 28);
                match avg {
                    Some(avg) => {
                        ui.label(format!("Moyenne personnelle (28 jours) : {avg:.1} h"));
                        if let Some(dev) =
                            sleep::pct_deviation(f64::from(self.sleep_form.duration_h), Some(avg))
                        {
                            ui.label(format!("Écart avec la saisie en cours : {dev:+.0} %"));
                        }
                    }
                    None => {
                        ui.weak(
                            "Référentiel insuffisant (aucune entrée sur les 28 derniers jours).",
                        );
                    }
                }
                ui.add_space(6.0);
                ui.label(format!("{} nuit(s) enregistrée(s).", logs.len()));
            }
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture impossible : {e}"),
                );
            }
        }
    }

    fn save_sleep(&mut self, db: &nexus_db::Connection) {
        let today = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        let notes = if self.sleep_form.notes.trim().is_empty() {
            None
        } else {
            Some(self.sleep_form.notes.clone())
        };
        let entry = nexus_db::SleepLog {
            id: nexus_db::new_id(),
            date: today,
            duration_h: f64::from(self.sleep_form.duration_h),
            quality: i64::from(self.sleep_form.quality),
            notes,
        };
        if let Err(e) = nexus_db::upsert_sleep_log(db, &entry) {
            self.glados(
                "Impossible d'enregistrer la nuit de sommeil.",
                e.to_string(),
            );
        }
    }

    fn draw_mood(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        ui.heading("État psy");
        ui.add_space(6.0);
        for (label, value) in [
            ("Épuisement", &mut self.mood_form.epuisement),
            ("Cognitif", &mut self.mood_form.cognitif),
            ("Sensoriel", &mut self.mood_form.sensoriel),
            ("Masquage", &mut self.mood_form.masquage),
            ("Fonctionnement", &mut self.mood_form.fonctionnement),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!("{label} :"));
                for v in 1..=5u8 {
                    ui.selectable_value(value, v, v.to_string());
                }
            });
        }
        ui.label("Notes (optionnel) :");
        ui.text_edit_multiline(&mut self.mood_form.notes);
        ui.add_space(6.0);
        if ui.button("Enregistrer").clicked() {
            self.save_mood(db);
        }
        ui.separator();

        match nexus_db::list_mood_logs(db) {
            Ok(logs) => {
                let today = chrono::Local::now().date_naive();
                let avg = mood::rolling_average_dimensions(&logs, today, 7);
                let current = mood::Dimensions {
                    epuisement: f64::from(self.mood_form.epuisement),
                    cognitif: f64::from(self.mood_form.cognitif),
                    sensoriel: f64::from(self.mood_form.sensoriel),
                    masquage: f64::from(self.mood_form.masquage),
                    fonctionnement: f64::from(self.mood_form.fonctionnement),
                };
                mood::draw_radar(ui, 260.0, current, avg);
                if avg.is_none() {
                    ui.weak(
                        "Référentiel insuffisant (aucune entrée sur les 7 derniers jours) : \
                         radar affiché sans moyenne de comparaison.",
                    );
                }
                ui.add_space(6.0);
                ui.label(format!("{} saisie(s) enregistrée(s).", logs.len()));
            }
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture impossible : {e}"),
                );
            }
        }
    }

    fn save_mood(&mut self, db: &nexus_db::Connection) {
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let notes = if self.mood_form.notes.trim().is_empty() {
            None
        } else {
            Some(self.mood_form.notes.clone())
        };
        let entry = nexus_db::MoodLog {
            id: nexus_db::new_id(),
            logged_at: now,
            epuisement: i64::from(self.mood_form.epuisement),
            cognitif: i64::from(self.mood_form.cognitif),
            sensoriel: i64::from(self.mood_form.sensoriel),
            masquage: i64::from(self.mood_form.masquage),
            fonctionnement: i64::from(self.mood_form.fonctionnement),
            notes,
        };
        if let Err(e) = nexus_db::insert_mood_log(db, &entry) {
            self.glados("Impossible d'enregistrer l'état psy.", e.to_string());
        }
    }

    fn draw_medication(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        ui.heading("Médication");
        ui.add_space(6.0);
        ui.collapsing("Ajouter un médicament au registre", |ui| {
            ui.horizontal(|ui| {
                ui.label("Nom :");
                ui.text_edit_singleline(&mut self.medication_form.nom);
            });
            ui.horizontal(|ui| {
                ui.label("Molécule :");
                ui.text_edit_singleline(&mut self.medication_form.molecule);
            });
            ui.horizontal(|ui| {
                ui.label("Dose par défaut (mg) :");
                ui.add(
                    egui::DragValue::new(&mut self.medication_form.dose_default)
                        .speed(0.5)
                        .range(0.0..=2000.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Notes :");
                ui.text_edit_singleline(&mut self.medication_form.notes);
            });
            if ui.button("Ajouter au registre").clicked() {
                self.save_medication(db);
            }
        });
        ui.separator();

        let medications = match nexus_db::list_medications(db) {
            Ok(m) => m,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture du registre impossible : {e}"),
                );
                return;
            }
        };
        if medications.is_empty() {
            ui.weak("Aucun médicament au registre pour l'instant.");
            return;
        }
        let doses = match nexus_db::list_med_doses(db) {
            Ok(d) => d,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture des prises impossible : {e}"),
                );
                return;
            }
        };
        let now = chrono::Local::now().naive_local();
        let today = now.date();

        for med in &medications {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(&med.nom);
                    ui.weak(&med.molecule);
                    if ui.button("💊 Prise").clicked() {
                        self.dose_dialog = Some(medication::DoseForm::new(med, now));
                    }
                });
                let mine: Vec<nexus_db::MedDose> = doses
                    .iter()
                    .filter(|d| d.med_id == med.id)
                    .cloned()
                    .collect();
                let today_doses = medication::doses_on(&mine, today);
                if today_doses.is_empty() {
                    ui.weak("Aucune prise aujourd'hui.");
                } else {
                    let heures: Vec<String> = today_doses
                        .iter()
                        .filter_map(|d| d.taken_at.split('T').nth(1))
                        .map(|h| h.to_string())
                        .collect();
                    ui.label(format!("Aujourd'hui : {}", heures.join(", ")));
                }
                let week = medication::doses_in_window(&mine, today, 7);
                ui.weak(format!("Historique 7 jours : {} prise(s).", week.len()));

                for d in &week {
                    if self.cfg.ressenti_prompt_enabled && medication::needs_ressenti_prompt(d, now)
                    {
                        ui.horizontal(|ui| {
                            ui.weak(format!("Ressenti pour la prise de {} :", d.taken_at));
                            for v in 1..=5u8 {
                                if ui.button(v.to_string()).clicked() {
                                    self.save_ressenti(db, &d.id, v);
                                }
                            }
                        });
                    }
                }
            });
        }

        if let Some(dialog) = &mut self.dose_dialog {
            let mut close = false;
            let mut confirm = false;
            egui::Window::new("Enregistrer une prise")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Horodatage (YYYY-MM-DD HH:MM) :");
                        ui.text_edit_singleline(&mut dialog.timestamp_input);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Dose (mg) :");
                        ui.add(
                            egui::DragValue::new(&mut dialog.dose_mg)
                                .speed(0.5)
                                .range(0.0..=2000.0),
                        );
                    });
                    if dialog.parsed_taken_at().is_none() {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 46, 136),
                            "Horodatage illisible — corrige-le avant de confirmer.",
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Confirmer").clicked() {
                            confirm = true;
                        }
                        if ui.button("Annuler").clicked() {
                            close = true;
                        }
                    });
                });
            if confirm {
                if let Some(taken_at) = dialog.parsed_taken_at() {
                    let entry = nexus_db::MedDose {
                        id: nexus_db::new_id(),
                        med_id: dialog.med_id.clone(),
                        taken_at,
                        dose_mg: f64::from(dialog.dose_mg),
                        ressenti: None,
                        notes: None,
                    };
                    if let Err(e) = nexus_db::insert_med_dose(db, &entry) {
                        self.glados("Impossible d'enregistrer la prise.", e.to_string());
                    }
                    close = true;
                }
            }
            if close {
                self.dose_dialog = None;
            }
        }
    }

    fn save_medication(&mut self, db: &nexus_db::Connection) {
        if self.medication_form.nom.trim().is_empty() {
            self.glados("Donne un nom au médicament.", "nom vide");
            return;
        }
        let notes = if self.medication_form.notes.trim().is_empty() {
            None
        } else {
            Some(self.medication_form.notes.clone())
        };
        let entry = nexus_db::Medication {
            id: nexus_db::new_id(),
            nom: self.medication_form.nom.clone(),
            molecule: self.medication_form.molecule.clone(),
            dose_default: f64::from(self.medication_form.dose_default),
            notes,
        };
        match nexus_db::insert_medication(db, &entry) {
            Ok(()) => self.medication_form = medication::MedicationForm::default(),
            Err(e) => self.glados("Impossible d'ajouter le médicament.", e.to_string()),
        }
    }

    fn save_ressenti(&mut self, db: &nexus_db::Connection, dose_id: &str, ressenti: u8) {
        if let Err(e) = nexus_db::update_med_dose_ressenti(db, dose_id, i64::from(ressenti)) {
            self.glados("Impossible d'enregistrer le ressenti.", e.to_string());
        }
    }

    /// Doc §3 : `02_sante/notes_sante.md` (`.typst` hérité respecté), observations libres. Scaffoldé
    /// vide par le hub à la création du projet, mais jusqu'ici jamais lu ni
    /// écrit par aucun module — cette sous-vue ferme ce trou.
    fn draw_notes(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        let Some(root) = self.project_root.clone() else {
            ui.weak("Aucun projet ouvert.");
            return;
        };
        if !self.notes_state.loaded {
            match notes::load(&root) {
                Ok(content) => self.notes_state.content = content,
                Err(e) => self.glados("Lecture des notes santé impossible.", e),
            }
            self.notes_state.loaded = true;
        }
        ui.heading("Notes santé");
        ui.weak("Observations libres (02_sante/notes_sante.md) — aucune structure imposée.");
        if notes::is_legacy(&root) {
            ui.horizontal_wrapped(|ui| {
                ui.weak("Typst — secondaire.");
                if ui
                    .button("Créer une copie Markdown")
                    .on_hover_text(
                        "Crée notes_sante.md convertie (structures sûres uniquement) + \
                         un rapport. L'original .typst n'est JAMAIS modifié.",
                    )
                    .clicked()
                {
                    match notes::create_md_copy(&root, &self.notes_state.content) {
                        Ok(dst) => {
                            // file_path() résout désormais sur le .md : on
                            // recharge depuis la copie et on indexe.
                            self.notes_state.dirty = false;
                            let key = notes::fts_key(&root);
                            match std::fs::read_to_string(&dst) {
                                Ok(c) => self.notes_state.content = c,
                                Err(e) => self.glados(
                                    "Copie créée mais relecture impossible.",
                                    e.to_string(),
                                ),
                            }
                            if let Err(e) =
                                nexus_db::fts_upsert(db, &key, &self.notes_state.content)
                            {
                                self.glados(
                                    "Copie créée mais indexation impossible.",
                                    e.to_string(),
                                );
                            }
                        }
                        Err(e) => self.glados("Copie Markdown impossible.", e),
                    }
                }
            });
        }
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut self.notes_state.content)
                        .desired_rows(18)
                        .desired_width(f32::INFINITY),
                );
                if resp.changed() {
                    self.notes_state.dirty = true;
                }
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Enregistrer").clicked() {
                match notes::save(&root, &self.notes_state.content) {
                    Ok(()) => {
                        self.notes_state.dirty = false;
                        let key = notes::fts_key(&root);
                        if let Err(e) = nexus_db::fts_upsert(db, &key, &self.notes_state.content) {
                            self.glados(
                                "Impossible d'indexer les notes santé pour la recherche.",
                                e.to_string(),
                            );
                        }
                    }
                    Err(e) => self.glados("Impossible d'enregistrer les notes santé.", e),
                }
            }
            if self.notes_state.dirty {
                ui.weak("Modifications non enregistrées.");
            } else {
                ui.colored_label(egui::Color32::from_rgb(120, 255, 180), "✓ Enregistré");
            }
        });
    }
}

impl Module for HealthModule {
    fn name(&self) -> &'static str {
        "health"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        let mut errors = Vec::new();
        self.cfg = ctx.licorne.section("health", &mut errors);
        for e in errors {
            self.glados(
                "Configuration 'health' invalide : valeurs par défaut utilisées.",
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
        let viewport_id = egui::ViewportId::from_hash_of("health");
        let builder = egui::ViewportBuilder::default()
            .with_title("Hive-RBMK-mod-Tcherenkov — Santé")
            .with_inner_size([520.0, 640.0]);
        let mut open_palette = false;
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                self.closed = true;
                return;
            }
            // Redrop est accessible depuis TOUTES les fenêtres sans
            // exception (doc §6) : la palette globale s'ouvre aussi depuis
            // ici, même mécanisme que cockpit côté écrivain.
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                    egui::Key::P,
                )
            }) {
                open_palette = true;
            }
            self.draw(ctx);
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
