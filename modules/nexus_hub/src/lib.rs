// ============================================================================
// modules/nexus_hub/src/lib.rs — Point d'entrée du module Hub Nexus
//
// Ce que je fais : j'implémente le trait Module du core pour la fenêtre
// principale de Nexus (doc §5.1). Ouverture/création d'un projet Nexus
// (arborescence doc §3, chemin saisi au clavier — pas de sélecteur de
// dossier natif, cf. README_MODULE.md « limites »), initialisation de
// nexus.db (.engram/nexus.db) via nexus_db::open_db, résumé du jour
// (journal écrit ? état psy loggé ? tâches "today" ?) qui REMPLACE les
// stats narratives de Hive (doc §5.1 : « pas de stats narratives — à la
// place : résumé du jour »), recherche plein-texte (fts_search), arbre de
// navigation sur la structure du projet, watcher notify sur les fichiers
// externes, bouton Redrop visible.
//
// Comment je marche : EmbeddedInCore, comme file_tree côté écrivain — je ne
// possède pas de fenêtre OS propre, je me dessine dans un panel fourni par
// le binaire Hive_RBMK_Tcherenkov. Je ne parle au core QUE via
// ModuleResponse (règle d'étanchéité §7 de la doctrine). Le bouton Redrop
// du hub pousse un
// ModuleResponse::OpenPaletteRequested — je ne sais pas ouvrir le popup
// Redrop moi-même (il vit dans app_nexus, le shell, doc §2 : « Redrop n'est
// pas un module »), donc je ne fais que déclencher la palette qui, elle,
// sait le faire (mirroir exact du hotkey Ctrl+Shift+P déjà câblé ailleurs).
//
// Comment me virer : supprimer modules/nexus_hub/ + retirer "nexus_hub" du
// registre du binaire nexus (app_nexus/src/main.rs) + la dépendance de
// app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module), nexus_db (couche de
// données — pas un module au sens du trait Module), chrono (dates, déjà
// dans le workspace), notify (watcher fichiers, déjà dans le workspace via
// editor/file_tree).
// ============================================================================

mod project;
mod summary;
mod tree;
mod watcher;

use std::path::PathBuf;

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse, RenderMode};

/// Erreur destinée à l'utilisateur (doctrine §8.3) : message clair affiché
/// dans la fenêtre, séparé de la cause technique qui va au log.
struct UserError {
    user_message: String,
    #[allow(dead_code)] // conservée pour un futur panneau "détails techniques"
    technical: String,
}

/// Un projet Nexus ouvert : racine + connexion à nexus.db.
struct OpenProject {
    root: PathBuf,
    db: nexus_db::Connection,
}

/// Le module Hub Nexus.
#[derive(Default)]
pub struct NexusHubModule {
    #[allow(dead_code)] // conservé pour les futurs modules qui en auront besoin (thème, dirs)
    ctx: Option<CoreContext>,
    proj: Option<OpenProject>,
    path_input: String,
    /// Recherche plein-texte (doc §5.1). Requêtée à chaque frame quand non
    /// vide — même philosophie que les moyennes glissantes de health/dashboard
    /// (« recalculé depuis la DB, jamais mis en cache à la main »).
    search_query: String,
    /// Watcher notify (doc §5.1) sur le projet ouvert. `None` sans projet.
    watcher: Option<watcher::ProjectWatcher>,
    /// `true` si le watcher a signalé un changement externe non encore
    /// acquitté par l'utilisateur (bannière dismissable, PAS une erreur).
    external_change_notice: bool,
    /// Messages d'erreur visibles (max 4, dismissables) — même règle GLaDOS
    /// que file_tree : jamais d'erreur silencieuse.
    glados: Vec<UserError>,
    pending: Vec<ModuleResponse>,
}

impl NexusHubModule {
    fn glados(&mut self, user_message: impl Into<String>, technical: impl Into<String>) {
        let user_message = user_message.into();
        let technical = technical.into();
        tracing::error!(target: "nexus_hub", "{user_message} ({technical})");
        self.pending.push(ModuleResponse::Error {
            module: "nexus_hub".into(),
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

    /// Ouvre (`create = false`) ou crée puis ouvre (`create = true`) le
    /// projet dont le chemin est dans `path_input`.
    fn open_or_create(&mut self, create: bool, egui_ctx: &egui::Context) {
        let raw = self.path_input.trim();
        if raw.is_empty() {
            self.glados("Indique un chemin de projet.", "path_input vide");
            return;
        }
        let candidate = PathBuf::from(raw);
        if create {
            if let Err(e) = project::scaffold(&candidate) {
                self.glados("Impossible de créer la structure du projet.", e);
                return;
            }
        }
        let root = match std::fs::canonicalize(&candidate) {
            Ok(r) => r,
            Err(e) => {
                self.glados(
                    format!("Dossier introuvable : '{}'.", candidate.display()),
                    e.to_string(),
                );
                return;
            }
        };
        if !create && !project::is_nexus_project(&root) {
            self.glados(
                "Ce dossier n'est pas encore un projet Nexus.",
                "pas de .engram/ — utilise « Nouveau » pour créer la structure",
            );
            return;
        }
        let db_path = root.join(".engram").join("nexus.db");
        let db = match nexus_db::open_db(&db_path) {
            Ok(c) => c,
            Err(e) => {
                self.glados(
                    format!(
                        "Impossible d'ouvrir la base de données ({}).",
                        db_path.display()
                    ),
                    e.to_string(),
                );
                return;
            }
        };
        self.watcher = match watcher::ProjectWatcher::start(&root, egui_ctx.clone()) {
            Ok(w) => Some(w),
            Err(e) => {
                self.glados("Surveillance des fichiers externes indisponible.", e);
                None
            }
        };
        self.external_change_notice = false;
        self.pending
            .push(ModuleResponse::PublishProjectRoot(Some(root.clone())));
        self.proj = Some(OpenProject { root, db });
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

        match &self.proj {
            None => {
                ui.heading("Hive-RBMK-mod-Tcherenkov — Nexus");
                ui.add_space(6.0);
                ui.label("Chemin du projet Nexus :");
                ui.text_edit_singleline(&mut self.path_input);
                ui.horizontal(|ui| {
                    if ui.button("📂 Ouvrir").clicked() {
                        self.open_or_create(false, ui.ctx());
                    }
                    if ui.button("✨ Nouveau (créer la structure)").clicked() {
                        self.open_or_create(true, ui.ctx());
                    }
                });
                ui.add_space(6.0);
                ui.weak(
                    "Pas de sélecteur de dossier natif pour cette fondation : \
                     colle le chemin complet du dossier.",
                );
            }
            Some(p) => {
                ui.heading("Projet Nexus");
                ui.label(format!("Racine : {}", p.root.display()));
                if self.external_change_notice {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(120, 200, 255), "ℹ");
                        ui.label(
                            "Des fichiers ont été modifiés en dehors de l'application \
                             (watcher notify).",
                        );
                        if ui.small_button("✕").clicked() {
                            self.external_change_notice = false;
                        }
                    });
                }

                // Toutes les lectures via `p` (donc via `self.proj` emprunté
                // en lecture) sont regroupées ICI, avant toute mutation de
                // `self` (glados) — `p` n'est plus utilisé après ce bloc,
                // ce que NLL exige pour autoriser `&mut self` plus bas.
                let today = chrono::Local::now().date_naive();
                let date_str = today.format("%Y-%m-%d").to_string();
                let mut read_errs: Vec<(String, String)> = Vec::new();
                let journal_today = match nexus_db::get_journal_entry_by_date(&p.db, &date_str) {
                    Ok(j) => j,
                    Err(e) => {
                        read_errs.push((
                            "Lecture du journal du jour impossible.".into(),
                            e.to_string(),
                        ));
                        None
                    }
                };
                let mood_logs = match nexus_db::list_mood_logs(&p.db) {
                    Ok(m) => m,
                    Err(e) => {
                        read_errs.push(("Lecture de l'état psy impossible.".into(), e.to_string()));
                        Vec::new()
                    }
                };
                let tasks = match nexus_db::list_tasks(&p.db) {
                    Ok(t) => t,
                    Err(e) => {
                        read_errs.push(("Lecture des tâches impossible.".into(), e.to_string()));
                        Vec::new()
                    }
                };
                let present_tables = nexus_db::present_tables(&p.db);
                let tree_nodes = tree::build(&p.root);
                let search_hits: Vec<nexus_db::FtsHit> = if self.search_query.trim().is_empty() {
                    Vec::new()
                } else {
                    match nexus_db::fts_search(&p.db, self.search_query.trim(), 20) {
                        Ok(hits) => hits,
                        Err(e) => {
                            read_errs.push(("Recherche impossible.".into(), e.to_string()));
                            Vec::new()
                        }
                    }
                };

                for (msg, tech) in read_errs {
                    self.glados(msg, tech);
                }

                let day_summary = summary::build(journal_today, &mood_logs, &tasks, today);
                draw_day_summary(ui, &day_summary);
                ui.add_space(8.0);
                // Doc §5.1/§6 : bouton Redrop visible depuis le hub. Pousse
                // la même OpenPaletteRequested que le hotkey Ctrl+Shift+P
                // (doc §6 : le mécanisme EST la palette, une seule action
                // y figure aujourd'hui) — pas un raccourci, le mécanisme
                // documenté lui-même.
                if ui
                    .button(egui::RichText::new("💧 Redrop — enregistrer une prise").strong())
                    .on_hover_text("Ctrl+Shift+P fait la même chose depuis n'importe où.")
                    .clicked()
                {
                    self.pending.push(ModuleResponse::OpenPaletteRequested);
                }
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.heading("Recherche");
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.search_query);
                    if !self.search_query.is_empty() && ui.button("✕").clicked() {
                        self.search_query.clear();
                    }
                });
                if !self.search_query.trim().is_empty() {
                    if search_hits.is_empty() {
                        ui.weak("Aucun résultat.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .id_salt("hub_search_results")
                            .show(ui, |ui| {
                                for hit in &search_hits {
                                    ui.vertical(|ui| {
                                        ui.strong(&hit.file_path);
                                        ui.label(&hit.snippet);
                                    });
                                    ui.separator();
                                }
                            });
                    }
                }
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.heading("Fichiers");
                ui.weak("Format principal : Markdown (.md). Typst : secondaire, hérité.");
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .id_salt("hub_tree")
                    .show(ui, |ui| {
                        draw_tree(ui, &tree_nodes);
                    });
                ui.add_space(6.0);

                match present_tables {
                    Ok(present) => {
                        ui.label(format!(
                            "nexus.db : {}/{} tables présentes",
                            present.len(),
                            nexus_db::EXPECTED_TABLES.len()
                        ));
                    }
                    Err(e) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 46, 136),
                            format!("Lecture de l'état de la base impossible : {e}"),
                        );
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("🏥 Santé").clicked() {
                        self.pending
                            .push(ModuleResponse::OpenModuleWindow("health".into()));
                    }
                    if ui.button("📓 Journal").clicked() {
                        self.pending
                            .push(ModuleResponse::OpenModuleWindow("journal".into()));
                    }
                    if ui.button("✅ Todo").clicked() {
                        self.pending
                            .push(ModuleResponse::OpenModuleWindow("todo".into()));
                    }
                    if ui.button("📊 Dashboard").clicked() {
                        self.pending
                            .push(ModuleResponse::OpenModuleWindow("dashboard".into()));
                    }
                    if ui.button("📰 Articles").clicked() {
                        self.pending
                            .push(ModuleResponse::OpenModuleWindow("articles".into()));
                    }
                    if ui.button("🛠 Cockpit").clicked() {
                        self.pending
                            .push(ModuleResponse::OpenModuleWindow("cockpit_nexus".into()));
                    }
                });
                ui.add_space(6.0);
                if ui.button("Fermer le projet").clicked() {
                    self.proj = None;
                    self.watcher = None;
                    self.external_change_notice = false;
                    self.pending.push(ModuleResponse::PublishProjectRoot(None));
                }
            }
        }
    }
}

impl Module for NexusHubModule {
    fn name(&self) -> &'static str {
        "nexus_hub"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        self.ctx = Some(ctx.clone());
        Ok(())
    }

    fn update(&mut self, _egui_ctx: &egui::Context, _out: &mut Vec<ModuleResponse>) {
        if let Some(w) = &self.watcher {
            if w.poll_changed() {
                self.external_change_notice = true;
            }
        }
    }

    fn render_mode(&self) -> RenderMode {
        RenderMode::EmbeddedInCore
    }

    fn draw_embedded(&mut self, ui: &mut egui::Ui, out: &mut Vec<ModuleResponse>) {
        self.draw(ui);
        out.append(&mut self.pending);
    }

    fn handle_event(&mut self, _event: &CoreEvent) {
        // Rien à traiter pour cette fondation : pas d'IPC, pas de focus mode.
    }

    fn shutdown(&mut self) {
        self.proj = None; // ferme la connexion nexus_db (Drop de rusqlite::Connection).
    }
}

/// Rendu récursif de l'arbre de navigation (doc §5.1). Un dossier est un
/// `CollapsingHeader` ; un fichier est un label cliquable qui l'ouvre avec
/// le gestionnaire par défaut de l'OS (même mécanisme que `cockpit`/
/// `cockpit_nexus` pour « Ouvrir fichier », doc §5.7 : `xdg-open`/`open`).
fn draw_tree(ui: &mut egui::Ui, nodes: &[tree::TreeNode]) {
    for node in nodes {
        if node.is_dir {
            egui::CollapsingHeader::new(format!("📁 {}", node.name))
                .default_open(false)
                .show(ui, |ui| {
                    draw_tree(ui, &node.children);
                });
        } else {
            // Brief Phase 11 : le type de fichier est affiché sobrement —
            // les `.typ(st)` hérités sont marqués « Typst — secondaire ».
            let is_typst = std::path::Path::new(&node.name)
                .extension()
                .is_some_and(|e| e == "typst" || e == "typ");
            let label = if is_typst {
                format!("📄 {}  · Typst — secondaire", node.name)
            } else {
                format!("📄 {}", node.name)
            };
            if ui.selectable_label(false, label).clicked() {
                open_in_os(&node.path);
            }
        }
    }
}

fn open_in_os(path: &std::path::Path) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(cmd).arg(path).spawn() {
        tracing::warn!("Ouverture de {} ratée avec {} : {e}", path.display(), cmd);
    }
}

/// Rendu pur (aucun accès self/DB ici) du résumé du jour (doc §5.1).
fn draw_day_summary(ui: &mut egui::Ui, s: &summary::DaySummary) {
    ui.heading("Aujourd'hui");
    let ok = egui::Color32::from_rgb(120, 255, 180);
    ui.horizontal(|ui| {
        if s.journal_today.is_some() {
            ui.colored_label(ok, "✓");
            ui.label("Journal écrit aujourd'hui.");
        } else {
            ui.weak("○");
            ui.label("Pas encore d'entrée journal aujourd'hui.");
        }
    });
    ui.horizontal(|ui| match &s.mood_today {
        Some(m) => {
            ui.colored_label(ok, "✓");
            ui.label(format!(
                "État psy loggé — épuisement {}/5, cognitif {}/5, fonctionnement {}/5.",
                m.epuisement, m.cognitif, m.fonctionnement
            ));
        }
        None => {
            ui.weak("○");
            ui.label("Pas encore d'état psy loggé aujourd'hui.");
        }
    });
    if s.today_tasks.is_empty() {
        ui.horizontal(|ui| {
            ui.weak("○");
            ui.label("Aucune tâche dans la colonne « Aujourd'hui ».");
        });
    } else {
        ui.label(format!(
            "Tâches « Aujourd'hui » ({}) :",
            s.today_tasks.len()
        ));
        for t in &s.today_tasks {
            ui.horizontal(|ui| {
                ui.label("•");
                ui.label(&t.titre);
                if let Some(e) = &t.energie {
                    ui.weak(format!("[{e}]"));
                }
            });
        }
    }
}
