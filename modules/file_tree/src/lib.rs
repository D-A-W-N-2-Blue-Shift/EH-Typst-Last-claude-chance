// ============================================================================
// modules/file_tree/src/lib.rs — Point d'entrée du module File Tree
//
// Implémente le trait Module du core. C'est l'orchestrateur :
//   - une fenêtre OS native (viewport egui indépendant)
//   - ouverture/création de projet via le sélecteur de dossier intégré
//     (folder_picker, 100% egui — aucune dépendance au portail XDG)
//   - tree sémantique (tree_view) nourri par l'indexeur SQLite (indexer)
//   - clic droit (context_menu) + palette globale pilotée par le core
//   - table de travail 06_en_cours/ en symlinks (symlinks)
//   - session.ron (positions et tailles locales)
//   - erreurs GLaDOS : tout Err remonte dans la fenêtre, pas que dans les logs
//
// Le module ne parle au core QUE via ModuleResponse. Pour le virer :
// supprimer modules/file_tree/ + retirer "file_tree" de modules.ron.
// ============================================================================

mod config;
mod context_menu;
mod folder_picker;
mod indexer;
mod layout;
pub mod palette;
mod project;
mod stats;
mod symlinks;
mod temporal;
mod tree_view;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse};

use context_menu::MenuAction;
use folder_picker::{FolderPicker, PickerOutcome};
use palette::PaletteAction;
use stats::{ProjectTotals, TreeNode};

/// Pourquoi le sélecteur de dossier intégré a été ouvert.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerPurpose {
    OpenProject,
    NewProjectLocation,
}

/// Boîtes de dialogue modales (nom uniquement — jamais de chemin libre :
/// le choix de dossier passe par le sélecteur intégré, folder_picker).
enum Dialog {
    NewFile {
        parent: PathBuf,
        name: String,
    },
    NewFolder {
        parent: PathBuf,
        name: String,
    },
    Rename {
        target: PathBuf,
        name: String,
    },
    ConfirmDelete {
        target: PathBuf,
        non_empty: bool,
    },
    NewProjectName {
        location: PathBuf,
        name: String,
    },
    ProjectOpenPrompt {
        root: PathBuf,
        state: project::ProjectState,
    },
    StructurePreview {
        root: PathBuf,
        create: bool,
        items: Vec<String>,
    },
    CorpusSearch {
        query: String,
        results: Vec<indexer::CorpusHit>,
        last_error: Option<String>,
        last_run: String,
    },
}

/// Un projet ouvert : racine, config, session, indexeur, modèle d'arbre.
struct OpenProject {
    root: PathBuf,
    config: project::Project,
    session: layout::SessionConfig,
    indexer: indexer::IndexerHandle,
    tree: Option<(TreeNode, ProjectTotals)>,
    seen_generation: u64,
    /// L'utilisateur a créé/renommé/supprimé : reconstruire l'arbre même
    /// sans nouvelle génération d'index (les dossiers vides ne passent pas
    /// par notify).
    structure_dirty: bool,
    session_dirty: bool,
    last_session_save: Instant,
}

/// Le module File Tree. Depuis la fusion core+file_tree, il ne possède PAS de
/// viewport propre : il se dessine dans un panel de la fenêtre core (mode
/// `EmbeddedInCore`). Les deux forment un seul bloc visuel — le « hub ».
#[derive(Default)]
pub struct FileTreeModule {
    ctx: Option<CoreContext>,
    cfg: config::Config,
    proj: Option<OpenProject>,
    ui_state: tree_view::TreeUiState,
    dialog: Option<Dialog>,
    /// Messages GLaDOS visibles dans la fenêtre (max 4, dismissables).
    glados: Vec<String>,
    pending: Vec<ModuleResponse>,
    /// Sélecteur de dossier intégré (egui) ouvert, le cas échéant.
    picker: Option<FolderPicker>,
    /// Projet à ouvrir à la prochaine frame (reopen_last_project à l'init).
    pending_open: Option<PathBuf>,
    /// Mode focus de l'éditeur : le panel file_tree se masque (état conservé).
    hidden_by_focus: bool,
    /// Le dialogue modal était-il déjà affiché à la frame précédente ? Sert à
    /// ne demander le focus du champ qu'UNE fois, à l'ouverture. Réclamer le
    /// focus à chaque frame le fait se disputer avec les champs des autres
    /// viewports et casse la saisie (bug « une lettre »). Voir
    /// tests/focus_input.rs.
    dialog_shown_prev: bool,
}

impl FileTreeModule {
    /// Pousse une erreur GLaDOS : visible dans la fenêtre + loguée + remontée au core.
    fn glados(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        tracing::error!(target: "file_tree", "{msg}");
        self.pending.push(ModuleResponse::Error {
            module: "file_tree".into(),
            message: msg.clone(),
        });
        self.glados.push(msg);
        if self.glados.len() > 4 {
            self.glados.remove(0);
        }
    }

    // -----------------------------------------------------------------------
    // Cycle de vie projet.
    // -----------------------------------------------------------------------

    fn open_project(&mut self, root: PathBuf, egui_ctx: &egui::Context, scaffold: bool) {
        let root = match std::fs::canonicalize(&root) {
            Ok(r) => r,
            Err(e) => {
                self.glados(format!(
                    "Dossier introuvable : '{}' ({e}). Il a peut-être déménagé sans te \
                     prévenir. Ouvre un projet existant ou crée-en un nouveau.",
                    root.display()
                ));
                return;
            }
        };
        if scaffold {
            if let Err(e) = project::scaffold(&root) {
                self.glados(e);
                return;
            }
        }
        let Some(ctx) = self.ctx.clone() else {
            self.glados(
                "Ouverture impossible : le module file_tree n'a pas été initialisé \
                 (init() non appelé avant open_project).",
            );
            return;
        };
        let indexer = match indexer::spawn(root.clone(), self.cfg.clone(), egui_ctx.clone()) {
            Ok(h) => h,
            Err(e) => {
                self.glados(e);
                return;
            }
        };
        let session = layout::SessionConfig::load(&root);
        let config = project::load_project_config(&ctx.config_dir, &root);
        project::save_last_project(&self.cfg.module_config_dir, &root);
        self.proj = Some(OpenProject {
            root,
            config,
            session,
            indexer,
            tree: None,
            seen_generation: 0,
            structure_dirty: true,
            session_dirty: false,
            last_session_save: Instant::now(),
        });
        self.ui_state = tree_view::TreeUiState::default();
    }

    /// Ouvre le sélecteur de dossier intégré (egui). Aucun thread, aucun
    /// portail système : 100% sous notre contrôle, identique sur tout WM.
    fn start_picker(&mut self, purpose: PickerPurpose) {
        if self.picker.is_some() {
            return; // un sélecteur à la fois.
        }
        let start = self.picker_start_dir();
        self.picker = Some(FolderPicker::new(purpose, start));
    }

    /// Dossier de départ du sélecteur : le parent du projet courant, sinon
    /// celui du dernier projet ouvert, sinon le home.
    fn picker_start_dir(&self) -> PathBuf {
        if let Some(p) = &self.proj {
            if let Some(parent) = p.root.parent() {
                return parent.to_path_buf();
            }
        }
        if let Some(last) = project::load_last_project(&self.cfg.module_config_dir) {
            if let Some(parent) = last.parent() {
                return parent.to_path_buf();
            }
        }
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    }

    /// Affiche le sélecteur (s'il est ouvert) et traite son résultat.
    fn show_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        match picker.show(ctx) {
            PickerOutcome::Browsing => {}
            PickerOutcome::Cancelled => self.picker = None,
            PickerOutcome::Selected(folder) => {
                let Some(chosen) = self.picker.take() else {
                    return;
                };
                let purpose = chosen.purpose;
                tracing::info!(target: "file_tree", "Dossier choisi : {}", folder.display());
                match purpose {
                    PickerPurpose::OpenProject => match project::detect_project_state(&folder) {
                        project::ProjectState::Engram => self.open_project(folder, ctx, false),
                        state => {
                            self.dialog = Some(Dialog::ProjectOpenPrompt {
                                root: folder,
                                state,
                            });
                        }
                    },
                    PickerPurpose::NewProjectLocation => {
                        self.dialog = Some(Dialog::NewProjectName {
                            location: folder,
                            name: String::new(),
                        });
                    }
                }
            }
        }
    }

    /// Reconstruit le modèle d'arbre si l'indexeur a publié une nouvelle
    /// génération ou si une action utilisateur a changé la structure.
    /// Publie au passage l'index des .typ vers le core (PublishFileIndex) :
    /// c'est lui qui nourrit l'auto-complétion des wikilinks de l'éditeur —
    /// l'éditeur ne scanne jamais le filesystem lui-même.
    fn refresh_tree(&mut self) {
        let cfg = self.cfg.clone();
        let Some(p) = &mut self.proj else { return };
        let generation = p
            .indexer
            .shared
            .generation
            .load(std::sync::atomic::Ordering::SeqCst);
        if generation != p.seen_generation || p.structure_dirty || p.tree.is_none() {
            let files = p
                .indexer
                .shared
                .snapshot
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            p.tree = Some(stats::build_tree(&p.root, &files, &cfg));
            p.seen_generation = generation;
            p.structure_dirty = false;
            let mut typst_files: Vec<PathBuf> = files
                .keys()
                .filter(|f| f.extension().is_some_and(|e| e == "typ"))
                .cloned()
                .collect();
            typst_files.sort();
            self.pending.push(ModuleResponse::PublishFileIndex {
                project_root: p.root.clone(),
                files: std::sync::Arc::new(typst_files),
            });
        }
    }

    // -----------------------------------------------------------------------
    // Exécution des actions (menus + palette).
    // -----------------------------------------------------------------------

    fn open_file(&mut self, path: &PathBuf) {
        let canonical = match std::fs::canonicalize(path) {
            Ok(canonical) => canonical,
            Err(e) => {
                self.glados(format!(
                    "Impossible d'ouvrir '{}' : {e}. Le fichier existait il y a une seconde, \
                     je te le jure.",
                    path.display()
                ));
                return;
            }
        };
        match open_kind(&canonical) {
            OpenKind::EngramText => self.pending.push(ModuleResponse::OpenFile(canonical)),
            OpenKind::Okular => {
                if let Err(e) = open_with_cmd("okular", &canonical) {
                    self.glados(e);
                }
            }
            OpenKind::DefaultApp => {
                if let Err(e) = open_in_default_app(&canonical) {
                    self.glados(e);
                }
            }
        }
    }

    fn run_menu_action(&mut self, action: MenuAction, egui_ctx: &egui::Context) {
        match action {
            MenuAction::Open(p) => self.open_file(&p),
            MenuAction::NewFileIn(parent) => {
                self.dialog = Some(Dialog::NewFile {
                    parent,
                    name: String::new(),
                });
            }
            MenuAction::NewFolderIn(parent) => {
                self.dialog = Some(Dialog::NewFolder {
                    parent,
                    name: String::new(),
                });
            }
            MenuAction::Rename(target) => {
                let name = target
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.dialog = Some(Dialog::Rename { target, name });
            }
            MenuAction::Delete(target) => {
                let non_empty = target.is_dir()
                    && std::fs::read_dir(&target)
                        .map(|mut rd| rd.next().is_some())
                        .unwrap_or(false);
                self.dialog = Some(Dialog::ConfirmDelete { target, non_empty });
            }
            MenuAction::AddToEnCours(target) => {
                if let Some(p) = &self.proj {
                    let root = p.root.clone();
                    match symlinks::add_to_en_cours(&root, &target) {
                        Ok(_) => self.mark_structure_dirty(),
                        Err(e) => self.glados(e),
                    }
                }
            }
            MenuAction::CopyPath(target) => {
                let text = std::fs::canonicalize(&target)
                    .unwrap_or(target)
                    .display()
                    .to_string();
                egui_ctx.copy_text(text);
            }
            MenuAction::RemoveFromEnCours(link) => {
                if let Some(p) = &self.proj {
                    let root = p.root.clone();
                    match symlinks::remove_from_en_cours(&root, &link) {
                        Ok(()) => self.mark_structure_dirty(),
                        Err(e) => self.glados(e),
                    }
                }
            }
            MenuAction::GoToOriginal(link) => match symlinks::resolve_original(&link) {
                Ok(original) => {
                    // Déplie tous les ancêtres et sélectionne l'original.
                    if let Some(p) = &self.proj {
                        let mut cur = original.parent();
                        while let Some(dir) = cur {
                            if !dir.starts_with(&p.root) || dir == p.root {
                                break;
                            }
                            self.ui_state.expanded.insert(dir.to_path_buf());
                            cur = dir.parent();
                        }
                    }
                    self.ui_state.selected = Some(original);
                }
                Err(e) => self.glados(e),
            },
        }
    }

    fn run_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::ProjectOpen => self.start_picker(PickerPurpose::OpenProject),
            PaletteAction::ProjectNew => self.start_picker(PickerPurpose::NewProjectLocation),
            PaletteAction::TreeNewFile | PaletteAction::TreeNewFolder => {
                let Some(p) = &self.proj else {
                    self.glados("Aucun projet ouvert. Crée ou ouvre un projet d'abord.");
                    return;
                };
                // Cible : le dossier sélectionné, sinon le parent du fichier
                // sélectionné, sinon la racine.
                let parent = match &self.ui_state.selected {
                    Some(sel) if sel.is_dir() => sel.clone(),
                    Some(sel) => sel
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| p.root.clone()),
                    None => p.root.clone(),
                };
                self.dialog = Some(if matches!(action, PaletteAction::TreeNewFile) {
                    Dialog::NewFile {
                        parent,
                        name: String::new(),
                    }
                } else {
                    Dialog::NewFolder {
                        parent,
                        name: String::new(),
                    }
                });
            }
            PaletteAction::TreeRename => match self.ui_state.selected.clone() {
                Some(target) => {
                    let name = target
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.dialog = Some(Dialog::Rename { target, name });
                }
                None => self.glados("Renommer quoi ? Sélectionne d'abord un élément dans le tree."),
            },
            PaletteAction::ContextAdd => {
                let (Some(p), Some(sel)) = (&self.proj, self.ui_state.selected.clone()) else {
                    self.glados("Ajouter quoi à En Cours ? Sélectionne un fichier d'abord.");
                    return;
                };
                let root = p.root.clone();
                match symlinks::add_to_en_cours(&root, &sel) {
                    Ok(_) => self.mark_structure_dirty(),
                    Err(e) => self.glados(e),
                }
            }
            PaletteAction::ContextClear => {
                if let Some(p) = &self.proj {
                    let root = p.root.clone();
                    match symlinks::clear_en_cours(&root) {
                        Ok(n) => {
                            tracing::info!(target: "file_tree", "En Cours vidé : {n} liens retirés.");
                            self.mark_structure_dirty();
                        }
                        Err(e) => self.glados(e),
                    }
                }
            }
            PaletteAction::ContextGoToOriginal => match self.ui_state.selected.clone() {
                Some(sel) => {
                    let a = MenuAction::GoToOriginal(sel);
                    // Pas besoin du ctx egui ici, GoToOriginal n'y touche pas.
                    self.run_menu_action(a, &egui::Context::default());
                }
                None => self.glados("Sélectionne un lien dans En Cours d'abord."),
            },
            PaletteAction::ProjectSearch => {
                let Some(p) = &self.proj else {
                    self.glados("Aucun projet ouvert. Ouvre un projet avant de chercher.");
                    return;
                };
                self.dialog = Some(Dialog::CorpusSearch {
                    query: String::new(),
                    results: Vec::new(),
                    last_error: None,
                    last_run: format!("Corpus prêt : {}", p.root.display()),
                });
            }
            PaletteAction::BackupNow => {
                self.pending.push(ModuleResponse::BackupNow);
                tracing::info!(target: "file_tree", "Backup manuel demandé.");
            }
            PaletteAction::BackupShowDir => {
                self.pending.push(ModuleResponse::OpenBackupDir);
            }
            PaletteAction::CockpitOpen => {
                self.pending
                    .push(ModuleResponse::OpenModuleWindow("cockpit".to_string()));
            }
            PaletteAction::CockpitToggle => {
                self.pending
                    .push(ModuleResponse::ToggleModuleWindow("cockpit".to_string()));
            }
            PaletteAction::WrapDriveOpen => {
                self.pending.push(ModuleResponse::OpenModuleWindow(
                    "wrapdrive_panel".to_string(),
                ));
            }
            PaletteAction::TimelineOpen => {
                self.pending
                    .push(ModuleResponse::OpenModuleWindow("timeline".to_string()));
            }
            PaletteAction::TimelineChronology => {
                self.pending.push(ModuleResponse::OpenTimelineChronology);
            }
            PaletteAction::ClaudeTerminalOpen => {
                self.pending.push(ModuleResponse::OpenModuleWindow(
                    "claude_terminal".to_string(),
                ));
            }
        }
    }

    fn run_corpus_search(
        &mut self,
        query: &str,
        results: &mut Vec<indexer::CorpusHit>,
        last_error: &mut Option<String>,
        last_run: &mut String,
    ) {
        let Some(p) = &self.proj else {
            *last_error = Some("Aucun projet ouvert.".into());
            results.clear();
            return;
        };
        match indexer::search_corpus(&p.root, query, 30) {
            Ok(hits) => {
                *last_error = None;
                *last_run = if query.trim().is_empty() {
                    format!("Requête vide sur {}", p.root.display())
                } else {
                    format!(
                        "{} résultat(s) pour '{}' dans {}",
                        hits.len(),
                        query.trim(),
                        p.root.display()
                    )
                };
                *results = hits;
            }
            Err(e) => {
                *last_error = Some(e.clone());
                *last_run = "Recherche impossible".into();
                results.clear();
                self.glados(e);
            }
        }
    }

    fn mark_structure_dirty(&mut self) {
        if let Some(p) = &mut self.proj {
            p.structure_dirty = true;
        }
    }

    /// Notifie l'indexeur après une action fichier du module lui-même.
    fn reindex(&self, path: &Path) {
        if let Some(p) = &self.proj {
            p.indexer
                .send(indexer::IndexerCmd::Reindex(path.to_path_buf()));
        }
    }

    // -----------------------------------------------------------------------
    // Dialogues modaux (nom uniquement, jamais de chemin libre).
    // -----------------------------------------------------------------------

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            self.dialog_shown_prev = false;
            return;
        };
        // Focus demandé uniquement à la première frame d'affichage du dialogue.
        let just_opened = !self.dialog_shown_prev;
        self.dialog_shown_prev = true;
        let mut keep = true;
        let mut validated = false;

        let title = match &dialog {
            Dialog::NewFile { .. } => "Nouveau fichier",
            Dialog::NewFolder { .. } => "Nouveau dossier",
            Dialog::Rename { .. } => "Renommer",
            Dialog::ConfirmDelete { .. } => "Supprimer ?",
            Dialog::NewProjectName { .. } => "Nouveau projet",
            Dialog::ProjectOpenPrompt { .. } => "Ouvrir le dossier",
            Dialog::StructurePreview { .. } => "Prévisualisation de la structure",
            Dialog::CorpusSearch { .. } => "Recherche corpus",
        };
        let dialog_root = match &dialog {
            Dialog::ProjectOpenPrompt { root, .. } | Dialog::StructurePreview { root, .. } => {
                Some(root.clone())
            }
            _ => None,
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                match &mut dialog {
                    Dialog::NewFile { name, .. }
                    | Dialog::NewFolder { name, .. }
                    | Dialog::Rename { name, .. }
                    | Dialog::NewProjectName { name, .. } => {
                        let edit = ui.text_edit_singleline(name);
                        if just_opened {
                            edit.request_focus();
                        }
                        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            validated = true;
                        }
                    }
                    Dialog::ConfirmDelete { target, non_empty } => {
                        let name = target
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if *non_empty {
                            ui.label(format!(
                                "'{name}' est un dossier NON VIDE. Tout son contenu disparaît \
                                 avec lui. Définitivement."
                            ));
                        } else {
                            ui.label(format!("Supprimer '{name}' ? C'est définitif."));
                        }
                    }
                    Dialog::ProjectOpenPrompt { root, state } => {
                        ui.label(match state {
                            project::ProjectState::Empty => "Ce dossier est vide.",
                            project::ProjectState::NonEngram => {
                                "Ce dossier n’est pas reconnu comme projet Engram."
                            }
                            project::ProjectState::Partial { .. } => {
                                "Une structure partielle a été détectée."
                            }
                            project::ProjectState::Engram => "Projet Engram reconnu.",
                        });
                        ui.label(format!("Dossier : {}", root.display()));
                    }
                    Dialog::StructurePreview {
                        root,
                        create,
                        items,
                    } => {
                        ui.label(format!(
                            "{}",
                            if *create {
                                "Créer/compléter la structure suivante ?"
                            } else {
                                "Ouvrir tel quel ?"
                            }
                        ));
                        ui.label(format!("Dossier : {}", root.display()));
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for item in items.iter() {
                                    ui.monospace(item);
                                }
                            });
                    }
                    Dialog::CorpusSearch {
                        query,
                        results,
                        last_error,
                        last_run,
                    } => {
                        ui.label("Recherche FTS dans le corpus du projet ouvert.");
                        ui.label(last_run.as_str());
                        let edit = ui.text_edit_singleline(query);
                        if just_opened {
                            edit.request_focus();
                        }
                        let want_search =
                            edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("Rechercher").clicked() || want_search {
                            let query_text = query.clone();
                            self.run_corpus_search(&query_text, results, last_error, last_run);
                        }
                        if let Some(err) = last_error.as_ref() {
                            ui.colored_label(egui::Color32::from_rgb(255, 46, 136), err);
                        }
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                if results.is_empty() {
                                    ui.weak("Aucun résultat pour l'instant.");
                                }
                                for hit in results.iter() {
                                    ui.group(|ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.button(hit.path.display().to_string()).clicked() {
                                                self.open_file(&hit.path);
                                            }
                                            ui.weak(format!(
                                                "{} · {} · {} mot(s)",
                                                hit.section, hit.file_stem, hit.words_body
                                            ));
                                        });
                                        ui.label(hit.snippet.as_str());
                                    });
                                    ui.add_space(4.0);
                                }
                            });
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| match &dialog {
                    Dialog::ProjectOpenPrompt { state, .. } => {
                        let create_label = match state {
                            project::ProjectState::Partial { .. } => "Compléter la structure",
                            _ => "Créer la structure",
                        };
                        if ui.button(create_label).clicked() {
                            let items = match state {
                                project::ProjectState::Partial { missing } => {
                                    missing.iter().map(|m| format!("manquant: {m}")).collect()
                                }
                                _ => project::structure_plan(),
                            };
                            self.dialog = Some(Dialog::StructurePreview {
                                root: dialog_root.clone().unwrap_or_default(),
                                create: true,
                                items,
                            });
                            keep = false;
                        }
                        let open_label = match state {
                            project::ProjectState::Partial { .. } => "Ouvrir tel quel",
                            _ => "Ouvrir sans structure",
                        };
                        if ui.button(open_label).clicked() {
                            if let Some(root) = dialog_root.clone() {
                                self.open_project(root, ctx, false);
                            }
                            keep = false;
                        }
                        if ui.button("Annuler").clicked() {
                            keep = false;
                        }
                    }
                    Dialog::StructurePreview { .. } => {
                        if ui.button("Créer la structure").clicked() {
                            validated = true;
                        }
                        if ui.button("Ouvrir tel quel").clicked() {
                            if let Some(root) = dialog_root.clone() {
                                self.open_project(root, ctx, false);
                            }
                            keep = false;
                        }
                        if ui.button("Annuler").clicked() {
                            keep = false;
                        }
                    }
                    Dialog::CorpusSearch { .. } => {
                        if ui.button("Fermer").clicked() {
                            keep = false;
                        }
                    }
                    Dialog::ConfirmDelete { .. } => {
                        if ui.button("Supprimer").clicked() {
                            validated = true;
                        }
                        if ui.button("Annuler").clicked() {
                            keep = false;
                        }
                    }
                    _ => {
                        if ui.button("Valider").clicked() {
                            validated = true;
                        }
                        if ui.button("Annuler").clicked() {
                            keep = false;
                        }
                    }
                });
            });

        if ui_escape(ctx) {
            keep = false;
        }

        if validated {
            self.execute_dialog(dialog, ctx);
        } else if keep {
            self.dialog = Some(dialog);
        }
    }

    fn execute_dialog(&mut self, dialog: Dialog, ctx: &egui::Context) {
        match dialog {
            Dialog::NewFile { parent, name } => match project::new_file(&parent, &name) {
                Ok(path) => {
                    self.reindex(&path);
                    self.mark_structure_dirty();
                }
                Err(e) => self.glados(e),
            },
            Dialog::NewFolder { parent, name } => match project::new_folder(&parent, &name) {
                Ok(_) => self.mark_structure_dirty(),
                Err(e) => self.glados(e),
            },
            Dialog::Rename { target, name } => match project::rename(&target, &name) {
                Ok(_) => {
                    // Les chemins canoniques ont changé : re-scan complet.
                    if let Some(p) = &self.proj {
                        p.indexer.send(indexer::IndexerCmd::FullRescan);
                    }
                    self.mark_structure_dirty();
                }
                Err(e) => self.glados(e),
            },
            Dialog::ConfirmDelete { target, .. } => {
                let canonical = std::fs::canonicalize(&target).ok();
                match project::delete(&target) {
                    Ok(()) => {
                        if let Some(p) = &self.proj {
                            match canonical {
                                Some(c) if !target.is_dir() => {
                                    p.indexer.send(indexer::IndexerCmd::Remove(c))
                                }
                                _ => p.indexer.send(indexer::IndexerCmd::FullRescan),
                            }
                        }
                        self.mark_structure_dirty();
                    }
                    Err(e) => self.glados(e),
                }
            }
            Dialog::NewProjectName { location, name } => {
                match project::create_project(&location, &name) {
                    Ok(root) => self.pending_open = Some(root),
                    Err(e) => self.glados(e),
                }
            }
            Dialog::ProjectOpenPrompt { root, state } => match state {
                project::ProjectState::Engram => self.open_project(root, ctx, false),
                project::ProjectState::Empty => {
                    self.dialog = Some(Dialog::StructurePreview {
                        root,
                        create: true,
                        items: project::structure_plan(),
                    });
                }
                project::ProjectState::NonEngram => {
                    self.dialog = Some(Dialog::StructurePreview {
                        root,
                        create: true,
                        items: project::structure_plan(),
                    });
                }
                project::ProjectState::Partial { missing } => {
                    self.dialog = Some(Dialog::StructurePreview {
                        root,
                        create: true,
                        items: missing
                            .into_iter()
                            .map(|m| format!("manquant: {m}"))
                            .collect(),
                    });
                }
            },
            Dialog::StructurePreview { root, create, .. } => {
                self.open_project(root, ctx, create);
            }
            Dialog::CorpusSearch { .. } => {}
        }
    }

    // -----------------------------------------------------------------------
    // Fenêtre principale du module.
    // -----------------------------------------------------------------------

    /// Dessine le file_tree dans un `ui` fourni (panel de la fenêtre core).
    /// Plus de CentralPanel ni de viewport : le rendu vit dans le bloc core.
    /// Palette, sélecteur et dialogues restent des `egui::Window` flottantes
    /// (overlays au niveau du contexte), via `ui.ctx()`.
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.show_picker(&ctx);
        self.show_dialog(&ctx);

        let mut menu_action = None;
        let mut open_request = None;

        // Bandeau GLaDOS : les erreurs se voient, c'est la règle.
        if !self.glados.is_empty() {
            let mut dismiss = None;
            for (i, msg) in self.glados.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(255, 46, 136), "⚠");
                    ui.label(msg.as_str());
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

        // Barre d'actions projet : toujours visible quand un projet est ouvert.
        // Sans elle (régression de la fusion core+file_tree), il fallait passer
        // par la CLI pour changer de projet.
        if self.proj.is_some() {
            ui.horizontal(|ui| {
                if ui
                    .button("📂 Ouvrir")
                    .on_hover_text("Ouvrir un projet existant")
                    .clicked()
                {
                    self.start_picker(PickerPurpose::OpenProject);
                }
                if ui
                    .button("✨ Nouveau")
                    .on_hover_text("Créer un nouveau projet")
                    .clicked()
                {
                    self.start_picker(PickerPurpose::NewProjectLocation);
                }
            });
            ui.separator();
        }

        match &self.proj {
            None => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    if ui.button("📂 Ouvrir un projet").clicked() {
                        self.start_picker(PickerPurpose::OpenProject);
                    }
                    ui.add_space(4.0);
                    if ui.button("✨ Nouveau projet").clicked() {
                        self.start_picker(PickerPurpose::NewProjectLocation);
                    }
                    ui.add_space(10.0);
                    ui.weak("Ctrl+Shift+P ouvre la palette globale.");
                });
            }
            Some(p) => {
                if let Some((tree, totals)) = &p.tree {
                    let goal = p.config.goal_words;
                    // Couleurs : config file_tree (folder_color, en_cours_symlink_color)
                    // si renseignées, sinon fallback sur la palette du thème core.
                    let pal = self.ctx.as_ref().map(|c| &c.theme);
                    let accent =
                        pal.map_or(egui::Color32::from_rgb(157, 0, 255), |p| p.accent_secondary);
                    let fg = pal.map_or(egui::Color32::from_rgb(255, 46, 136), |p| p.foreground);
                    let folder = parse_hex_color(&self.cfg.vomi.folder_color).unwrap_or(accent);
                    let symlink = parse_hex_color(&self.cfg.vomi.en_cours_symlink_color)
                        .unwrap_or_else(|| accent.linear_multiply(0.6));
                    let colors = tree_view::TreeColors {
                        folder,
                        file: fg,
                        symlink,
                        guide: accent.linear_multiply(0.3),
                    };
                    let out = tree_view::show(
                        ui,
                        tree,
                        totals,
                        goal,
                        &mut self.ui_state,
                        &self.cfg,
                        &colors,
                    );
                    if let Some(path) = out.open {
                        open_request = Some(path);
                    }
                    if let Some(a) = out.menu_action {
                        menu_action = Some(a);
                    }
                } else {
                    ui.spinner();
                    ui.label("Scan initial en cours…");
                }
                // Pied de page : statut indexeur (progression, reconstruction).
                if let Some(msg) = p
                    .indexer
                    .shared
                    .status_msg
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone()
                {
                    ui.separator();
                    ui.weak(msg);
                }
            }
        }

        if let Some(path) = open_request {
            self.open_file(&path);
        }
        if let Some(a) = menu_action {
            self.run_menu_action(a, &ctx);
        }

        self.save_session_throttled();
    }

    /// Persiste la session locale au plus toutes les 2 secondes.
    /// Plus de suivi d'attache : le file_tree n'a plus de fenêtre propre
    /// (embarqué dans le core).
    fn save_session_throttled(&mut self) {
        if let Some(p) = &mut self.proj {
            if p.session_dirty && p.last_session_save.elapsed() > Duration::from_secs(2) {
                if let Err(e) = p.session.save(&p.root) {
                    tracing::warn!(target: "file_tree", "{e}");
                }
                p.session_dirty = false;
                p.last_session_save = Instant::now();
            }
        }
    }
}

/// Échap pressé ? (helper hors de la closure du dialogue pour éviter les
/// conflits d'emprunt).
/// "#rrggbb" → Color32. None si vide ou invalide (caller fournit un fallback).
fn parse_hex_color(s: &str) -> Option<egui::Color32> {
    let hex = s.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(egui::Color32::from_rgb(
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    ))
}

fn ui_escape(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

impl Module for FileTreeModule {
    fn name(&self) -> &'static str {
        "file_tree"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        let (cfg, errors) = config::Config::load(&ctx.config_dir, &ctx.licorne);
        self.cfg = cfg;
        for e in errors {
            self.glados(e);
        }
        self.ctx = Some(ctx.clone());
        // Réouverture du dernier projet (différée : l'indexeur a besoin du
        // contexte egui, disponible seulement à la première frame).
        if self.cfg.simple.reopen_last_project {
            self.pending_open = project::load_last_project(&self.cfg.module_config_dir);
        }
        Ok(())
    }

    /// Module embarqué : `update` ne fait QUE la logique (ouverture de projet
    /// différée, rafraîchissement de l'arbre). Le rendu est fait par le core
    /// via `draw_embedded`. Les réponses produites sont remontées tout de suite.
    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        if let Some(root) = self.pending_open.take() {
            self.open_project(root, egui_ctx, true);
        }
        self.refresh_tree();
        out.append(&mut self.pending);
    }

    fn render_mode(&self) -> engram_core::RenderMode {
        engram_core::RenderMode::EmbeddedInCore
    }

    /// Dessine le file_tree dans le panel du core. Masqué en mode focus de
    /// l'éditeur (état conservé : il réapparaît à la sortie du focus).
    fn draw_embedded(&mut self, ui: &mut egui::Ui, out: &mut Vec<ModuleResponse>) {
        if self.hidden_by_focus {
            return;
        }
        self.draw(ui);
        out.append(&mut self.pending);
    }

    fn handle_event(&mut self, event: &CoreEvent) {
        // Mode focus de l'éditeur : le panel file_tree se masque, état intact.
        if let CoreEvent::FocusModeChanged(active) = event {
            self.hidden_by_focus = *active;
        }
        if let CoreEvent::IpcCommand(cmd) = event {
            if cmd.module == self.name() && cmd.action == "palette" {
                if let Some(action) = cmd
                    .value
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .and_then(PaletteAction::from_command)
                {
                    self.run_palette_action(action);
                }
            }
        }
        // Le reste (OpenFileRequested, FileIndexUpdated…) concerne d'autres
        // modules.
    }

    fn shutdown(&mut self) {
        if let Some(p) = &mut self.proj {
            if let Err(e) = p.session.save(&p.root) {
                tracing::warn!(target: "file_tree", "Sauvegarde session : {e}");
            }
        }
        // L'indexeur s'arrête proprement via Drop (Shutdown + join).
        self.proj = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenKind {
    EngramText,
    Okular,
    DefaultApp,
}

fn open_kind(path: &Path) -> OpenKind {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "typ"
        || matches!(
            ext.as_str(),
            "md" | "txt" | "ron" | "toml" | "sql" | "csv" | "tsv" | "json"
        )
    {
        OpenKind::EngramText
    } else if ext == "pdf" {
        OpenKind::Okular
    } else {
        OpenKind::DefaultApp
    }
}

fn open_in_default_app(path: &Path) -> Result<(), String> {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            format!(
                "Ouverture de {} avec {cmd} impossible : {e}",
                path.display()
            )
        })
}

fn open_with_cmd(cmd: &str, path: &Path) -> Result<(), String> {
    std::process::Command::new(cmd)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            format!(
                "Ouverture de {} avec {cmd} impossible : {e}",
                path.display()
            )
        })
}

// ===========================================================================
// CLI headless : les actions de la palette invocables depuis le terminal
// (engram_hive tree new-file …, context add …, context clear).
// Opèrent sur le dernier projet ouvert.
// ===========================================================================

fn cli_project_root() -> Result<PathBuf, String> {
    let config_dir = dirs::config_dir()
        .ok_or("Pas de dossier de config OS.")?
        .join("engram_hive_typst");
    let module_dir = config_dir.join("modules").join("file_tree");
    project::load_last_project(&module_dir).ok_or_else(|| {
        "Aucun dernier projet connu. Lance l'app graphique et ouvre un projet d'abord.".into()
    })
}

/// `engram_hive project open --path <dossier>` : désigne le projet à ouvrir
/// au prochain lancement de la GUI, SANS passer par l'explorateur natif.
/// L'échappatoire quand aucun portail XDG n'est disponible (les boutons
/// "Ouvrir/Nouveau projet" restent alors sans effet). Le dossier est
/// mémorisé comme "dernier projet" ; la GUI le rouvre (et le scaffolde s'il
/// n'est pas encore un projet Engram_Hive).
pub fn cli_set_project(path: &str) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(path)
        .map_err(|e| format!("'{path}' : {e}. Vérifie le chemin du dossier projet."))?;
    if !root.is_dir() {
        return Err(format!("'{}' n'est pas un dossier.", root.display()));
    }
    let config_dir = dirs::config_dir()
        .ok_or("Pas de dossier de config OS.")?
        .join("engram_hive_typst");
    let module_dir = config_dir.join("modules").join("file_tree");
    std::fs::create_dir_all(&module_dir)
        .map_err(|e| format!("Impossible de créer {} : {e}", module_dir.display()))?;
    project::save_last_project(&module_dir, &root);
    Ok(root)
}

/// `engram_hive tree new-file --path <dossier relatif>` : crée "nouveau.typ".
pub fn cli_new_file(rel_dir: &str, name: &str) -> Result<String, String> {
    let root = cli_project_root()?;
    let parent = root.join(rel_dir);
    if !parent.is_dir() {
        return Err(format!(
            "'{rel_dir}' n'est pas un dossier du projet {}.",
            root.display()
        ));
    }
    let path = project::new_file(&parent, name)?;
    Ok(format!("Créé : {}", path.display()))
}

/// `engram_hive context add --file <chemin relatif>`.
pub fn cli_context_add(rel_file: &str) -> Result<String, String> {
    let root = cli_project_root()?;
    let target = root.join(rel_file);
    let link = symlinks::add_to_en_cours(&root, &target)?;
    Ok(format!("Lié : {}", link.display()))
}

/// `engram_hive context clear`.
pub fn cli_context_clear() -> Result<String, String> {
    let root = cli_project_root()?;
    let n = symlinks::clear_en_cours(&root)?;
    Ok(format!("En Cours vidé : {n} liens retirés."))
}
