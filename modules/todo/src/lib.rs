// ============================================================================
// modules/todo/src/lib.rs — Point d'entrée du module Todo
//
// Ce que je fais : kanban orienté énergie disponible, pas priorité abstraite
// (doc §5.4). 6 colonnes (backlog/today/doing/blocked/done/dropped), filtre
// "maintenant" (croise l'énergie des tâches avec le cognitif le plus récent
// de mood_log), filtre contexte, signalement des tâches sans durée, sous-
// tâches (1 niveau — sélecteur "Sous-tâche de" limité aux tâches de premier
// niveau, sur la liste COMPLÈTE non filtrée), récurrence (3 règles connues
// + champ libre pour une règle custom, doc §5.4 — non régénérée seule si
// non reconnue, §A2) avec régénération automatique à `done`.
//
// Comment je marche : OwnViewport, fenêtre à la demande. J'apprends la
// racine du projet actif via CoreEvent::ProjectRootUpdated (même mécanisme
// que health/journal) et j'ouvre ma PROPRE connexion à nexus.db.
//
// Ce que je ne fais pas (doc §5.4, explicitement hors périmètre) : pas de
// drag-and-drop (changement de statut via menu déroulant), pas de partage,
// pas de Gantt.
//
// Comment me virer : supprimer modules/todo/ + retirer "todo" du registre
// du binaire nexus (app_nexus/src/main.rs) + la dépendance de
// app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module), nexus_db (couche de
// données), chrono (dates/récurrence).
// ============================================================================

mod config;
mod logic;

use std::path::PathBuf;

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse};

struct UserError {
    user_message: String,
    #[allow(dead_code)]
    technical: String,
}

/// État du formulaire « nouvelle tâche » (brouillon, pas encore enregistré).
struct NewTaskForm {
    titre: String,
    statut: String,
    energie: String,
    contexte: String,
    duree_min: String,
    echeance: String,
    notes: String,
    parent_id: Option<String>,
    recurrence_rule: String,
}

impl Default for NewTaskForm {
    fn default() -> Self {
        Self {
            titre: String::new(),
            statut: "backlog".into(),
            energie: String::new(),
            contexte: String::new(),
            duree_min: String::new(),
            echeance: String::new(),
            notes: String::new(),
            parent_id: None,
            recurrence_rule: String::new(),
        }
    }
}

pub struct TodoModule {
    closed: bool,
    project_root: Option<PathBuf>,
    db: Option<nexus_db::Connection>,
    show_new_task: bool,
    new_task_form: NewTaskForm,
    filter_maintenant: bool,
    filter_contexte: Option<String>,
    glados: Vec<UserError>,
    pending: Vec<ModuleResponse>,
}

impl Default for TodoModule {
    fn default() -> Self {
        Self {
            closed: true,
            project_root: None,
            db: None,
            show_new_task: false,
            new_task_form: NewTaskForm::default(),
            filter_maintenant: false,
            filter_contexte: None,
            glados: Vec::new(),
            pending: Vec::new(),
        }
    }
}

const CONTEXTES: &[&str] = &["maison", "ordi", "téléphone", "dehors", "n'importe"];
const ENERGIES: &[&str] = &["low", "medium", "high"];
const RECURRENCES: &[&str] = &["quotidien", "hebdo", "mensuel"];

impl TodoModule {
    fn glados(&mut self, user_message: impl Into<String>, technical: impl Into<String>) {
        let user_message = user_message.into();
        let technical = technical.into();
        tracing::error!(target: "todo", "{user_message} ({technical})");
        self.pending.push(ModuleResponse::Error {
            module: "todo".into(),
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

    fn create_task(&mut self, db: &nexus_db::Connection) {
        if self.new_task_form.titre.trim().is_empty() {
            self.glados("Donne un titre à la tâche.", "titre vide");
            return;
        }
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let duree_min: Option<i64> = self.new_task_form.duree_min.trim().parse().ok();
        let task = nexus_db::Task {
            id: nexus_db::new_id(),
            parent_id: self.new_task_form.parent_id.clone(),
            titre: self.new_task_form.titre.clone(),
            statut: self.new_task_form.statut.clone(),
            priorite: None,
            energie: non_empty(&self.new_task_form.energie),
            contexte: non_empty(&self.new_task_form.contexte),
            duree_min,
            echeance: non_empty(&self.new_task_form.echeance),
            created_at: now.clone(),
            updated_at: now,
            notes: non_empty(&self.new_task_form.notes),
        };
        let task_id = task.id.clone();
        if let Err(e) = nexus_db::insert_task(db, &task) {
            self.glados("Impossible de créer la tâche.", e.to_string());
            return;
        }
        if !self.new_task_form.recurrence_rule.is_empty() {
            if let Some(echeance) = &task.echeance {
                if let Ok(d) = chrono::NaiveDate::parse_from_str(echeance, "%Y-%m-%d") {
                    if let Some(next) =
                        logic::next_occurrence(&self.new_task_form.recurrence_rule, d)
                    {
                        let rec = nexus_db::TaskRecurrence {
                            id: nexus_db::new_id(),
                            task_id,
                            regle: self.new_task_form.recurrence_rule.clone(),
                            prochaine_date: next.format("%Y-%m-%d").to_string(),
                        };
                        if let Err(e) = nexus_db::insert_task_recurrence(db, &rec) {
                            self.glados("Impossible d'enregistrer la récurrence.", e.to_string());
                        }
                    }
                }
            }
        }
        self.new_task_form = NewTaskForm::default();
        self.show_new_task = false;
    }

    /// Change le statut d'une tâche. Si elle passe en `done` et porte une
    /// règle de récurrence, régénère automatiquement l'occurrence suivante
    /// (doc §5.4) — le titre/l'énergie/le contexte/la durée sont repris à
    /// l'identique, seule l'échéance avance.
    fn change_status(&mut self, db: &mut nexus_db::Connection, task_id: &str, new_statut: &str) {
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let updated = match nexus_db::transition_task_status(db, task_id, new_statut, &now) {
            Ok(t) => t,
            Err(e) => {
                self.glados("Impossible de changer le statut.", e.to_string());
                return;
            }
        };
        if new_statut != "done" {
            return;
        }
        let Ok(Some(rec)) = nexus_db::get_task_recurrence(db, task_id) else {
            return;
        };
        let from = updated
            .echeance
            .as_deref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        let Some(next) = logic::next_occurrence(&rec.regle, from) else {
            self.glados(
                "Récurrence non régénérée automatiquement.",
                format!("règle '{}' non reconnue", rec.regle),
            );
            return;
        };
        let new_task = nexus_db::Task {
            id: nexus_db::new_id(),
            parent_id: updated.parent_id.clone(),
            titre: updated.titre.clone(),
            statut: "backlog".into(),
            priorite: updated.priorite,
            energie: updated.energie.clone(),
            contexte: updated.contexte.clone(),
            duree_min: updated.duree_min,
            echeance: Some(next.format("%Y-%m-%d").to_string()),
            created_at: now.clone(),
            updated_at: now,
            notes: updated.notes.clone(),
        };
        let new_task_id = new_task.id.clone();
        if let Err(e) = nexus_db::insert_task(db, &new_task) {
            self.glados(
                "Régénération de la tâche récurrente impossible.",
                e.to_string(),
            );
            return;
        }
        let new_rec = nexus_db::TaskRecurrence {
            id: nexus_db::new_id(),
            task_id: new_task_id,
            regle: rec.regle,
            prochaine_date: next.format("%Y-%m-%d").to_string(),
        };
        if let Err(e) = nexus_db::insert_task_recurrence(db, &new_rec) {
            self.glados(
                "Régénération : règle de récurrence non reportée.",
                e.to_string(),
            );
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
            ui.heading("Todo");
            ui.add_space(6.0);
            ui.weak("Aucun projet ouvert. Ouvre un projet depuis le hub Nexus d'abord.");
            return;
        }
        let Some(mut db) = self.db.take() else {
            ui.heading("Todo");
            ui.add_space(6.0);
            ui.weak("Base de données indisponible.");
            return;
        };

        ui.horizontal(|ui| {
            if ui.button("+ Nouvelle tâche").clicked() {
                self.show_new_task = true;
            }
            ui.checkbox(&mut self.filter_maintenant, "Filtre « maintenant »")
                .on_hover_text(
                    "Ne montre que les tâches dont l'énergie correspond à ton état \
                     cognitif le plus récent (doc §5.4).",
                );
            ui.separator();
            ui.label("Contexte :");
            for c in CONTEXTES {
                let selected = self.filter_contexte.as_deref() == Some(*c);
                if ui.selectable_label(selected, *c).clicked() {
                    self.filter_contexte = if selected {
                        None
                    } else {
                        Some((*c).to_string())
                    };
                }
            }
        });
        ui.separator();

        let tasks = match nexus_db::list_tasks(&db) {
            Ok(t) => t,
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture des tâches impossible : {e}"),
                );
                self.db = Some(db);
                return;
            }
        };

        let allowed_energy: Option<&'static [&'static str]> = if self.filter_maintenant {
            match nexus_db::list_mood_logs(&db) {
                Ok(logs) => logs
                    .first()
                    .map(|m| logic::allowed_energy_levels(m.cognitif)),
                Err(e) => {
                    self.glados("Filtre « maintenant » indisponible.", e.to_string());
                    None
                }
            }
        } else {
            None
        };

        let visible: Vec<&nexus_db::Task> = tasks
            .iter()
            .filter(|t| {
                logic::matches_context(t.contexte.as_deref(), self.filter_contexte.as_deref())
            })
            .filter(|t| match (allowed_energy, &t.energie) {
                (Some(allowed), Some(e)) => allowed.contains(&e.as_str()),
                (Some(_), None) => false,
                (None, _) => true,
            })
            .collect();
        let (top_level, children) = logic::group_by_parent(&visible);

        let mut status_change: Option<(String, String)> = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for column in logic::COLUMNS {
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        ui.strong(logic::column_label(column));
                        ui.separator();
                        for t in top_level.iter().filter(|t| t.statut == *column) {
                            draw_card(ui, t, &children, &mut status_change);
                        }
                    });
                    ui.separator();
                }
            });
        });

        if let Some((task_id, new_statut)) = status_change {
            self.change_status(&mut db, &task_id, &new_statut);
        }

        if self.show_new_task {
            let mut close = false;
            let mut confirm = false;
            // Candidats pour « Sous-tâche de » : tâches de premier niveau
            // SEULES (parent_id absent), sur la liste COMPLÈTE (pas
            // `visible`, qui est filtrée par contexte/énergie — un parent
            // masqué par le filtre courant doit rester sélectionnable). Un
            // seul niveau de imbrication (doc §5.4) : une sous-tâche ne peut
            // donc jamais apparaître ici.
            let parent_candidates: Vec<&nexus_db::Task> =
                tasks.iter().filter(|t| t.parent_id.is_none()).collect();
            egui::Window::new("Nouvelle tâche")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Titre :");
                        ui.text_edit_singleline(&mut self.new_task_form.titre);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Sous-tâche de :");
                        let selected_label = self
                            .new_task_form
                            .parent_id
                            .as_deref()
                            .and_then(|pid| parent_candidates.iter().find(|t| t.id == pid))
                            .map_or("aucune (tâche de premier niveau)", |t| t.titre.as_str());
                        egui::ComboBox::from_id_salt("new_task_parent")
                            .selected_text(selected_label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.new_task_form.parent_id,
                                    None,
                                    "aucune (tâche de premier niveau)",
                                );
                                for t in &parent_candidates {
                                    ui.selectable_value(
                                        &mut self.new_task_form.parent_id,
                                        Some(t.id.clone()),
                                        &t.titre,
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Énergie :");
                        for e in ENERGIES {
                            ui.selectable_value(
                                &mut self.new_task_form.energie,
                                (*e).to_string(),
                                *e,
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Contexte :");
                        for c in CONTEXTES {
                            ui.selectable_value(
                                &mut self.new_task_form.contexte,
                                (*c).to_string(),
                                *c,
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Durée estimée (minutes) :");
                        ui.text_edit_singleline(&mut self.new_task_form.duree_min);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Échéance (YYYY-MM-DD, optionnel) :");
                        ui.text_edit_singleline(&mut self.new_task_form.echeance);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Récurrence :");
                        for r in RECURRENCES {
                            ui.selectable_value(
                                &mut self.new_task_form.recurrence_rule,
                                (*r).to_string(),
                                *r,
                            );
                        }
                        ui.selectable_value(
                            &mut self.new_task_form.recurrence_rule,
                            String::new(),
                            "aucune",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("…ou règle personnalisée (doc §5.4 « règle custom ») :");
                        ui.text_edit_singleline(&mut self.new_task_form.recurrence_rule);
                    });
                    if !self.new_task_form.recurrence_rule.is_empty()
                        && !RECURRENCES.contains(&self.new_task_form.recurrence_rule.as_str())
                    {
                        ui.weak(
                            "Règle non reconnue automatiquement : enregistrée telle quelle, \
                             mais la régénération à l'échéance ne se déclenchera pas seule \
                             (§A2 — aucune syntaxe de récurrence personnalisée inventée).",
                        );
                    }
                    if !self.new_task_form.recurrence_rule.is_empty()
                        && self.new_task_form.echeance.trim().is_empty()
                    {
                        ui.weak(
                            "Une récurrence nécessite une échéance (date de référence pour la \
                             prochaine occurrence).",
                        );
                    }
                    ui.label("Notes :");
                    ui.text_edit_multiline(&mut self.new_task_form.notes);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Créer").clicked() {
                            confirm = true;
                        }
                        if ui.button("Annuler").clicked() {
                            close = true;
                        }
                    });
                });
            if confirm {
                self.create_task(&db);
                close = true;
            }
            if close {
                self.show_new_task = false;
            }
        }

        self.db = Some(db);
    }
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn draw_card(
    ui: &mut egui::Ui,
    t: &nexus_db::Task,
    children: &std::collections::HashMap<String, Vec<&nexus_db::Task>>,
    status_change: &mut Option<(String, String)>,
) {
    ui.group(|ui| {
        let missing_duration = logic::duration_missing(t.duree_min);
        if missing_duration {
            ui.colored_label(egui::Color32::from_rgb(255, 200, 0), "⚠ pas de durée");
        }
        ui.strong(&t.titre);
        ui.horizontal_wrapped(|ui| {
            if let Some(e) = &t.energie {
                ui.weak(format!("⚡{e}"));
            }
            if let Some(c) = &t.contexte {
                ui.weak(format!("📍{c}"));
            }
            if let Some(d) = t.duree_min {
                ui.weak(format!("{d} min"));
            }
        });
        egui::ComboBox::from_id_salt(&t.id)
            .selected_text("Déplacer vers…")
            .show_ui(ui, |ui| {
                for column in logic::COLUMNS {
                    if *column != t.statut && ui.selectable_label(false, *column).clicked() {
                        *status_change = Some((t.id.clone(), (*column).to_string()));
                    }
                }
            });
        if let Some(subs) = children.get(&t.id) {
            for sub in subs {
                ui.indent(&sub.id, |ui| {
                    ui.weak(format!("↳ {}", sub.titre));
                });
            }
        }
    });
}

impl Module for TodoModule {
    fn name(&self) -> &'static str {
        "todo"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        let mut errors = Vec::new();
        let cfg: config::TodoConfig = ctx.licorne.section("todo", &mut errors);
        self.filter_maintenant = cfg.default_filter_maintenant;
        self.filter_contexte = non_empty(&cfg.default_filter_contexte);
        for e in errors {
            self.glados(
                "Configuration 'todo' invalide : valeurs par défaut utilisées.",
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
        let viewport_id = egui::ViewportId::from_hash_of("todo");
        let builder = egui::ViewportBuilder::default()
            .with_title("Hive-RBMK-mod-Tcherenkov — Todo")
            .with_inner_size([900.0, 620.0]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_task_avec_parent_id_ecrit_bien_la_sous_tache() {
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = TodoModule::default();
        m.new_task_form.titre = "Parent".to_string();
        m.create_task(&db);
        let parent_id = nexus_db::list_tasks(&db).expect("list")[0].id.clone();

        m.new_task_form.titre = "Enfant".to_string();
        m.new_task_form.parent_id = Some(parent_id.clone());
        m.create_task(&db);

        let tasks = nexus_db::list_tasks(&db).expect("list");
        let enfant = tasks.iter().find(|t| t.titre == "Enfant").expect("enfant");
        assert_eq!(enfant.parent_id.as_deref(), Some(parent_id.as_str()));
    }

    #[test]
    fn create_task_reinitialise_le_formulaire_dont_le_parent_id() {
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = TodoModule::default();
        m.new_task_form.titre = "Parent".to_string();
        m.create_task(&db);
        let parent_id = nexus_db::list_tasks(&db).expect("list")[0].id.clone();

        m.new_task_form.titre = "Enfant".to_string();
        m.new_task_form.parent_id = Some(parent_id);
        m.create_task(&db);
        assert!(m.new_task_form.parent_id.is_none());
    }

    #[test]
    fn create_task_avec_parent_id_inexistant_echoue_sans_vider_le_formulaire() {
        // `parent_id` est une FK réelle (doc §8) : un id qui ne correspond à
        // aucune tâche doit échouer proprement, pas être inséré en silence
        // — et le formulaire de l'utilisateur ne doit pas être perdu sur un
        // échec (il pourrait vouloir juste changer le parent et réessayer).
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = TodoModule::default();
        m.new_task_form.titre = "Orpheline".to_string();
        m.new_task_form.parent_id = Some("id-inexistant".to_string());
        m.create_task(&db);
        assert!(nexus_db::list_tasks(&db).expect("list").is_empty());
        assert_eq!(m.new_task_form.titre, "Orpheline");
    }
}
