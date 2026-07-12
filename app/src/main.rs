// ============================================================================
// app/src/main.rs — Le binaire engram_hive
//
// Deux visages :
//   - GUI (défaut) : charge les modules activés dans modules.ron, ouvre une
//     petite fenêtre core (statut/logs des ModuleResponse) + une fenêtre OS
//     par module. Thème OLED noir + rose néon appliqué globalement.
//   - CLI : les actions de la palette invocables en headless :
//       engram_hive tree new-file --path "05_texte/..." [--name "scene_2"]
//       engram_hive context add --file "02_intendance_micro/.../svetlana.typ"
//       engram_hive context clear
//       engram_hive module add|remove <nom>   (scaffolding build-time)
//       engram_hive project open   (équivaut à lancer la GUI)
//
// Cube 0 assumé : ModuleResponse::OpenFile est reçu, logué dans le terminal
// et affiché dans la fenêtre core. L'éditeur sera branché dessus à la
// prochaine session — le plomb est posé, il ne reste qu'à visser.
// ============================================================================

mod assets;
mod module_cli;
mod palette;

use engram_core::{
    BackupConfig, BackupHandle, CoreContext, CoreEvent, IpcCommand, IpcServer, Module,
    ModuleRegistry, ModuleResponse, ModulesConfig, RenderMode,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

const APP_VERSION: &str = "v 0.4.6.Codex";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg_strs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    // Avertissements de démarrage à montrer dans la fenêtre core (en plus
    // de stderr) : visibles même si l'app n'a pas été lancée depuis un terminal.
    let mut startup_status: Vec<String> = Vec::new();

    // --- Mode CLI : actions headless de la palette. ---
    match arg_strs.as_slice() {
        ["tree", "new-file", rest @ ..] => {
            let path = flag_value(rest, "--path").unwrap_or_default();
            let name = flag_value(rest, "--name").unwrap_or("nouveau");
            exit_with(file_tree::cli_new_file(path, name));
        }
        ["context", "add", rest @ ..] => {
            let file = flag_value(rest, "--file").unwrap_or_default();
            exit_with(file_tree::cli_context_add(file));
        }
        ["context", "clear", ..] => {
            exit_with(file_tree::cli_context_clear());
        }
        // engram_hive module add|remove <nom> : scaffolding build-time d'un
        // module (crée/retire fichiers + entrées de config). cargo build après.
        ["module", rest @ ..] => {
            exit_with(module_cli::run(rest));
        }
        // engram_hive editor zoom 150 | focus-mode toggle | typewriter toggle
        //             | open /chemin/fichier.typ | save
        // → envoyé en JSON à l'instance qui tourne, via le socket Unix.
        ["editor", action, rest @ ..] => {
            exit_with(cli_editor(action, rest));
        }
        // engram_hive project open [--path <dossier>]
        // Sans --path : lance la GUI (le file tree rouvre le dernier projet).
        // Avec --path : désigne le projet à ouvrir SANS passer par
        // l'explorateur natif — l'échappatoire quand aucun portail XDG
        // (xdg-desktop-portal) n'est installé et que les boutons ne font rien.
        ["project", "open", rest @ ..] => {
            if let Some(path) = flag_value(rest, "--path") {
                match file_tree::cli_set_project(path) {
                    Ok(root) => {
                        println!("Projet désigné : {}. Lancement de la GUI…", root.display());
                    }
                    Err(e) => exit_with(Err(e)),
                }
            }
            // Puis on enchaîne sur la GUI ci-dessous.
        }
        [] => { /* GUI ci-dessous */ }
        other => {
            eprintln!(
                "Commande inconnue : {:?}. Les options sont : tree new-file, \
                 context add, context clear, editor <action>, \
                 module add|remove <nom>, \
                 project open [--path <dossier>] — ou rien pour la GUI.",
                other.join(" ")
            );
            std::process::exit(2);
        }
    }

    // --- Mode GUI. ---
    let (core_ctx, theme_errors) = match CoreContext::from_env_with_errors() {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    init_logging(&core_ctx);
    for e in theme_errors {
        tracing::warn!("{e}");
        startup_status.push(format!("⚠ {e}"));
    }

    // Socket IPC : une seule instance à la fois. Si une instance répond
    // déjà au ping, on s'arrête là — le CLI, lui, passe par le socket.
    let ipc = match IpcServer::bind(&core_ctx.data_dir) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    // Câblage des modules : le SEUL endroit où un nom de module apparaît
    // côté code. modules.ron décide de ce qui est réellement chargé.
    let mut registry = ModuleRegistry::new();
    registry.register("file_tree", || {
        Box::new(file_tree::FileTreeModule::default())
    });
    registry.register("editor", || Box::new(editor::EditorModule::default()));
    registry.register("cockpit", || Box::new(cockpit::CockpitModule::default()));
    registry.register("sticky_notes", || {
        Box::new(sticky_notes::StickyNotesModule::default())
    });
    registry.register("timeline", || Box::new(timeline::TimelineModule::default()));
    registry.register("claude_terminal", || {
        Box::new(claude_terminal::ClaudeTerminalModule::default())
    });

    let modules_cfg = ModulesConfig::load_from_licorne(&core_ctx.licorne, &registry);

    // Un module enregistré mais absent de la section `modules` n'est PAS
    // chargé. Les modules volontairement désactivés par défaut (timeline,
    // claude_terminal) ne sont pas signalés comme erreur de configuration.
    const OPTIONAL_DISABLED: &[&str] = &["timeline", "claude_terminal"];
    for name in registry.registered_names() {
        if OPTIONAL_DISABLED.contains(&name) {
            continue;
        }
        if !modules_cfg.enabled.iter().any(|n| n == name) {
            let msg = format!(
                "Module '{name}' enregistré mais ABSENT de la section modules de \
                 engram.ron — donc pas chargé. Ajoute \"{name}\" à `enabled` dans \
                 ~/.config/engram_hive_typst/engram.ron."
            );
            tracing::warn!("{msg}");
            eprintln!("{msg}");
            startup_status.push(format!("⚠ {msg}"));
        }
    }
    let mut modules = registry.instantiate(&modules_cfg);
    for m in &mut modules {
        if let Err(e) = m.init(&core_ctx) {
            let msg = format!("Init du module '{}' ratée : {e}", m.name());
            tracing::error!("{msg}");
            startup_status.push(format!("⚠ {msg}"));
        }
    }

    // WrapDrive : thread de backup automatique invisible. Sa config vient de
    // la section "backup" de licorne-a-gerber.ron.
    let backup_cfg: BackupConfig = core_ctx.licorne.section("backup", &mut startup_status);
    let backup = engram_core::backup::spawn(
        core_ctx.config_dir.clone(),
        core_ctx.data_dir.clone(),
        backup_cfg,
    );

    // La palette (theme.ron) pilote les visuels globaux egui.
    let theme_palette = core_ctx.theme.clone();
    let app = HiveApp {
        modules,
        status: startup_status,
        ipc,
        queued_events: Vec::new(),
        bg_texture: None,
        bg_tried: false,
        backup,
        palette: palette::PaletteState::default(),
        frame_time_ema: 0.0,
        frame_counter: 0,
        restart_requested: false,
    };
    // Icône d'application : absente ⇒ icône système par défaut, pas de crash.
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Engram Hive — Typst Lab — Core")
        .with_inner_size([641.0, 641.0]);
    if let Some(icon) = assets::load_icon() {
        viewport = viewport.with_icon(icon);
    }
    viewport = viewport.with_title(format!("Engram Hive — Typst Lab {APP_VERSION} — Core"));
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "Engram Hive — Typst Lab",
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

/// L'application core : tient les modules, relaie les ModuleResponse,
/// affiche un statut minimal. Le core ne connaît pas le contenu des modules.
struct HiveApp {
    modules: Vec<Box<dyn Module>>,
    /// Dernières réponses reçues, affichées dans la fenêtre core.
    status: Vec<String>,
    /// Serveur IPC (socket Unix). Vit aussi longtemps que la GUI ;
    /// Drop supprime le fichier socket.
    ipc: Option<IpcServer>,
    /// Événements à diffuser aux modules à la prochaine frame.
    queued_events: Vec<CoreEvent>,
    /// Texture du fond logo (chargée à la première frame). None = pas de fond.
    bg_texture: Option<egui::TextureHandle>,
    /// On n'essaie de charger le fond qu'une fois (évite de taper le disque
    /// chaque frame si le PNG est absent).
    bg_tried: bool,
    /// WrapDrive : poignée vers le thread de backup (déclenchement manuel +
    /// erreurs à remonter en GLaDOS).
    backup: BackupHandle,
    /// Palette globale unique, pilotée par app/core.
    palette: palette::PaletteState,
    /// Instrumentation §5 : moyenne exponentielle du temps de frame (ms).
    frame_time_ema: f32,
    /// Instrumentation §5 : compteur de frames pour la cadence de log.
    frame_counter: u64,
    /// Un module a demandé le redémarrage du programme (bouton reboot cockpit).
    /// Traité en fin de frame : relance différée + fermeture de la racine.
    restart_requested: bool,
}

impl HiveApp {
    fn process(&mut self, response: ModuleResponse) {
        match response {
            ModuleResponse::OpenFile(path) => {
                // Relais vers l'éditeur (et quiconque écoute) : le core ne
                // sait pas QUI ouvre les fichiers, il diffuse.
                self.push_status(format!("OpenFile: {}", path.display()));
                self.queued_events.push(CoreEvent::OpenFileRequested(path));
            }
            ModuleResponse::CreateAndOpenFile { path } => {
                // Wikilink orphelin : le core crée le fichier (et ses
                // parents) puis rediffuse l'ouverture, chemin canonique.
                if let Err(e) = create_empty_file(&path) {
                    tracing::error!("{e}");
                    self.push_status(e);
                    return;
                }
                match std::fs::canonicalize(&path) {
                    Ok(canonical) => {
                        self.push_status(format!("Créé + ouvert : {}", canonical.display()));
                        self.queued_events
                            .push(CoreEvent::OpenFileRequested(canonical));
                    }
                    Err(e) => self.push_status(format!(
                        "Créé mais introuvable juste après ({}) : {e}. Étrange.",
                        path.display()
                    )),
                }
            }
            ModuleResponse::PublishFileIndex {
                project_root,
                files,
            } => {
                tracing::debug!("Index projet publié : {} fichiers .typ", files.len());
                self.queued_events.push(CoreEvent::FileIndexUpdated {
                    project_root,
                    files,
                });
            }
            ModuleResponse::PublishNoteIndex {
                file_path,
                note_count,
            } => {
                tracing::debug!(
                    "Index notes publié : {} note(s) pour {}",
                    note_count,
                    file_path.display()
                );
                self.queued_events.push(CoreEvent::NoteIndexUpdated {
                    file_path,
                    note_count,
                });
            }
            ModuleResponse::FocusModeChanged(active) => {
                tracing::info!("Mode focus : {active}");
                self.queued_events.push(CoreEvent::FocusModeChanged(active));
            }
            ModuleResponse::WindowMoved { id, new_pos } => {
                tracing::debug!("Fenêtre '{id}' déplacée en {new_pos:?}");
            }
            ModuleResponse::Error { module, message } => {
                tracing::error!(target: "core", "[{module}] {message}");
                self.push_status(format!("[{module}] {message}"));
            }
            ModuleResponse::BackupNow => {
                self.backup.trigger_now();
                self.push_status("Backup manuel lancé (WrapDrive).".to_string());
            }
            ModuleResponse::OpenBackupDir => {
                let dir = self.backup.dest_dir().to_path_buf();
                // Le dossier peut ne pas exister tant qu'aucun backup n'a tourné.
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = open_in_file_manager(&dir) {
                    self.push_status(format!("Ouverture du dossier backup ratée : {e}"));
                }
            }
            ModuleResponse::OpenModuleWindow(name) => {
                tracing::info!(target: "core", "Ouverture demandée du module '{name}'.");
                self.queued_events
                    .push(CoreEvent::OpenModuleWindowRequested(name));
            }
            ModuleResponse::OpenPaletteRequested => {
                tracing::debug!(target: "core", "Palette demandée depuis un viewport enfant.");
                self.palette.open();
            }
            ModuleResponse::ToggleModuleWindow(name) => {
                tracing::info!(target: "core", "Basculer la fenêtre du module '{name}'.");
                self.queued_events
                    .push(CoreEvent::ToggleModuleWindowRequested(name));
            }
            ModuleResponse::OpenTimelineChronology => {
                tracing::info!(target: "core", "Vue chronologie demandée.");
                self.queued_events
                    .push(CoreEvent::TimelineChronologyRequested);
            }
            ModuleResponse::RestartApp => {
                tracing::info!(target: "core", "Redémarrage du programme demandé.");
                self.restart_requested = true;
            }
        }
    }

    /// Relance le binaire puis ferme la fenêtre racine. La relance est différée
    /// (~0,5 s) via `sh` pour laisser l'instance courante libérer le socket IPC
    /// (sinon la nouvelle instance verrait l'ancienne « vivante » et refuserait
    /// de démarrer). La fermeture de la racine déclenche `on_exit`/Drop → socket
    /// nettoyé.
    fn restart_now(&mut self, ctx: &egui::Context) {
        match std::env::current_exe() {
            Ok(exe) => {
                // `exec "$0"` : le chemin du binaire est passé en $0, aucun
                // problème d'échappement.
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

    fn push_status(&mut self, line: String) {
        self.status.push(line);
        if self.status.len() > 12 {
            self.status.remove(0);
        }
    }

    fn handle_palette_action(&mut self, action: file_tree::palette::PaletteAction) {
        match action {
            file_tree::palette::PaletteAction::TreeNewFile
            | file_tree::palette::PaletteAction::TreeNewFolder
            | file_tree::palette::PaletteAction::TreeRename
            | file_tree::palette::PaletteAction::ProjectOpen
            | file_tree::palette::PaletteAction::ProjectNew
            | file_tree::palette::PaletteAction::ContextAdd
            | file_tree::palette::PaletteAction::ContextClear
            | file_tree::palette::PaletteAction::ContextGoToOriginal
            | file_tree::palette::PaletteAction::ProjectSearch => {
                self.queued_events.push(CoreEvent::IpcCommand(IpcCommand {
                    module: "file_tree".to_string(),
                    action: "palette".to_string(),
                    value: Some(serde_json::Value::String(action.command().to_string())),
                    path: None,
                }));
            }
            file_tree::palette::PaletteAction::BackupNow => {
                self.process(ModuleResponse::BackupNow);
            }
            file_tree::palette::PaletteAction::BackupShowDir => {
                self.process(ModuleResponse::OpenBackupDir);
            }
            file_tree::palette::PaletteAction::CockpitOpen => {
                self.process(ModuleResponse::OpenModuleWindow("cockpit".to_string()));
            }
            file_tree::palette::PaletteAction::CockpitToggle => {
                self.process(ModuleResponse::ToggleModuleWindow("cockpit".to_string()));
            }
            file_tree::palette::PaletteAction::StickyNotesOpen => {
                self.process(ModuleResponse::OpenModuleWindow("sticky_notes".to_string()));
            }
            file_tree::palette::PaletteAction::StickyNotesToggle => {
                self.process(ModuleResponse::ToggleModuleWindow(
                    "sticky_notes".to_string(),
                ));
            }
            file_tree::palette::PaletteAction::WrapDriveOpen => {
                self.process(ModuleResponse::OpenModuleWindow(
                    "wrapdrive_panel".to_string(),
                ));
            }
            file_tree::palette::PaletteAction::TimelineOpen => {
                self.process(ModuleResponse::OpenModuleWindow("timeline".to_string()));
            }
            file_tree::palette::PaletteAction::TimelineChronology => {
                self.process(ModuleResponse::OpenTimelineChronology);
            }
            file_tree::palette::PaletteAction::ClaudeTerminalOpen => {
                self.process(ModuleResponse::OpenModuleWindow(
                    "claude_terminal".to_string(),
                ));
            }
        }
    }

    /// Dessine la fenêtre core, désormais bloc unifié : un panel haut
    /// redimensionnable (statut + fond logo = le « hub ») et, en dessous, le
    /// file_tree embarqué. La poignée du TopBottomPanel sert de séparateur
    /// draggable entre les deux sections.
    fn draw_core_window(&mut self, ctx: &egui::Context, responses: &mut Vec<ModuleResponse>) {
        // Texture de fond chargée une seule fois (absente ⇒ pas de fond).
        if !self.bg_tried {
            self.bg_tried = true;
            if let Some(img) = assets::load_core_background() {
                self.bg_texture =
                    Some(ctx.load_texture("core_bg", img, egui::TextureOptions::LINEAR));
            }
        }

        // Haut : le hub (statut + logs), avec le fond logo discret. Resizable.
        egui::TopBottomPanel::top("core_hub")
            .resizable(true)
            .default_height(150.0)
            .min_height(56.0)
            .show(ctx, |ui| {
                if let Some(tex) = &self.bg_texture {
                    let tint = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 13);
                    ui.painter().image(
                        tex.id(),
                        ui.max_rect(),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        tint,
                    );
                }
                ui.heading(format!("Engram Hive — Typst Lab {APP_VERSION}"));
                ui.weak(format!("Core actif — {} module(s)", self.modules.len()));
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.status.is_empty() {
                            ui.weak("Aucune réponse de module pour l'instant. Calme plat.");
                        }
                        for line in &self.status {
                            ui.label(line);
                        }
                    });
            });

        // Bas : le(s) module(s) embarqué(s) — le file_tree. Le bloc forme un
        // tout avec le hub : déplacer/minimiser la fenêtre core emporte les deux.
        egui::CentralPanel::default().show(ctx, |ui| {
            for module in &mut self.modules {
                if module.render_mode() == RenderMode::EmbeddedInCore {
                    module.draw_embedded(ui, responses);
                }
            }
        });
    }
}

impl eframe::App for HiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Erreurs du thread de backup (WrapDrive) → status bar GLaDOS.
        for err in self.backup.drain_errors() {
            self.push_status(err);
        }

        // Commandes IPC arrivées par le socket → événements à diffuser.
        if let Some(ipc) = &self.ipc {
            let cmds: Vec<_> = ipc.rx.try_iter().collect();
            for cmd in cmds {
                self.push_status(format!("IPC: {} {}", cmd.module, cmd.action));
                self.queued_events.push(CoreEvent::IpcCommand(cmd));
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

        // Diffusion des événements en attente (réponses de la frame
        // précédente + IPC) à TOUS les modules : chacun filtre.
        for event in std::mem::take(&mut self.queued_events) {
            for module in &mut self.modules {
                module.handle_event(&event);
            }
        }

        // Tour de piste des modules : logique + rendu des modules à viewport
        // propre (l'éditeur dessine ses fenêtres ici). Les modules embarqués
        // (file_tree) ne font QUE leur logique ici ; ils seront dessinés dans
        // la fenêtre core juste après.
        let frame_start = std::time::Instant::now();
        let mut responses = Vec::new();
        for module in &mut self.modules {
            module.update(ctx, &mut responses);
        }

        // Fenêtre core = bloc « hub » : en haut le statut (avec le fond logo),
        // en dessous le file_tree embarqué, séparés par une poignée draggable.
        self.draw_core_window(ctx, &mut responses);

        for r in responses {
            self.process(r);
        }
        if let Some(action) = palette::show(ctx, &mut self.palette) {
            self.handle_palette_action(action);
        }
        // Redémarrage demandé (bouton reboot du cockpit) : relance différée +
        // fermeture propre de la fenêtre racine.
        if self.restart_requested {
            self.restart_requested = false;
            self.restart_now(ctx);
        }
        // Des événements ont été générés : re-peindre tout de suite pour
        // les livrer sans attendre une interaction.
        if !self.queued_events.is_empty() {
            ctx.request_repaint();
        }

        // §5 — Heartbeat de frame pour les viewports enfants.
        // Les fenêtres OS de l'éditeur sont créées via show_viewport_immediate :
        // elles sont peintes DANS la boucle de frame du core. Quand le core
        // est minimisé, Wayland cesse d'envoyer les frame callbacks ; le core
        // ne redraw que sur événement (notify, IPC) — les fenêtres éditeur
        // ralentissent en conséquence (cause de la régression signalée).
        // Solution : tant qu'au moins un viewport enfant est ouvert, on
        // demande un repaint dans 16ms via le timer winit, qui fonctionne
        // même quand la fenêtre est minimisée. En état visible, la requête
        // est fusionnée avec la vsync du compositeur : coût négligeable.
        let active_children: usize = self
            .modules
            .iter()
            .filter(|m| m.name() != "cockpit")
            .map(|m| m.active_viewport_count())
            .sum();
        if active_children > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // Instrumentation §5 : temps de frame du core, moyenne glissante 60
        // frames. Activer avec RUST_LOG=engram_hive=debug pour lire les
        // chiffres core-visible vs core-minimisé (test §5 du brief).
        let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        self.frame_time_ema = if self.frame_time_ema == 0.0 {
            frame_ms
        } else {
            self.frame_time_ema * 0.98 + frame_ms * 0.02
        };
        self.frame_counter = self.frame_counter.wrapping_add(1);
        if self.frame_counter % 60 == 0 {
            tracing::debug!(
                target: "engram_hive",
                "frame_time ema={:.2}ms last={:.2}ms active_children={}",
                self.frame_time_ema,
                frame_ms,
                active_children
            );
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for module in &mut self.modules {
            module.shutdown();
        }
    }
}

/// Ouvre un dossier dans le gestionnaire de fichiers du système. `xdg-open`
/// sur Linux/BSD, `open` sur macOS. Aucune dépendance : on délègue à l'OS.
fn open_in_file_manager(dir: &std::path::Path) -> Result<(), String> {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{cmd} {} : {e}", dir.display()))
}

/// Logs : stdout + un fichier par module dans
/// ~/.local/share/engram_hive/logs/modules/<module>.log
/// (règle GLaDOS : le fichier de log par module existe, ET les erreurs sont
/// affichées dans la fenêtre du module — jamais l'un sans l'autre).
fn init_logging(ctx: &CoreContext) {
    use tracing_subscriber::Registry;
    let logs_dir = ctx.data_dir.join("logs").join("modules");
    let _ = std::fs::create_dir_all(&logs_dir);
    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> =
        vec![tracing_subscriber::fmt::layer().boxed()];
    for module in [
        "file_tree",
        "editor",
        "cockpit",
        "timeline",
        "claude_terminal",
    ] {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs_dir.join(format!("{module}.log")))
            .ok();
        if let Some(f) = file {
            layers.push(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::sync::Arc::new(f))
                    .with_ansi(false)
                    .with_filter(
                        tracing_subscriber::filter::Targets::new()
                            .with_target(module, tracing::Level::TRACE),
                    )
                    .boxed(),
            );
        }
    }
    tracing_subscriber::registry().with(layers).init();
}

/// Crée un fichier vide + ses dossiers parents (wikilink orphelin).
fn create_empty_file(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        return Ok(()); // déjà là : on se contente de l'ouvrir.
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Impossible de créer {} : {e}", parent.display()))?;
    }
    std::fs::write(path, "").map_err(|e| format!("Impossible de créer {} : {e}", path.display()))
}

/// `engram_hive editor <action> [valeur]` → une commande JSON sur le socket.
fn cli_editor(action: &str, rest: &[&str]) -> Result<String, String> {
    let core_ctx = CoreContext::from_env()?;
    let mut cmd = IpcCommand {
        module: "editor".into(),
        action: action.into(),
        value: None,
        path: None,
    };
    match action {
        "open" => {
            let p = rest
                .first()
                .ok_or("Ouvrir quoi ? Il manque le chemin du fichier.")?;
            // Chemin absolu : le CLI peut être lancé depuis n'importe où.
            let abs =
                std::fs::canonicalize(p).map_err(|e| format!("'{p}' : {e}. Vérifie le chemin."))?;
            cmd.path = Some(abs);
        }
        "zoom" => {
            let v = rest
                .first()
                .ok_or("Zoom à combien ? Il manque la valeur (ex: 150).")?;
            let n: f64 = v.parse().map_err(|_| {
                format!("'{v}' n'est pas un nombre. Le zoom se mesure en chiffres, ici.")
            })?;
            cmd.value = Some(serde_json::json!(n));
        }
        // focus-mode toggle, typewriter toggle, save, … : la valeur texte
        // passe telle quelle, le module éditeur interprète.
        _ => {
            if let Some(v) = rest.first() {
                cmd.value = Some(serde_json::json!(v));
            }
        }
    }
    let resp = engram_core::ipc::send_command(&core_ctx.data_dir, &cmd)?;
    Ok(format!("editor {action} → {resp}"))
}

/// Extrait la valeur d'un flag CLI ("--path", "--file", "--name").
fn flag_value<'a>(args: &'a [&'a str], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| *a == flag)
        .and_then(|i| args.get(i + 1).copied())
}

fn exit_with(result: Result<String, String>) -> ! {
    match result {
        Ok(msg) => {
            println!("{msg}");
            std::process::exit(0);
        }
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(1);
        }
    }
}
