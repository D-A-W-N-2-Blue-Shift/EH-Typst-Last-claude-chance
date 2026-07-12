// ============================================================================
// modules/sticky_notes/src/lib.rs — Module Sticky Notes
//
// Post-its d'écrivain ancrés dans les fichiers. Le module garde ses données
// dans la base SQLite du projet (.engram/index.db) et publie des compteurs de
// notes par fichier vers le core.
// ============================================================================

mod config;
mod db;
mod render;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse, RenderMode};

use config::Config;
use db::{NoteDraft, NoteRecord};

pub struct StickyNotesModule {
    config_dir: PathBuf,
    config: Config,
    closed: bool,
    project_root: Option<PathBuf>,
    current_source: Option<PathBuf>,
    notes: Vec<NoteRecord>,
    note_counts: BTreeMap<PathBuf, usize>,
    published_counts: BTreeMap<PathBuf, usize>,
    list_refresh_pending: bool,
    list_status: Option<String>,
    list_error: Option<String>,
    selected_note_id: Option<String>,
    note_draft: Option<NoteDraft>,
    note_original: Option<NoteRecord>,
    note_error: Option<String>,
    status: Vec<String>,
}

impl Default for StickyNotesModule {
    fn default() -> Self {
        Self {
            config_dir: PathBuf::new(),
            config: Config::default(),
            closed: true,
            project_root: None,
            current_source: None,
            notes: Vec::new(),
            note_counts: BTreeMap::new(),
            published_counts: BTreeMap::new(),
            list_refresh_pending: false,
            list_status: None,
            list_error: None,
            selected_note_id: None,
            note_draft: None,
            note_original: None,
            note_error: None,
            status: Vec::new(),
        }
    }
}

impl Module for StickyNotesModule {
    fn name(&self) -> &'static str {
        "sticky_notes"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        self.config_dir = ctx.config_dir.clone();
        let (cfg, errs) = Config::load(&self.config_dir);
        self.config = cfg;
        self.closed = self.config.closed;
        for err in errs {
            self.status.push(format!("⚠ {err}"));
        }
        Ok(())
    }

    fn render_mode(&self) -> RenderMode {
        RenderMode::OwnViewport
    }

    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        if self.list_refresh_pending {
            self.sync_project(out);
            self.list_refresh_pending = false;
        }

        if self.closed {
            return;
        }

        let list_id = egui::ViewportId::from_hash_of("sticky_notes_list");
        let list_builder = egui::ViewportBuilder::default()
            .with_title("Engram Hive — Sticky Notes")
            .with_inner_size([860.0, 620.0]);
        let mut open_palette = false;

        egui_ctx.show_viewport_immediate(list_id, list_builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                self.closed = true;
                self.persist_visibility();
                self.note_draft = None;
                self.selected_note_id = None;
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
            egui::TopBottomPanel::top("sticky_notes_status").show(ctx, |ui| {
                if !self.status.is_empty() {
                    for line in &self.status {
                        ui.label(line);
                    }
                    ui.separator();
                }
                if let Some(src) = &self.current_source {
                    ui.weak(format!("Source courante : {}", src.display()));
                }
                if let Some(msg) = &self.list_status {
                    ui.weak(msg);
                }
            });
            egui::CentralPanel::default().show(ctx, |ui| {
                let orphan_count = self.orphan_count();
                let action = render::draw_list(
                    ui,
                    &self.notes,
                    &mut self.config.list_filter_text,
                    &mut self.config.list_filter_tag_type,
                    &mut self.config.list_filter_file,
                    &mut self.config.sort_mode,
                    self.current_source.as_deref(),
                    orphan_count,
                    self.list_status.as_deref(),
                    self.list_error.as_deref(),
                );
                match action {
                    render::ListAction::None => {}
                    render::ListAction::Open(id) => self.open_note(&id),
                    render::ListAction::NewFromCurrent => self.create_note_from_current(out),
                    render::ListAction::Refresh => self.list_refresh_pending = true,
                }
            });
        });

        if open_palette {
            out.push(ModuleResponse::OpenPaletteRequested);
        }

        if self.selected_note_id.is_some() {
            self.draw_note_viewport(egui_ctx, out);
        }
    }

    fn active_viewport_count(&self) -> usize {
        let mut n = if self.closed { 0 } else { 1 };
        if self.selected_note_id.is_some() {
            n += 1;
        }
        n
    }

    fn handle_event(&mut self, event: &CoreEvent) {
        match event {
            CoreEvent::OpenModuleWindowRequested(name) if name == self.name() => {
                self.closed = false;
                self.persist_visibility();
            }
            CoreEvent::ToggleModuleWindowRequested(name) if name == self.name() => {
                self.closed = !self.closed;
                self.persist_visibility();
                if self.closed {
                    self.selected_note_id = None;
                    self.note_draft = None;
                    self.note_original = None;
                }
            }
            CoreEvent::FileIndexUpdated { project_root, .. } => {
                self.project_root = Some(project_root.clone());
                self.list_refresh_pending = true;
            }
            CoreEvent::OpenFileRequested(path) => {
                self.current_source = Some(path.clone());
            }
            CoreEvent::NoteIndexUpdated { .. } => {}
            _ => {}
        }
    }

    fn shutdown(&mut self) {
        self.persist_visibility();
    }
}

impl StickyNotesModule {
    fn persist_visibility(&mut self) {
        self.config.closed = self.closed;
        if let Err(e) = self.config.save(&self.config_dir) {
            self.status.push(format!("⚠ {e}"));
        }
    }

    fn sync_project(&mut self, out: &mut Vec<ModuleResponse>) {
        let Some(root) = self.project_root.clone() else {
            self.list_status = Some("Aucun projet ouvert.".into());
            self.list_error = None;
            self.notes.clear();
            return;
        };
        match db::sync_from_project(&root) {
            Ok(result) => {
                self.notes = result.notes;
                self.note_counts = result.note_counts;
                db::publish_counts(&self.note_counts, &mut self.published_counts, out);
                self.list_status = Some(format!(
                    "{} note(s) chargée(s) depuis {}",
                    self.notes.len(),
                    root.display()
                ));
                self.list_error = None;
                self.refresh_open_note();
            }
            Err(e) => {
                self.list_error = Some(e.clone());
                self.list_status = Some("Recherche de notes suspendue.".into());
                self.status.push(format!("⚠ {e}"));
            }
        }
    }

    fn orphan_count(&self) -> usize {
        self.notes.iter().filter(|n| n.orphan).count()
    }

    fn open_note(&mut self, id: &str) {
        self.selected_note_id = Some(id.to_string());
        match db::load_note(
            self.project_root
                .as_deref()
                .unwrap_or_else(|| Path::new(".")),
            id,
        ) {
            Ok(Some(note)) => {
                self.note_original = Some(note.clone());
                self.note_draft = Some(NoteDraft {
                    id: note.id,
                    contenu: note.contenu,
                    source_path: note.source_path,
                    anchor_line: note.anchor_line,
                    tags: note.tags,
                    links: note.links,
                });
                self.note_error = None;
            }
            Ok(None) => {
                self.note_error = Some("Note introuvable.".into());
                self.note_draft = None;
                self.note_original = None;
            }
            Err(e) => {
                self.note_error = Some(e);
                self.note_draft = None;
                self.note_original = None;
            }
        }
    }

    fn create_note_from_current(&mut self, out: &mut Vec<ModuleResponse>) {
        let Some(root) = self.project_root.clone() else {
            self.list_error = Some("Aucun projet ouvert.".into());
            return;
        };
        let mut draft = NoteDraft::new();
        draft.source_path = self.current_source.clone();
        if draft.source_path.is_none() {
            self.list_error = Some("Ouvre d'abord un fichier source pour l'ancrage.".into());
            return;
        }
        let id = draft.id.clone();
        if let Err(e) = db::save_note(&root, &draft, None) {
            self.list_error = Some(e.clone());
            self.status.push(format!("⚠ {e}"));
            return;
        }
        self.list_refresh_pending = true;
        out.push(ModuleResponse::OpenModuleWindow(self.name().to_string()));
        self.open_note(&id);
    }

    fn draw_note_viewport(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        let Some(note_id) = self.selected_note_id.clone() else {
            return;
        };
        let viewport_id = egui::ViewportId::from_hash_of(("sticky_notes_note", &note_id));
        let builder = egui::ViewportBuilder::default()
            .with_title("Engram Hive — Note")
            .with_inner_size([720.0, 540.0]);
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                self.selected_note_id = None;
                self.note_draft = None;
                self.note_original = None;
                return;
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(draft) = self.note_draft.as_mut() {
                    let action = render::draw_note(
                        ui,
                        draft,
                        self.note_original.as_ref(),
                        self.current_source.as_deref(),
                    );
                    match action {
                        render::NoteAction::None => {}
                        render::NoteAction::Save => self.save_current_note(out),
                        render::NoteAction::Delete => self.delete_current_note(),
                        render::NoteAction::Close => {
                            self.selected_note_id = None;
                            self.note_draft = None;
                            self.note_original = None;
                        }
                    }
                    if let Some(err) = &self.note_error {
                        ui.colored_label(egui::Color32::from_rgb(255, 46, 136), err);
                    }
                } else {
                    ui.label("Aucune note chargée.");
                }
            });
        });
    }

    fn save_current_note(&mut self, out: &mut Vec<ModuleResponse>) {
        let Some(root) = self.project_root.clone() else {
            self.note_error = Some("Aucun projet ouvert.".into());
            return;
        };
        let Some(draft) = self.note_draft.clone() else {
            return;
        };
        let original = self.note_original.as_ref();
        match db::save_note(&root, &draft, original) {
            Ok(()) => {
                self.note_error = None;
                self.list_refresh_pending = true;
                out.push(ModuleResponse::OpenModuleWindow(self.name().to_string()));
            }
            Err(e) => {
                self.note_error = Some(e.clone());
                self.status.push(format!("⚠ {e}"));
            }
        }
    }

    fn delete_current_note(&mut self) {
        let Some(root) = self.project_root.clone() else {
            self.note_error = Some("Aucun projet ouvert.".into());
            return;
        };
        let Some(id) = self.selected_note_id.clone() else {
            return;
        };
        match db::delete_note(&root, &id) {
            Ok(()) => {
                self.selected_note_id = None;
                self.note_draft = None;
                self.note_original = None;
                self.list_refresh_pending = true;
            }
            Err(e) => {
                self.note_error = Some(e.clone());
                self.status.push(format!("⚠ {e}"));
            }
        }
    }

    fn refresh_open_note(&mut self) {
        let Some(id) = self.selected_note_id.clone() else {
            return;
        };
        if let Some(root) = self.project_root.as_deref() {
            if let Ok(Some(note)) = db::load_note(root, &id) {
                self.note_original = Some(note.clone());
                self.note_draft = Some(NoteDraft {
                    id: note.id,
                    contenu: note.contenu,
                    source_path: note.source_path,
                    anchor_line: note.anchor_line,
                    tags: note.tags,
                    links: note.links,
                });
            }
        }
    }
}
