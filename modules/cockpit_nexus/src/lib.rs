// ============================================================================
// modules/cockpit_nexus/src/lib.rs — Cockpit Nexus : statut + actions config
//
// Même patron que le Cockpit de l'app écrivain (modules/cockpit) : PAS un
// formulaire d'édition de valeurs. Vérifié par lecture directe de son
// source avant d'écrire ce module : malgré le commentaire d'en-tête
// « édition GUI des fichiers RON », le Cockpit écrivain réel n'a que 4
// actions par ligne de config — « Ouvrir fichier » / « Ouvrir dossier » /
// « Recharger depuis disque » / « Relancer app » — jamais de champ de
// saisie de valeur individuelle. Le vrai mécanisme d'édition établi dans ce
// dépôt : ouvrir le RON dans l'éditeur externe de l'utilisateur (xdg-open),
// pas un formulaire interne. Ce module reproduit ce patron pour les
// sections Nexus réelles, doc §5.7 : « config journal (template), config
// medications (…), config todo (colonnes, filtres par défaut), config
// dashboard (fenêtres temporelles, seuils d'alerte) ».
//
// Panneau : theme (rechargement à chaud, même mécanisme que le Cockpit
// écrivain) + theme_expert + journal + dashboard + todo + health
// (rechargement = relance requise, aucun de ces 4 modules ne relit sa
// config à chaud) + une carte d'INFORMATION (pas un fichier) pour les
// médicaments, délibérément
// hors RON — décision incrément 3 (health/README_MODULE.md) : nexus_db.
// medications est la source de vérité, pas un fichier de config. Une ligne
// de cockpit qui pointerait vers un fichier RON inexistant et jamais lu
// serait un décor en plastique — exactement ce que le Cockpit écrivain
// s'interdit (son propre texte d'aide : « pas décor en plastique »).
// Colonnes todo : non exposées ici non plus, même raison — voir
// modules/todo/src/config.rs et README_MODULE.md.
//
// « colonnes » n'a pas d'équivalent ici : voir modules/todo/README_MODULE.md
// pour la justification (statuts canoniques dont dépend la récurrence).
//
// Comment me virer : supprimer modules/cockpit_nexus/ + retirer
// "cockpit_nexus" du registre du binaire nexus (app_nexus/src/main.rs) + la
// dépendance de app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module + theme::{ThemeConfig,
// ThemeExpert, Palette, apply_palette} + Licorne, tous déjà publics et
// partagés avec le Cockpit écrivain), ron (probe des fichiers RON).
// ============================================================================

use std::path::{Path, PathBuf};

use engram_core::{CoreContext, Module, ModuleResponse, RenderMode};

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
                Some("Le fichier n'existe pas encore — créé au prochain démarrage de Nexus."),
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
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReloadMode {
    Live,
    RestartRequired,
}

pub struct CockpitNexusModule {
    config_dir: PathBuf,
    licorne: engram_core::Licorne,
    theme_draft: Option<engram_core::theme::ThemeConfig>,
    theme_expert: engram_core::theme::ThemeExpert,
    theme_apply_pending: bool,
    restart_requested: bool,
    status: Status,
    closed: bool,
}

impl Default for CockpitNexusModule {
    fn default() -> Self {
        Self {
            config_dir: PathBuf::new(),
            licorne: engram_core::Licorne::default(),
            theme_draft: None,
            theme_expert: engram_core::theme::ThemeExpert::default(),
            theme_apply_pending: false,
            restart_requested: false,
            status: Status::default(),
            // Doctrine « fenêtre à la demande » : pas d'imposition au
            // tiling au démarrage, même règle que tous les modules Nexus.
            closed: true,
        }
    }
}

impl Module for CockpitNexusModule {
    fn name(&self) -> &'static str {
        "cockpit_nexus"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        self.config_dir = ctx.config_dir.clone();
        self.licorne = ctx.licorne.clone();
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
        let viewport_id = egui::ViewportId::from_hash_of("cockpit_nexus");
        let builder = egui::ViewportBuilder::default()
            .with_title("Hive-RBMK-mod-Tcherenkov — Cockpit")
            .with_inner_size([820.0, 620.0]);
        let mut open_palette = false;
        let mut open_health = false;
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
            self.draw(ctx, &mut open_health);
        });
        if open_palette {
            out.push(ModuleResponse::OpenPaletteRequested);
        }
        if open_health {
            out.push(ModuleResponse::OpenModuleWindow("health".into()));
        }
        if self.restart_requested {
            self.restart_requested = false;
            out.push(ModuleResponse::RestartApp);
        }
        // Application à chaud du thème (même mécanisme que le Cockpit
        // écrivain) : le Context egui est partagé par tous les viewports,
        // un seul set_visuals repeint toute l'app.
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
        if let engram_core::CoreEvent::OpenModuleWindowRequested(name) = event {
            if name == self.name() {
                self.closed = false;
            }
        }
        if let engram_core::CoreEvent::ToggleModuleWindowRequested(name) = event {
            if name == self.name() {
                self.closed = !self.closed;
            }
        }
    }

    fn shutdown(&mut self) {}
}

impl CockpitNexusModule {
    fn load_theme(&mut self) {
        let mut errs = Vec::new();
        let simple: engram_core::theme::ThemeConfig = self.licorne.section("theme", &mut errs);
        self.theme_expert = self.licorne.section("theme_expert", &mut errs);
        for e in errs {
            self.status.warn(e);
        }
        self.theme_draft = Some(simple);
    }

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

    fn reload_theme_from_disk(&mut self) {
        self.load_theme();
        self.theme_apply_pending = true;
        self.status.ok("theme relu depuis le disque et appliqué.");
    }

    fn draw(&mut self, ctx: &egui::Context, open_health: &mut bool) {
        egui::TopBottomPanel::top("cockpit_nexus_status")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                self.draw_status(ui);
                ui.add_space(2.0);
            });
        egui::SidePanel::left("cockpit_nexus_nav")
            .resizable(false)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("Cockpit Nexus");
                ui.weak("Source cible : engram.ron (Nexus)");
                ui.add_space(10.0);
                if ui
                    .button("🔄 Recharger depuis le disque")
                    .on_hover_text(
                        "Relit les fichiers de config affichés ici. Le thème est réappliqué \
                         à chaud ; journal/dashboard/todo nécessitent une relance pour être \
                         pris en compte par leur runtime.",
                    )
                    .clicked()
                {
                    self.reload_all_from_disk();
                }
                ui.add_space(6.0);
                if ui
                    .button("⏻ Redémarrer l'application")
                    .on_hover_text(
                        "Relance complète du binaire nexus. À utiliser quand la config \
                         n'est pas relue à chaud.",
                    )
                    .clicked()
                {
                    self.restart_requested = true;
                }
                ui.add_space(10.0);
                ui.weak(
                    "Le cockpit expose les réglages réellement consommés au runtime. \
                     Objectif : accès direct, pas décor en plastique.",
                );
            });
        // §glitch géométrie (rapport signalé) : un Frame::inner_margin sur un
        // CentralPanel dont le contenu a une hauteur variable (retour à la
        // ligne dépendant de la largeur disponible) peut osciller — la marge
        // réduit la largeur, un groupe de boutons passe sur 2 lignes, la
        // hauteur de contenu change, le viewport renégocie sa taille, ce qui
        // change à nouveau la largeur disponible, etc. Pas de Frame ici ;
        // le contenu est dans une ScrollArea verticale pour que sa hauteur
        // ne puisse plus jamais entraîner de renégociation de la taille de
        // la fenêtre (absorbée par le scroll, jamais par un redimensionnement
        // de viewport).
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.draw_overview(ui, open_health));
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

    fn draw_overview(&mut self, ui: &mut egui::Ui, open_health: &mut bool) {
        let engram = self.config_dir.join("engram.ron");
        let engram_state = probe_ron_file(&engram, |raw| {
            ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        ui.heading("Configuration Nexus");
        ui.weak(
            "engram.ron (dossier de config `hive_rbmk_tcherenkov`) est la source unique. \
             Les sections ci-dessous sont celles réellement lues par les modules Nexus.",
        );
        ui.add_space(6.0);
        egui::Grid::new("cockpit_nexus_overview")
            .striped(true)
            .spacing([12.0, 10.0])
            .show(ui, |ui| {
                self.draw_config_row(
                    ui,
                    "theme",
                    &engram,
                    "engram_core::theme → section theme de engram.ron",
                    engram_state.clone(),
                    ReloadMode::Live,
                    |this| this.reload_theme_from_disk(),
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "theme_expert",
                    &engram,
                    "engram_core::theme → section theme_expert de engram.ron",
                    engram_state.clone(),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("theme_expert relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "journal",
                    &engram,
                    "modules/journal/src/config.rs → gabarit de création (doc §5.2)",
                    engram_state.clone(),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("journal relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "dashboard",
                    &engram,
                    "modules/dashboard/src/config.rs → fenêtre par défaut + seuils d'alerte",
                    engram_state.clone(),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("dashboard relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "todo",
                    &engram,
                    "modules/todo/src/config.rs → filtres par défaut",
                    engram_state.clone(),
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("todo relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
                self.draw_config_row(
                    ui,
                    "health",
                    &engram,
                    "modules/health/src/config.rs → rappel de ressenti différé (doc §5.3)",
                    engram_state,
                    ReloadMode::RestartRequired,
                    |this| {
                        this.status
                            .ok("health relu depuis le disque. Relance requise.");
                    },
                );
                ui.end_row();
            });
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(6.0);
        ui.strong("Médicaments");
        ui.weak(
            "Pas une section de engram.ron : le registre des médicaments vit dans \
             nexus.db (table `medications`), pas dans un fichier RON — décision \
             documentée à l'incrément 3 (voir health/README_MODULE.md). Édité \
             directement depuis la fenêtre Santé.",
        );
        if ui.button("🏥 Ouvrir Santé").clicked() {
            *open_health = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
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
            if ui.button("Recharger depuis disque").clicked() {
                reload(self);
            }
            if matches!(reload_mode, ReloadMode::RestartRequired)
                && ui.button("Relancer app").clicked()
            {
                self.restart_requested = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_ron_file_absent() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("inexistant.ron");
        let state = probe_ron_file(&path, |_| Ok(()));
        assert!(matches!(state, RonState::Missing));
    }

    #[test]
    fn probe_ron_file_valide() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("engram.ron");
        std::fs::write(&path, "{}").expect("write");
        let state = probe_ron_file(&path, |raw| {
            ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        assert!(matches!(state, RonState::LoadedOk));
    }

    #[test]
    fn probe_ron_file_invalide() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("engram.ron");
        std::fs::write(&path, "ceci n'est pas du ron valide {{{").expect("write");
        let state = probe_ron_file(&path, |raw| {
            ron::from_str::<std::collections::HashMap<String, ron::Value>>(raw)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        assert!(matches!(state, RonState::ParseError(_)));
    }
}
