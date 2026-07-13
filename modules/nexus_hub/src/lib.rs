// ============================================================================
// modules/nexus_hub/src/lib.rs — Point d'entrée du module Hub Nexus
//
// Ce que je fais : j'implémente le trait Module du core pour la fenêtre
// principale de Nexus. Fondation de cette session : ouverture/création d'un
// projet Nexus (arborescence doc §3, chemin saisi au clavier — pas de
// sélecteur de dossier natif, cf. README_MODULE.md « limites »),
// initialisation de nexus.db (.engram/nexus.db) via nexus_db::open_db,
// affichage de son état (tables présentes / attendues).
//
// Comment je marche : EmbeddedInCore, comme file_tree côté écrivain — je ne
// possède pas de fenêtre OS propre, je me dessine dans un panel fourni par
// le binaire nexus. Je ne parle au core QUE via ModuleResponse (règle
// d'étanchéité §7 de la doctrine).
//
// Comment me virer : supprimer modules/nexus_hub/ + retirer "nexus_hub" du
// registre du binaire nexus (app_nexus/src/main.rs) + la dépendance de
// app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module), nexus_db (couche de
// données — pas un module au sens du trait Module).
//
// Différé (hors périmètre de cette fondation, §7.6 YAGNI) : arbre de
// fichiers, index FTS, watcher notify, palette de commandes, bouton Redrop.
// Viendront avec les modules health/todo/journal/dashboard.
// ============================================================================

mod project;

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
    fn open_or_create(&mut self, create: bool) {
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
                        self.open_or_create(false);
                    }
                    if ui.button("✨ Nouveau (créer la structure)").clicked() {
                        self.open_or_create(true);
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
                match nexus_db::present_tables(&p.db) {
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
        // Fondation : aucune logique différée (pas de watcher, pas
        // d'indexeur — viendront avec les futurs modules health/todo/hub).
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
