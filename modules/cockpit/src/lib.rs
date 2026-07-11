// ============================================================================
// modules/cockpit/src/lib.rs — Cockpit : centre d'accès aux configs RON
//
// Le Cockpit expose les configs réellement lues au runtime, avec un état
// visible par fichier et des actions explicites : ouvrir le fichier, ouvrir
// le dossier, recharger depuis disque, relancer l'app si nécessaire.
//
// Découplé : le cockpit ne dépend PAS des crates des autres modules. Il
// connaît UN seul format typé stable (ThemeConfig, qui vit dans le core) et
// traite les autres fichiers RON au besoin.
// ============================================================================

use std::path::{Path, PathBuf};

use engram_core::{CoreContext, Module, ModuleResponse, RenderMode};

mod config;

#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum Category {
    #[default]
    Overview,
    Engram,
}

impl Category {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Vue",
            Self::Engram => "Engram",
        }
    }
}

/// État GLaDOS local du cockpit : une bannière en haut, dismissable.
#[derive(Default)]
struct Status {
    msgs: Vec<(StatusKind, String)>,
}

#[derive(Clone, Copy)]
enum StatusKind {
    Ok,
    Warn,
}

impl Status {
    fn ok(&mut self, s: impl Into<String>) {
        self.msgs.push((StatusKind::Ok, s.into()));
    }
    fn warn(&mut self, s: impl Into<String>) {
        self.msgs.push((StatusKind::Warn, s.into()));
    }
}

#[derive(Clone)]
enum RonState {
    Missing,
    ReadError(String),
    ParseError(String),
    LoadedOk,
}

impl RonState {
    fn label(&self) -> (&'static str, egui::Color32, Option<&str>) {
        match self {
            Self::Missing => (
                "absent",
                egui::Color32::from_rgb(255, 200, 0),
                Some("Le fichier n'existe pas encore."),
            ),
            Self::ReadError(msg) => (
                "erreur de lecture",
                egui::Color32::from_rgb(255, 120, 120),
                Some(msg.as_str()),
            ),
            Self::ParseError(msg) => (
                "erreur de parsing",
                egui::Color32::from_rgb(255, 120, 120),
                Some(msg.as_str()),
            ),
            Self::LoadedOk => ("chargé OK", egui::Color32::from_rgb(120, 255, 180), None),
        }
    }

    fn is_loaded_ok(&self) -> bool {
        matches!(self, Self::LoadedOk)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReloadMode {
    Live,
    RestartRequired,
}

pub struct CockpitModule {
    config_dir: PathBuf,
    licorne: engram_core::Licorne,
    /// Catégorie sélectionnée à gauche.
    category: Category,
    /// Brouillon en cours d'édition pour la catégorie Thème (modifs non
    /// sauvegardées). `None` tant qu'on n'a pas chargé.
    theme_draft: Option<engram_core::theme::ThemeConfig>,
    /// Section experte du thème (engram.ron, repli legacy), capturée à
    /// l'init. Nécessaire pour résoudre `ThemeConfig` → `Palette` à
    /// l'identique du démarrage lors de l'application à chaud.
    theme_expert: engram_core::theme::ThemeExpert,
    /// Demande d'application à chaud du thème (posée par « Appliquer »,
    /// consommée dans `update` qui a le `Context` egui partagé).
    theme_apply_pending: bool,
    /// Demande de redémarrage du programme (bouton reboot), posée dans `draw`
    /// et consommée dans `update` qui a le canal `out` vers le core.
    restart_requested: bool,
    runtime_cfg: config::Config,
    status: Status,
    /// Fenêtre masquée (Steve a cliqué sur la croix) : on la garde fermée
    /// jusqu'à ce qu'une commande explicite la rouvre. v1 : pas encore de
    /// re-ouverture programmatique, juste pour ne pas la forcer en boucle.
    closed: bool,
}

impl Default for CockpitModule {
    fn default() -> Self {
        Self {
            config_dir: PathBuf::new(),
            licorne: engram_core::Licorne::default(),
            category: Category::default(),
            theme_draft: None,
            theme_expert: engram_core::theme::ThemeExpert::default(),
            theme_apply_pending: false,
            restart_requested: false,
            runtime_cfg: config::Config::default(),
            status: Status::default(),
            // Doctrine "fenêtre à la demande" : on ne s'impose pas au tiling
            // de l'utilisateur au démarrage. Cockpit s'ouvre sur action
            // explicite (palette "cockpit: ouvrir" ou OpenModuleWindow).
            closed: true,
        }
    }
}

impl Module for CockpitModule {
    fn name(&self) -> &'static str {
        "cockpit"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        self.config_dir = ctx.config_dir.clone();
        self.licorne = ctx.licorne.clone();
        let (cfg, errs) = config::Config::load(&self.config_dir);
        self.runtime_cfg = cfg;
        self.closed = self.runtime_cfg.closed;
        for e in errs {
            self.status.warn(e);
        }
        self.load_theme();
        Ok(())
    }

    fn render_mode(&self) -> RenderMode {
        RenderMode::OwnViewport
    }

    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        if self.closed {
            return;
        }
        let viewport_id = egui::ViewportId::from_hash_of("cockpit");
        let builder = egui::ViewportBuilder::default()
            .with_title("Engram Hive — Cockpit")
            .with_inner_size([720.0, 540.0]);
        let mut open_palette = false;
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                self.closed = true;
                self.persist_visibility();
                return;
            }
            // §7 — Ctrl+Shift+P depuis la fenêtre Cockpit.
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
        if self.restart_requested {
            self.restart_requested = false;
            out.push(ModuleResponse::RestartApp);
        }
        // Application à chaud du thème (« Appliquer ») : le Context egui est
        // partagé par tous les viewports, un seul set_visuals repeint toute
        // l'app — plus besoin de redémarrer.
        if self.theme_apply_pending {
            self.theme_apply_pending = false;
            if let Some(draft) = &self.theme_draft {
                let mut errs = Vec::new();
                let palette =
                    engram_core::theme::Palette::resolve(draft, &self.theme_expert, &mut errs);
                for e in errs {
                    self.status.warn(e);
                }
                engram_core::theme::apply_palette(egui_ctx, &palette);
            }
        }
    }

    fn active_viewport_count(&self) -> usize {
        if self.closed {
            0
        } else {
            1
        }
    }

    fn handle_event(&mut self, event: &engram_core::CoreEvent) {
        // La palette file_tree (ou tout autre déclencheur) demande l'ouverture
        // du cockpit ? On ré-ouvre la fenêtre (qui démarre `closed: true`).
        if let engram_core::CoreEvent::OpenModuleWindowRequested(name) = event {
            if name == self.name() {
                self.closed = false;
                self.persist_visibility();
            }
        }
        if let engram_core::CoreEvent::ToggleModuleWindowRequested(name) = event {
            if name == self.name() {
                self.closed = !self.closed;
                self.persist_visibility();
            }
        }
    }

    fn shutdown(&mut self) {}
}

impl CockpitModule {
    fn persist_visibility(&mut self) {
        self.runtime_cfg.closed = self.closed;
        if let Err(e) = self.runtime_cfg.save(&self.config_dir) {
            self.status.warn(e);
        }
    }

    fn load_theme(&mut self) {
        let mut errs = Vec::new();
        let simple: engram_core::theme::ThemeConfig = self.licorne.section("theme", &mut errs);
        self.theme_expert = self.licorne.section("theme_expert", &mut errs);
        for e in errs {
            self.status.warn(e);
        }
        self.theme_draft = Some(simple.clone());
    }

    /// §2.3 — Recharge toute la config depuis le disque. Le thème est
    /// réappliqué à chaud.
    fn reload_all_from_disk(&mut self) {
        let (licorne, errs) = engram_core::Licorne::load(&self.config_dir);
        self.licorne = licorne;
        for e in errs {
            self.status.warn(e);
        }
        self.load_theme();
        self.theme_apply_pending = true;
        self.status.ok("Config relue depuis le disque.");
    }

    fn draw(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("cockpit_status")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                self.draw_status(ui);
                ui.add_space(2.0);
            });
        egui::SidePanel::left("cockpit_nav")
            .resizable(false)
            .default_width(188.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("Cockpit");
                ui.weak("Source cible: engram.ron");
                ui.add_space(10.0);
                for cat in [Category::Overview, Category::Engram] {
                    let selected = self.category == cat;
                    let text = if selected {
                        egui::RichText::new(cat.label()).strong()
                    } else {
                        egui::RichText::new(cat.label())
                    };
                    if ui.selectable_label(selected, text).clicked() {
                        self.category = cat;
                    }
                }
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);
                if ui
                    .button("🔄 Recharger depuis le disque")
                    .on_hover_text(
                        "Relit les fichiers de config affichés par le cockpit. Le thème \
                         est réappliqué à chaud ; les autres configs nécessitent une \
                         relance pour être prises en compte par leur runtime.",
                    )
                    .clicked()
                {
                    self.reload_all_from_disk();
                }
                ui.add_space(6.0);
                if ui
                    .button("⏻ Redémarrer l'application")
                    .on_hover_text(
                        "Relance complète du binaire. À utiliser quand la config n'est \
                         pas relue à chaud.",
                    )
                    .clicked()
                {
                    self.restart_requested = true;
                }
                ui.add_space(10.0);
                ui.weak(
                    "Le cockpit expose les réglages réellement consommés au runtime. \
                     Objectif: accès direct, pas décor en plastique.",
                );
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::same(14)))
            .show(ctx, |ui| match self.category {
                Category::Overview => self.draw_overview(ui),
                Category::Engram => self.draw_static_config_page(
                    ui,
                    "EH5",
                    "engram.ron",
                    "config/engram.ron — cible EH5 (basic / expert / modules / providers)",
                    self.config_dir.join("engram.ron"),
                    |this| {
                        this.status.ok(
                            "engram.ron relu depuis le disque. Relance requise si la section modules change.",
                        );
                    },
                ),
            });
    }

    fn draw_status(&mut self, ui: &mut egui::Ui) {
        if self.status.msgs.is_empty() {
            ui.add_space(2.0);
            return;
        }
        let mut to_dismiss = None;
        for (i, (kind, msg)) in self.status.msgs.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                let (sym, color) = match kind {
                    StatusKind::Ok => ("✓", egui::Color32::from_rgb(120, 255, 180)),
                    StatusKind::Warn => ("⚠", egui::Color32::from_rgb(255, 46, 136)),
                };
                ui.colored_label(color, sym);
                ui.label(msg.as_str());
                if ui.small_button("✕").clicked() {
                    to_dismiss = Some(i);
                }
            });
        }
        if let Some(i) = to_dismiss {
            self.status.msgs.remove(i);
        }
    }

    fn draw_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Source unique");
        ui.weak(
            "Le cockpit vise `engram.ron` comme point d'entrée unique. Les sections \
             ci-dessous sont les seules configs runtime encore visibles.",
        );
        ui.add_space(6.0);
        egui::Grid::new("cockpit_overview")
            .striped(true)
            .spacing([12.0, 10.0])
            .show(ui, |ui| {
                let engram = self.config_dir.join("engram.ron");
                self.draw_config_row(
                    ui,
                    "EH5",
                    &engram,
                    "core/src/licorne.rs → cible EH5 (basic / expert / modules / providers)",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("engram.ron relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "theme",
                    &engram,
                    "core/src/theme.rs → section theme de engram.ron",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::Live,
                    |this| this.reload_theme_from_disk(),
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "theme_expert",
                    &engram,
                    "core/src/theme.rs → section theme_expert de engram.ron",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("theme_expert relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "editor",
                    &engram,
                    "modules/editor/src/config.rs → section editor de engram.ron",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("editor relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "editor_expert",
                    &engram,
                    "modules/editor/src/config.rs → section editor_expert de engram.ron",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("editor_expert relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "file_tree",
                    &engram,
                    "modules/file_tree/src/config.rs → section file_tree de engram.ron",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("file_tree relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "file_tree_expert",
                    &engram,
                    "modules/file_tree/src/config.rs → section file_tree_expert de engram.ron",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("file_tree_expert relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "claude_terminal",
                    &engram,
                    "modules/claude_terminal/src/config.rs → section claude_terminal de engram.ron",
                    probe_ron_file(&engram, |raw| {
                        ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("claude_terminal relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
            });
    }

    fn draw_config_row<F>(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        path: &Path,
        source: &str,
        state: RonState,
        reload_mode: ReloadMode,
        mut reload: F,
    ) where
        F: FnMut(&mut Self),
    {
        ui.vertical(|ui| {
            ui.strong(title);
            ui.weak(source);
        });
        ui.monospace(path.display().to_string());
        let (label, color, hover) = state.label();
        ui.vertical(|ui| {
            ui.colored_label(color, label);
            if let Some(h) = hover {
                ui.weak(h);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let open_file = ui.add_enabled(path.exists(), egui::Button::new("Ouvrir fichier"));
            if open_file.clicked() {
                open_ron_target(path);
            }
            if ui.button("Ouvrir dossier").clicked() {
                open_ron_folder(path);
            }
            let reload_label = "Recharger depuis disque";
            if ui.button(reload_label).clicked() {
                reload(self);
            }
            if matches!(reload_mode, ReloadMode::RestartRequired) {
                if ui.button("Relancer app").clicked() {
                    self.restart_requested = true;
                }
            }
        });
        if !state.is_loaded_ok() && path.exists() {
            ui.weak("Le cockpit montre l'état de lecture réel ; la prise en compte runtime peut exiger une relance.");
        }
        if matches!(state, RonState::Missing) {
            ui.weak(
                "Fichier absent : certains modules le créent au prochain démarrage, sinon passe par le dossier parent.",
            );
        }
    }

    fn reload_theme_from_disk(&mut self) {
        self.load_theme();
        self.theme_apply_pending = true;
        self.status
            .ok("theme.ron relu depuis le disque et appliqué.");
    }

    fn draw_static_config_page<F>(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        file_name: &str,
        hint: &str,
        path: PathBuf,
        reload: F,
    ) where
        F: FnMut(&mut Self),
    {
        let state = probe_ron_file(&path, |raw| {
            ron::from_str::<ron::Value>(raw)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        ui.heading(title);
        ui.weak(hint);
        self.draw_config_card(
            ui,
            file_name,
            &path,
            state,
            ReloadMode::RestartRequired,
            reload,
        );
    }

    fn draw_config_card<F>(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        path: &Path,
        state: RonState,
        reload_mode: ReloadMode,
        mut reload: F,
    ) where
        F: FnMut(&mut Self),
    {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(title);
            ui.monospace(path.display().to_string());
        });
        let (label, color, hover) = state.label();
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(color, label);
            if let Some(h) = hover {
                ui.weak(h);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let open_file = ui.add_enabled(path.exists(), egui::Button::new("Ouvrir fichier"));
            if open_file.clicked() {
                open_ron_target(path);
            }
            if ui.button("Ouvrir dossier").clicked() {
                open_ron_folder(path);
            }
            let reload_label = "Recharger depuis disque";
            if ui.button(reload_label).clicked() {
                reload(self);
            }
            if matches!(reload_mode, ReloadMode::RestartRequired) {
                if ui.button("Relancer app").clicked() {
                    self.restart_requested = true;
                }
            }
        });
    }
}

fn probe_ron_file(path: &Path, parse: impl Fn(&str) -> Result<(), String>) -> RonState {
    if !path.exists() {
        return RonState::Missing;
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => {
            return RonState::ReadError(format!("Lecture {} impossible : {e}", path.display()))
        }
    };
    match parse(&raw) {
        Ok(()) => RonState::LoadedOk,
        Err(e) => RonState::ParseError(e),
    }
}

fn open_ron_target(path: &Path) {
    let target = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    open_system_target(&target);
}

fn open_ron_folder(path: &Path) {
    let target = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    open_system_target(&target);
}

fn open_system_target(target: &Path) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(cmd).arg(target).spawn() {
        tracing::warn!("Ouverture de {} ratée avec {} : {e}", target.display(), cmd);
    }
}
