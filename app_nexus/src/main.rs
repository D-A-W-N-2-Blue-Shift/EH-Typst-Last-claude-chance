// ============================================================================
// app_nexus/src/main.rs — Le binaire nexus (Hive-RBMK-mod-Tcherenkov)
//
// Fork d'engram_hive : même core (engram_core, INCHANGÉ), domaine différent
// (suivi santé/organisation au lieu de prose fictionnelle). Construit son
// propre CoreContext avec un dossier de config/data distinct
// (hive_rbmk_tcherenkov, jamais engram_hive_typst) — possible sans toucher
// au core car CoreContext a des champs publics et Licorne::load / Palette::
// load prennent le dossier en paramètre (core/src/module_api.rs, licorne.rs,
// theme.rs).
//
// Modules câblés (doc de conception §10, sessions 1-3) : nexus_hub
// (EmbeddedInCore) + health (OwnViewport, sommeil/mood/médication). Le shell
// porte aussi la palette globale + Redrop (doc §2 : « ce n'est pas un
// module — un CoreEvent global déclenché depuis n'importe où »).
// ============================================================================

mod palette;
mod redrop;

use std::path::PathBuf;

use engram_core::{
    CoreContext, CoreEvent, Module, ModuleRegistry, ModuleResponse, ModulesConfig, RenderMode,
};

const APP_TITLE: &str = "Hive-RBMK-mod-Tcherenkov — Nexus";

fn main() {
    let config_dir = match dirs::config_dir() {
        Some(d) => d.join("hive_rbmk_tcherenkov"),
        None => {
            eprintln!("Impossible de trouver le dossier de configuration de l'OS.");
            std::process::exit(1);
        }
    };
    let data_dir = match dirs::data_dir() {
        Some(d) => d.join("hive_rbmk_tcherenkov"),
        None => {
            eprintln!("Impossible de trouver le dossier de données de l'OS.");
            std::process::exit(1);
        }
    };
    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        eprintln!("Impossible de créer {} : {e}", config_dir.display());
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("Impossible de créer {} : {e}", data_dir.display());
        std::process::exit(1);
    }

    tracing_subscriber::fmt::init();

    let (licorne, mut startup_status) = engram_core::Licorne::load(&config_dir);
    let (theme, theme_errors) = engram_core::Palette::load(&config_dir, &licorne);
    startup_status.extend(theme_errors);
    for e in &startup_status {
        tracing::warn!("{e}");
    }

    let core_ctx = CoreContext {
        config_dir,
        data_dir,
        theme,
        licorne,
    };

    // Câblage des modules Nexus : le SEUL endroit où un nom de module Nexus
    // apparaît côté code (§7.1). Aucun de ces noms n'appartient au registre
    // de l'app écrivain (engram_hive) et réciproquement.
    let mut registry = ModuleRegistry::new();
    registry.register("nexus_hub", || {
        Box::new(nexus_hub::NexusHubModule::default())
    });
    registry.register("health", || Box::new(health::HealthModule::default()));
    registry.register("journal", || Box::new(journal::JournalModule::default()));
    registry.register("todo", || Box::new(todo::TodoModule::default()));
    registry.register("dashboard", || {
        Box::new(dashboard::DashboardModule::default())
    });
    registry.register("articles", || Box::new(articles::ArticlesModule::default()));
    registry.register("cockpit_nexus", || {
        Box::new(cockpit_nexus::CockpitNexusModule::default())
    });

    let modules_cfg = ModulesConfig::load_from_licorne(&core_ctx.licorne, &registry);
    let mut modules = registry.instantiate(&modules_cfg);
    for m in &mut modules {
        if let Err(e) = m.init(&core_ctx) {
            let msg = format!("Init du module '{}' ratée : {e}", m.name());
            tracing::error!("{msg}");
            startup_status.push(msg);
        }
    }

    let theme_palette = core_ctx.theme.clone();
    let app = NexusApp {
        modules,
        status: startup_status,
        queued_events: Vec::new(),
        heartbeat_last_needed: false,
        project_root: None,
        palette: palette::PaletteState::default(),
        redrop: redrop::RedropState::default(),
        restart_requested: false,
    };

    let viewport = egui::ViewportBuilder::default()
        .with_title(format!("{APP_TITLE} — Core"))
        .with_inner_size([641.0, 641.0]);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        APP_TITLE,
        options,
        Box::new(move |cc| {
            engram_core::theme::apply_palette(&cc.egui_ctx, &theme_palette);
            Ok(Box::new(app))
        }),
    ) {
        eprintln!("eframe a refusé de démarrer : {e}");
        std::process::exit(1);
    }
}

/// L'application core de Nexus : tient les modules, relaie les
/// ModuleResponse, affiche un statut minimal, héberge la palette + Redrop
/// (même contrat que HiveApp côté écrivain, app/src/main.rs, plus les deux
/// derniers qui n'ont pas d'équivalent modulaire — doc §2).
struct NexusApp {
    modules: Vec<Box<dyn Module>>,
    status: Vec<String>,
    /// Événements à diffuser aux modules à la prochaine frame (relais des
    /// ModuleResponse qui ont un CoreEvent correspondant).
    queued_events: Vec<CoreEvent>,
    heartbeat_last_needed: bool,
    /// Racine du projet actif, capturée au passage de PublishProjectRoot —
    /// Redrop en a besoin pour retrouver nexus.db sans dépendre d'un module.
    project_root: Option<PathBuf>,
    palette: palette::PaletteState,
    redrop: redrop::RedropState,
    /// Posé par ModuleResponse::RestartApp (cockpit_nexus), consommé dans
    /// `update` qui a le `Context` egui nécessaire à `restart_now`.
    restart_requested: bool,
}

impl NexusApp {
    fn push_status(&mut self, line: String) {
        self.status.push(line);
        if self.status.len() > 12 {
            self.status.remove(0);
        }
    }

    /// Traite une réponse de module. Seules les variantes effectivement
    /// émises par nexus_hub/health ont un sens pour cette fondation ; les
    /// autres (OpenFile, PublishFileIndex, …) sont sans objet côté Nexus —
    /// aucun module Nexus enregistré ne les émet.
    fn process(&mut self, response: ModuleResponse) {
        match response {
            ModuleResponse::Error { module, message } => {
                tracing::error!(target: "core", "[{module}] {message}");
                self.push_status(format!("[{module}] {message}"));
            }
            ModuleResponse::OpenModuleWindow(name) => {
                tracing::info!(target: "core", "Ouverture demandée du module '{name}'.");
                self.queued_events
                    .push(CoreEvent::OpenModuleWindowRequested(name));
            }
            ModuleResponse::ToggleModuleWindow(name) => {
                tracing::info!(target: "core", "Basculer la fenêtre du module '{name}'.");
                self.queued_events
                    .push(CoreEvent::ToggleModuleWindowRequested(name));
            }
            ModuleResponse::PublishProjectRoot(root) => {
                tracing::debug!(target: "core", "Racine de projet publiée : {root:?}");
                self.project_root = root.clone();
                self.queued_events.push(CoreEvent::ProjectRootUpdated(root));
            }
            ModuleResponse::OpenPaletteRequested => {
                tracing::debug!(target: "core", "Palette demandée depuis un viewport enfant.");
                self.palette.open();
            }
            ModuleResponse::RestartApp => {
                tracing::info!(target: "core", "Redémarrage du programme demandé.");
                self.restart_requested = true;
            }
            _ => {
                // Sans objet pour les modules Nexus actuellement enregistrés.
            }
        }
    }

    /// Relance le binaire puis ferme la fenêtre racine (même mécanisme que
    /// HiveApp côté écrivain, app/src/main.rs::restart_now). La relance est
    /// différée (~0,5 s) via `sh` pour laisser l'instance courante libérer
    /// ses ressources avant que la nouvelle ne démarre.
    fn restart_now(&mut self, ctx: &egui::Context) {
        match std::env::current_exe() {
            Ok(exe) => {
                let spawned = std::process::Command::new("sh")
                    .arg("-c")
                    .arg("sleep 0.5; exec \"$0\"")
                    .arg(&exe)
                    .spawn();
                match spawned {
                    Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => self.push_status(format!(
                        "Redémarrage impossible (lancement de {} : {e}).",
                        exe.display()
                    )),
                }
            }
            Err(e) => self.push_status(format!(
                "Redémarrage impossible (binaire introuvable : {e})."
            )),
        }
    }

    /// Action « redrop » choisie dans la palette : ouvre le popup si un
    /// projet est ouvert, sinon le dit (jamais d'échec silencieux).
    fn handle_palette_action(&mut self, action: palette::PaletteAction) {
        match action {
            palette::PaletteAction::Redrop => {
                let Some(root) = self.project_root.clone() else {
                    self.push_status(
                        "Redrop : aucun projet ouvert. Ouvre un projet depuis le hub d'abord."
                            .to_string(),
                    );
                    return;
                };
                match redrop::open(&root) {
                    Ok(state) => self.redrop = state,
                    Err(e) => self.push_status(format!("Redrop indisponible : {e}")),
                }
            }
        }
    }

    /// Une prise a été confirmée depuis le popup Redrop : écrit
    /// immédiatement dans nexus.db (taken_at = now(), doc §6) et bascule
    /// vers la notification "logged" auto-disparaissante.
    fn confirm_redrop(&mut self, med_id: String, dose_mg: f64) {
        let Some(root) = &self.project_root else {
            self.push_status("Redrop : projet fermé entre-temps, prise NON enregistrée.".into());
            self.redrop = redrop::RedropState::Closed;
            return;
        };
        let db_path = root.join(".engram").join("nexus.db");
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let entry = nexus_db::MedDose {
            id: nexus_db::new_id(),
            med_id,
            taken_at: now,
            dose_mg,
            ressenti: None,
            notes: None,
        };
        match nexus_db::open_db(&db_path).and_then(|conn| nexus_db::insert_med_dose(&conn, &entry))
        {
            Ok(()) => {
                self.redrop = redrop::RedropState::Confirmed {
                    until: redrop::confirmed_until_now(),
                };
            }
            Err(e) => {
                self.push_status(format!("Redrop : enregistrement impossible ({e})."));
                self.redrop = redrop::RedropState::Closed;
            }
        }
    }
}

impl eframe::App for NexusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Diffusion des événements en attente (réponses de la frame
        // précédente) à TOUS les modules : chacun filtre (même mécanisme
        // que HiveApp côté écrivain, app/src/main.rs).
        for event in std::mem::take(&mut self.queued_events) {
            for module in &mut self.modules {
                module.handle_event(&event);
            }
        }

        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                egui::Key::P,
            )
        }) {
            self.palette.open();
        }
        // Hotkey dédiée pour le journal (doc §5.2 : « depuis n'importe
        // où »). Bascule directe, sans passer par la palette — le journal
        // est le SEUL module concerné par cette exigence explicite.
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                egui::Key::J,
            )
        }) {
            self.queued_events
                .push(CoreEvent::ToggleModuleWindowRequested("journal".into()));
        }

        let mut responses = Vec::new();
        for module in &mut self.modules {
            module.update(ctx, &mut responses);
        }

        egui::TopBottomPanel::top("nexus_core_hub")
            .resizable(true)
            .default_height(100.0)
            .min_height(48.0)
            .show(ctx, |ui| {
                ui.heading(APP_TITLE);
                ui.weak(format!("Core actif — {} module(s)", self.modules.len()));
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.status.is_empty() {
                            ui.weak("Aucune erreur pour l'instant.");
                        }
                        for line in &self.status {
                            ui.label(line);
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            for module in &mut self.modules {
                if module.render_mode() == RenderMode::EmbeddedInCore {
                    module.draw_embedded(ui, &mut responses);
                }
            }
        });

        for r in responses {
            self.process(r);
        }

        if self.restart_requested {
            self.restart_requested = false;
            self.restart_now(ctx);
        }

        if let Some(action) = palette::show(ctx, &mut self.palette) {
            self.handle_palette_action(action);
        }
        if let Some((med_id, dose_mg)) = redrop::show(ctx, &mut self.redrop) {
            self.confirm_redrop(med_id, dose_mg);
        }

        if !self.queued_events.is_empty() {
            ctx.request_repaint();
        }

        // Heartbeat de frame pour les viewports enfants (health est
        // OwnViewport, créé via show_viewport_immediate : sans ce heartbeat,
        // minimiser la fenêtre core ralentit ses repaints — régression
        // documentée côté écrivain, ARCHITECTURE.md §5). Forme générale,
        // sans exclusion par nom de module (contrat exact de
        // active_viewport_count, core/src/module_api.rs).
        let active_children: usize = self.modules.iter().map(|m| m.active_viewport_count()).sum();
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let heartbeat_needed = minimized && active_children > 0;
        if heartbeat_needed != self.heartbeat_last_needed {
            tracing::debug!(
                target: "nexus_app",
                "heartbeat minimized={minimized} active_children={active_children} request={heartbeat_needed}"
            );
            self.heartbeat_last_needed = heartbeat_needed;
        }
        if heartbeat_needed {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for module in &mut self.modules {
            module.shutdown();
        }
    }
}
