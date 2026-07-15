// ============================================================================
// modules/journal/src/lib.rs — Point d'entrée du module Journal
//
// Ce que je fais : entrée quotidienne (doc §5.2). J'ouvre automatiquement
// 01_journal/YYYY/YYYY-MM-DD.typst du jour, je le crée depuis un template
// par défaut s'il n'existe pas, j'affiche les 7 dernières entrées en
// sidebar (cliquables), et je synchronise journal_entries (word_count, tags
// extraits du frontmatter) ET l'index plein-texte fts_content (doc §5.1/§8 :
// le hub cherche dans ce que les modules indexent) à chaque sauvegarde.
//
// Comment je marche : OwnViewport, fenêtre à la demande. J'apprends la
// racine du projet actif via CoreEvent::ProjectRootUpdated (même mécanisme
// que health) et j'ouvre ma PROPRE connexion à nexus.db.
//
// Écart honnête au doc (§5.2 « éditeur identique à Hive ») : la zone de
// texte est un egui::TextEdit::multiline standard, PAS le moteur ropey +
// coloration Typst + wikilinks-cliquables du module editor de l'app
// écrivain. Cause vérifiée (pas supposée) : editor::EditorWindow (la struct
// qui assemble buffer+coloration+wikilinks) a un constructeur PRIVÉ
// (`fn new`, pas `pub fn new`) et plusieurs champs privés — Rust interdit
// la construction par littéral dès qu'un seul champ est privé, donc
// EditorWindow est structurellement irréutilisable tel quel depuis ce
// crate. Reconstruire l'équivalent à la main en assemblant les briques
// publiques (buffer::open_standalone, highlight::HighlightCache,
// viewport::show_text_area — ~800 lignes, le rendu le plus complexe du
// dépôt) est faisable mais représente une unité de travail à part entière,
// et ne serait pas vérifiable visuellement dans cet environnement headless.
// Voir README_MODULE.md « Écart documenté ».
//
// Comment me virer : supprimer modules/journal/ + retirer "journal" du
// registre du binaire nexus (app_nexus/src/main.rs) + la dépendance de
// app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module + atomic_write), nexus_db
// (couche de données), chrono (dates).
// ============================================================================

mod config;
mod entry;

use std::path::PathBuf;

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse};

/// Erreur destinée à l'utilisateur (doctrine §8.3).
struct UserError {
    user_message: String,
    #[allow(dead_code)]
    technical: String,
}

pub struct JournalModule {
    closed: bool,
    project_root: Option<PathBuf>,
    db: Option<nexus_db::Connection>,
    current_date: chrono::NaiveDate,
    current_path: Option<PathBuf>,
    content: String,
    loaded: bool,
    /// Gabarit de création (section "journal" de Hive_RBMK.ron, doc §5.2).
    /// Chargé une fois à `init()`, valeur par défaut tant qu'aucun projet
    /// n'a déclenché `init()` (ne peut pas arriver en pratique : le core
    /// appelle toujours `init()` avant `update()`).
    template: String,
    glados: Vec<UserError>,
    pending: Vec<ModuleResponse>,
}

impl Default for JournalModule {
    fn default() -> Self {
        Self {
            closed: true,
            project_root: None,
            db: None,
            current_date: chrono::Local::now().date_naive(),
            current_path: None,
            content: String::new(),
            loaded: false,
            template: entry::DEFAULT_TEMPLATE.to_string(),
            glados: Vec::new(),
            pending: Vec::new(),
        }
    }
}

impl JournalModule {
    fn glados(&mut self, user_message: impl Into<String>, technical: impl Into<String>) {
        let user_message = user_message.into();
        let technical = technical.into();
        tracing::error!(target: "journal", "{user_message} ({technical})");
        self.pending.push(ModuleResponse::Error {
            module: "journal".into(),
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
        self.loaded = false;
        self.current_path = None;
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

    /// Charge (en créant depuis le template au besoin) l'entrée de `date`.
    /// Sauve d'abord l'entrée en cours si elle diffère (jamais de perte
    /// silencieuse en changeant de jour).
    fn switch_to_date(
        &mut self,
        db: &nexus_db::Connection,
        root: &std::path::Path,
        date: chrono::NaiveDate,
    ) {
        if self.loaded && self.current_date != date {
            self.save_current(db);
        }
        let path = entry::path_for(root, date);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    self.glados(
                        format!("Impossible de créer {}.", parent.display()),
                        e.to_string(),
                    );
                    return;
                }
            }
            let template = entry::render_template(&self.template, date);
            if let Err(e) = engram_core::atomic_write(&path, template.as_bytes()) {
                self.glados(format!("Impossible de créer {}.", path.display()), e);
                return;
            }
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.content = content;
                self.current_path = Some(path);
                self.current_date = date;
                self.loaded = true;
            }
            Err(e) => self.glados(
                format!("Impossible de lire {}.", path.display()),
                e.to_string(),
            ),
        }
    }

    fn ensure_today_loaded(&mut self, db: &nexus_db::Connection, root: &std::path::Path) {
        let today = chrono::Local::now().date_naive();
        if self.loaded && self.current_date == today {
            return;
        }
        self.switch_to_date(db, root, today);
    }

    /// Écrit le fichier courant sur disque (atomique) et synchronise
    /// `journal_entries` (word_count, tags — jamais les métadonnées dans le
    /// comptage). No-op silencieux si rien n'est chargé (rien à sauver).
    fn save_current(&mut self, db: &nexus_db::Connection) {
        let Some(path) = self.current_path.clone() else {
            return;
        };
        if let Err(e) = engram_core::atomic_write(&path, self.content.as_bytes()) {
            self.glados(format!("Impossible d'enregistrer {}.", path.display()), e);
            return;
        }
        let (frontmatter, body) = entry::split_frontmatter(&self.content);
        let tags = entry::parse_tags(frontmatter);
        let word_count = entry::word_count(body);
        let file_path = self
            .project_root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        let record = nexus_db::JournalEntry {
            id: nexus_db::new_id(),
            date: self.current_date.format("%Y-%m-%d").to_string(),
            file_path,
            word_count,
            tags,
        };
        // Les deux appels DB sont faits AVANT toute mutation de `self`
        // (glados) : `body` emprunte `self.content` et est utilisé par
        // `fts_upsert`, donc `&mut self` ne peut intervenir qu'après sa
        // dernière utilisation (NLL).
        let upsert_result = nexus_db::upsert_journal_entry(db, &record);
        let fts_result = nexus_db::fts_upsert(db, &record.file_path, body);
        if let Err(e) = upsert_result {
            self.glados(
                "Impossible de synchroniser l'entrée journal.",
                e.to_string(),
            );
        }
        if let Err(e) = fts_result {
            self.glados(
                "Impossible d'indexer l'entrée journal pour la recherche.",
                e.to_string(),
            );
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
                ui.heading("Journal");
                ui.add_space(6.0);
                ui.weak("Aucun projet ouvert. Ouvre un projet depuis le hub Nexus d'abord.");
                return;
            }
            let (Some(db), Some(root)) = (self.db.take(), self.project_root.clone()) else {
                ui.heading("Journal");
                ui.add_space(6.0);
                ui.weak("Base de données indisponible.");
                return;
            };
            self.ensure_today_loaded(&db, &root);

            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(160.0);
                    ui.strong("7 derniers jours");
                    ui.separator();
                    match nexus_db::list_journal_entries(&db) {
                        Ok(entries) => {
                            let mut to_open = None;
                            for e in entries.iter().take(7) {
                                let label =
                                    if e.date == self.current_date.format("%Y-%m-%d").to_string() {
                                        egui::RichText::new(&e.date).strong()
                                    } else {
                                        egui::RichText::new(&e.date)
                                    };
                                if ui.selectable_label(false, label).clicked() {
                                    if let Ok(d) =
                                        chrono::NaiveDate::parse_from_str(&e.date, "%Y-%m-%d")
                                    {
                                        to_open = Some(d);
                                    }
                                }
                                ui.weak(format!("{} mot(s)", e.word_count));
                            }
                            if let Some(d) = to_open {
                                self.switch_to_date(&db, &root, d);
                            }
                        }
                        Err(e) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(255, 46, 136),
                                format!("Lecture impossible : {e}"),
                            );
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.heading(format!(
                        "Journal — {}",
                        self.current_date.format("%Y-%m-%d")
                    ));
                    ui.weak(
                        self.current_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    );
                    ui.add_space(6.0);
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.content)
                            .desired_width(f32::INFINITY)
                            .desired_rows(24),
                    );
                    if response.lost_focus() {
                        self.save_current(&db);
                    }
                });
            });

            self.db = Some(db);
        });
    }
}

impl Module for JournalModule {
    fn name(&self) -> &'static str {
        "journal"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        let mut errors = Vec::new();
        let cfg: config::JournalConfig = ctx.licorne.section("journal", &mut errors);
        self.template = cfg.template;
        for e in errors {
            self.glados(
                "Configuration 'journal' invalide : gabarit par défaut utilisé.",
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
        let viewport_id = egui::ViewportId::from_hash_of("journal");
        let builder = egui::ViewportBuilder::default()
            .with_title("Hive-RBMK-mod-Tcherenkov — Journal")
            .with_inner_size([760.0, 620.0]);
        let mut open_palette = false;
        let mut close_requested = false;
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                close_requested = true;
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
            self.draw(ctx);
        });
        if close_requested {
            if let Some(db) = self.db.take() {
                self.save_current(&db);
                self.db = Some(db);
            }
            self.closed = true;
        }
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
        if let Some(db) = self.db.take() {
            self.save_current(&db);
        }
        self.db = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_current_indexe_le_corps_pour_la_recherche_plein_texte() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = JournalModule {
            project_root: Some(root.to_path_buf()),
            ..JournalModule::default()
        };
        let today = chrono::Local::now().date_naive();
        m.switch_to_date(&db, root, today);
        m.content
            .push_str("\n\nBrouillard cognitif persistant ce matin.");
        m.save_current(&db);
        let hits = nexus_db::fts_search(&db, "brouillard", 10).expect("search");
        assert_eq!(hits.len(), 1);
    }
}
