// ============================================================================
// modules/editor/src/lib.rs — Point d'entrée du module Éditeur
//
// L'éditeur de prose d'Engram_Hive. Pas un éditeur de code : un éditeur
// pour écrivain de romans. Un fichier ouvert = une fenêtre OS native
// (viewport egui immédiat). JAMAIS d'onglets.
//
// Ce fichier orchestre :
//   - le cycle de vie des fenêtres (spawn sur CoreEvent::OpenFileRequested,
//     fermeture avec confirmation si non sauvé, session restaurée)
//   - les buffers partagés (BufferMap : même inode = même buffer)
//   - la détection des modifications externes (notify + hash, flag
//     "c'est moi qui écris" pendant 2 s à chaque sauvegarde)
//   - l'auto-save, la sauvegarde manuelle, le rechargement
//   - les commandes IPC (engram_hive editor zoom/open/save/…)
//   - les polices (famille de prose cherchée dans ~/.config/engram_hive/
//     fonts/ puis dans les polices système)
//
// Le module ne parle au core QUE via ModuleResponse. Pour le virer :
// supprimer modules/editor/ + retirer "editor" de modules.ron.
// ============================================================================

pub mod buffer;
pub mod config;
pub mod focus_mode;
pub mod highlight;
pub mod input;
pub mod search;
pub mod snippets;
pub mod stats;
pub mod table_dialog;
pub mod table_edit_assist;
pub mod toc;
pub mod typography;
pub mod viewport;
pub mod wikilinks;

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse};

use buffer::{BufferMap, SharedBuffer, SharedBufferExt};
use viewport::FontBook;
use wikilinks::WikilinkIndex;

/// Position et taille de fenêtre restaurées depuis la session :
/// (position (x, y), taille (w, h)), chacune optionnelle.
type WindowGeom = (Option<(f32, f32)>, Option<(f32, f32)>);

/// État d'une grille de tableau active dans le viewport.
pub struct TableGridState {
    pub block: table_edit_assist::TableBlock,
    pub cells: Vec<Vec<String>>,
    pub dirty: bool,
}

// ===========================================================================
// Session (réouverture des fenêtres au relancement).
// ===========================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WindowSession {
    path: PathBuf,
    cursor: usize,
    scroll: f32,
    font_size: f32,
    pos: Option<(f32, f32)>,
    size: Option<(f32, f32)>,
    typewriter: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct EditorSession {
    windows: Vec<WindowSession>,
}

impl EditorSession {
    fn path(dir: &Path) -> PathBuf {
        dir.join("session.ron")
    }

    fn load(dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|raw| ron::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self, dir: &Path) -> Result<(), String> {
        let raw = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("Session insérialisable : {e}"))?;
        std::fs::write(Self::path(dir), raw)
            .map_err(|e| format!("Sauvegarde de session ratée : {e}"))
    }
}

// ===========================================================================
// La fenêtre éditeur. Un fichier = une fenêtre. Champs pub(crate) :
// input.rs et viewport.rs travaillent dessus.
// ===========================================================================

pub struct EditorWindow {
    pub id: u64,
    pub canonical: PathBuf,
    pub buffer: SharedBuffer,
    // Curseur / sélection (indices caractères globaux).
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub desired_x: Option<f32>,
    pub pending_vmove: Option<i32>,
    // Zoom / modes (par fenêtre, pas globaux).
    pub font_size: f32,
    pub typewriter: bool,
    pub focus: focus_mode::FocusMode,
    // Caches de rendu.
    pub layout: viewport::LayoutCache,
    pub highlight: highlight::HighlightCache,
    // Scroll.
    pub pending_scroll: Option<f32>,
    pub scroll_offset: f32,
    pub viewport_h: f32,
    pub scroll_cursor_into_view: bool,
    // Sous-états.
    pub search: search::SearchState,
    pub stats: stats::StatsState,
    pub autocomplete: wikilinks::AutocompleteState,
    pub toc: toc::TocState,
    /// Dialogue « Insérer un tableau » ouvert, le cas échéant (menu / ;table).
    pub table_dialog: Option<table_dialog::TableDialogState>,
    /// Grilles de tableaux actives dans le viewport (rendu egui::Grid).
    pub table_grids: Vec<TableGridState>,
    /// Version du buffer lors du dernier scan des blocs tableaux.
    table_grids_version: u64,
    // Divers.
    pub last_input: Instant,
    pub confirm_close: bool,
    closed: bool,
    /// Position/taille restaurées depuis la session (appliquées au spawn).
    restore: Option<WindowGeom>,
    last_rect: Option<((f32, f32), (f32, f32))>,
    request_os_focus: bool,
    last_title: String,
    /// Le buffer a changé sous nos pieds (autre fenêtre) : re-clamper.
    seen_version: u64,
}

impl EditorWindow {
    fn new(
        id: u64,
        canonical: PathBuf,
        buffer: SharedBuffer,
        font_size: f32,
        typewriter: bool,
    ) -> Self {
        let seen_version = buffer.read_buf().version;
        Self {
            id,
            canonical,
            buffer,
            cursor: 0,
            anchor: None,
            desired_x: None,
            pending_vmove: None,
            font_size,
            typewriter,
            focus: focus_mode::FocusMode::default(),
            layout: viewport::LayoutCache::default(),
            highlight: highlight::HighlightCache::default(),
            pending_scroll: None,
            scroll_offset: 0.0,
            viewport_h: 600.0,
            scroll_cursor_into_view: false,
            search: search::SearchState::default(),
            stats: stats::StatsState::default(),
            autocomplete: wikilinks::AutocompleteState::default(),
            toc: toc::TocState::default(),
            table_dialog: None,
            table_grids: Vec::new(),
            table_grids_version: 0,
            last_input: Instant::now(),
            confirm_close: false,
            closed: false,
            restore: None,
            last_rect: None,
            request_os_focus: true,
            last_title: String::new(),
            seen_version,
        }
    }

    fn file_name(&self) -> String {
        self.canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "sans nom".into())
    }
}

// ===========================================================================
// Le module.
// ===========================================================================

pub struct EditorModule {
    ctx: Option<CoreContext>,
    cfg: config::Config,
    keybinds: input::Keybinds,
    snippets: snippets::SnippetSet,
    syntaxes: Option<highlight::Syntaxes>,
    fonts: FontBook,
    fonts_installed: bool,
    buffers: BufferMap,
    windows: Vec<EditorWindow>,
    next_id: u64,
    index: WikilinkIndex,
    /// Fichiers à ouvrir (CoreEvent, IPC, session) — drainés chaque frame.
    pending_open: Vec<(PathBuf, Option<WindowSession>)>,
    /// Commandes IPC en attente.
    pending_ipc: Vec<engram_core::IpcCommand>,
    /// Réponses à pousser au core.
    pending: Vec<ModuleResponse>,
    /// Messages GLaDOS affichés en haut des fenêtres (max 4, dismissables).
    glados: Vec<String>,
    /// Watcher notify + canal d'événements.
    watcher: Option<(
        notify::RecommendedWatcher,
        Receiver<notify::Result<notify::Event>>,
    )>,
    watched: std::collections::HashSet<PathBuf>,
    /// Dernière fenêtre au focus OS (cible des commandes IPC).
    last_focused: Option<u64>,
    session_dirty: bool,
    last_session_save: Instant,
    /// L'état focus global annoncé au core (pour ne pas spammer).
    announced_focus: bool,
}

impl Default for EditorModule {
    fn default() -> Self {
        Self {
            ctx: None,
            cfg: config::Config::default(),
            keybinds: input::Keybinds::default(),
            snippets: snippets::SnippetSet::default(),
            syntaxes: None,
            fonts: FontBook::default(),
            fonts_installed: false,
            buffers: BufferMap::default(),
            windows: Vec::new(),
            next_id: 0,
            index: WikilinkIndex::default(),
            pending_open: Vec::new(),
            pending_ipc: Vec::new(),
            pending: Vec::new(),
            glados: Vec::new(),
            watcher: None,
            watched: std::collections::HashSet::new(),
            last_focused: None,
            session_dirty: false,
            last_session_save: Instant::now(),
            announced_focus: false,
        }
    }
}

impl EditorModule {
    /// Erreur GLaDOS : visible dans les fenêtres + loguée + remontée au core.
    fn glados(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        tracing::error!(target: "editor", "{msg}");
        self.pending.push(ModuleResponse::Error {
            module: "editor".into(),
            message: msg.clone(),
        });
        self.glados.push(msg);
        if self.glados.len() > 4 {
            self.glados.remove(0);
        }
    }

    // -----------------------------------------------------------------------
    // Ouverture de fenêtres.
    // -----------------------------------------------------------------------

    fn open_file(&mut self, path: &Path, restore: Option<WindowSession>) {
        let (canonical, shared) = match self.buffers.open(path) {
            Ok(x) => x,
            Err(e) => {
                self.glados(e);
                return;
            }
        };
        // Fichier déjà affiché → on donne le focus à sa fenêtre, pas de
        // doublon silencieux (Ctrl+Shift+N crée une seconde vue explicite).
        if restore.is_none() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.canonical == canonical) {
                w.request_os_focus = true;
                return;
            }
        }
        self.watch(&canonical);
        self.next_id += 1;
        let mut win = EditorWindow::new(
            self.next_id,
            canonical,
            shared,
            self.cfg.simple.font_size as f32,
            self.cfg.simple.typewriter_mode,
        );
        if let Some(s) = restore {
            let len = win.buffer.read_buf().rope.len_chars();
            win.cursor = s.cursor.min(len);
            win.pending_scroll = Some(s.scroll.max(0.0));
            win.font_size = s
                .font_size
                .clamp(self.cfg.vomi.zoom_min_pt, self.cfg.vomi.zoom_max_pt);
            win.typewriter = s.typewriter;
            win.restore = Some((s.pos, s.size));
        }
        self.windows.push(win);
        self.session_dirty = true;
    }

    /// Seconde vue sur le même buffer (validation symlink/inode : deux
    /// fenêtres, UN buffer, modifications visibles des deux côtés).
    fn open_second_view(&mut self, from_id: u64) {
        let Some(src) = self.windows.iter().find(|w| w.id == from_id) else {
            return;
        };
        let canonical = src.canonical.clone();
        let shared = std::sync::Arc::clone(&src.buffer);
        let font = src.font_size;
        let tw = src.typewriter;
        self.next_id += 1;
        self.windows
            .push(EditorWindow::new(self.next_id, canonical, shared, font, tw));
        self.session_dirty = true;
    }

    // -----------------------------------------------------------------------
    // Watcher notify (modifications externes).
    // -----------------------------------------------------------------------

    fn watch(&mut self, canonical: &Path) {
        if self.watcher.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            match notify::recommended_watcher(move |ev| {
                let _ = tx.send(ev);
            }) {
                Ok(w) => self.watcher = Some((w, rx)),
                Err(e) => {
                    self.glados(format!(
                        "Watcher impossible ({e}) : les modifications externes \
                         passeront inaperçues. Méfiance."
                    ));
                    return;
                }
            }
        }
        if self.watched.contains(canonical) {
            return;
        }
        if let Some((w, _)) = &mut self.watcher {
            use notify::Watcher as _;
            if let Err(e) = w.watch(canonical, notify::RecursiveMode::NonRecursive) {
                tracing::warn!(target: "editor", "watch {} : {e}", canonical.display());
            } else {
                self.watched.insert(canonical.to_path_buf());
            }
        }
    }

    fn poll_watcher(&mut self) {
        let mut touched: Vec<PathBuf> = Vec::new();
        if let Some((_, rx)) = &self.watcher {
            for ev in rx.try_iter().flatten() {
                touched.extend(ev.paths);
            }
        }
        let mut messages = Vec::new();
        for path in touched {
            let canonical = std::fs::canonicalize(&path).unwrap_or(path);
            let Some(buf) = self.buffers.get(&canonical) else {
                continue;
            };
            let mut b = buf.write_buf();
            if b.self_write_active() || b.external_change {
                continue; // c'est nous, ou déjà signalé.
            }
            if b.disk_really_changed() {
                b.external_change = true;
                messages.push(format!(
                    "{} a changé sur le disque pendant que tu avais le dos tourné. \
                     Ctrl+Shift+R pour recharger — tes modifications locales non \
                     sauvées seraient perdues, à toi de choisir.",
                    b.path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                ));
            }
        }
        for m in messages {
            self.glados(m);
        }
    }

    // -----------------------------------------------------------------------
    // Sauvegardes.
    // -----------------------------------------------------------------------

    fn save_buffer(&mut self, shared: &SharedBuffer) {
        let silence = Duration::from_secs(self.cfg.vomi.self_write_silence_secs.max(1));
        let res = shared.write_buf().save(silence);
        if let Err(e) = res {
            self.glados(e);
        }
    }

    fn autosave(&mut self) {
        if self.cfg.simple.autosave_minutes == 0 {
            return;
        }
        let max_age = Duration::from_secs(self.cfg.simple.autosave_minutes * 60);
        let due: Vec<SharedBuffer> = self
            .buffers
            .iter()
            .filter(|(_, b)| {
                let b = b.read_buf();
                b.dirty && !b.external_change && b.last_save.elapsed() >= max_age
            })
            .map(|(_, b)| std::sync::Arc::clone(b))
            .collect();
        for b in due {
            self.save_buffer(&b);
            tracing::info!(target: "editor", "Auto-save : {}", b.read_buf().path.display());
        }
    }

    // -----------------------------------------------------------------------
    // IPC.
    // -----------------------------------------------------------------------

    fn run_ipc(&mut self, cmd: engram_core::IpcCommand) {
        let target = self
            .last_focused
            .and_then(|id| self.windows.iter().position(|w| w.id == id))
            .or(if self.windows.is_empty() {
                None
            } else {
                Some(0)
            });
        match cmd.action.as_str() {
            "open" => {
                if let Some(p) = cmd.path {
                    self.pending_open.push((p, None));
                } else {
                    self.glados("IPC editor open : pas de chemin. Ouvrir quoi, le néant ?");
                }
            }
            "zoom" => {
                let Some(pct) = cmd.value.as_ref().and_then(|v| v.as_f64()) else {
                    self.glados("IPC editor zoom : valeur manquante ou non numérique.");
                    return;
                };
                if let Some(i) = target {
                    let base = self.cfg.simple.font_size as f32;
                    self.windows[i].font_size = (base * pct as f32 / 100.0)
                        .clamp(self.cfg.vomi.zoom_min_pt, self.cfg.vomi.zoom_max_pt);
                    self.session_dirty = true;
                }
            }
            "focus-mode" => {
                if let Some(i) = target {
                    let active = self.windows[i].focus.toggle();
                    let _ = active;
                }
            }
            "typewriter" => {
                if let Some(i) = target {
                    self.windows[i].typewriter = !self.windows[i].typewriter;
                    self.session_dirty = true;
                }
            }
            "save" => {
                if let Some(i) = target {
                    let b = std::sync::Arc::clone(&self.windows[i].buffer);
                    self.save_buffer(&b);
                }
            }
            other => {
                self.glados(format!(
                    "IPC editor : action inconnue '{other}'. J'accepte open, zoom, \
                     focus-mode, typewriter, save."
                ));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Session.
    // -----------------------------------------------------------------------

    fn snapshot_session(&self) -> EditorSession {
        EditorSession {
            windows: self
                .windows
                .iter()
                .map(|w| WindowSession {
                    path: w.canonical.clone(),
                    cursor: w.cursor,
                    scroll: w.scroll_offset,
                    font_size: w.font_size,
                    pos: w.last_rect.map(|(p, _)| p),
                    size: w.last_rect.map(|(_, s)| s),
                    typewriter: w.typewriter,
                })
                .collect(),
        }
    }

    fn save_session(&mut self) {
        let session = self.snapshot_session();
        if let Err(e) = session.save(&self.cfg.module_config_dir) {
            tracing::warn!(target: "editor", "{e}");
        }
        self.session_dirty = false;
        self.last_session_save = Instant::now();
    }
}

// ===========================================================================
// Le trait Module.
// ===========================================================================

impl Module for EditorModule {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn active_viewport_count(&self) -> usize {
        self.windows.len()
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        let (cfg, errors) = config::Config::load(&ctx.config_dir, &ctx.theme, &ctx.licorne);
        self.cfg = cfg;
        for e in errors {
            self.glados(e);
        }
        let mut basic_errors = Vec::new();
        let basic: config::Basic = ctx.licorne.section("basic", &mut basic_errors);
        for e in basic_errors {
            self.glados(e);
        }
        let clamped = basic.font_size.clamp(8, 72);
        if clamped != basic.font_size {
            self.glados(format!(
                "basic.font_size = {} hors borne ; borné à {} pour l'affichage.",
                basic.font_size, clamped
            ));
        }
        self.cfg.simple.font_size = clamped;
        let (keybinds, errors) = input::Keybinds::load(&ctx.config_dir);
        self.keybinds = keybinds;
        for e in errors {
            self.glados(e);
        }
        let (snippets, errors) = snippets::SnippetSet::load(&ctx.config_dir);
        self.snippets = snippets;
        for e in errors {
            self.glados(e);
        }
        self.syntaxes = Some(highlight::Syntaxes::load());
        self.ctx = Some(ctx.clone());
        // Session : les fichiers rouverts au lancement.
        let session = EditorSession::load(&self.cfg.module_config_dir);
        for w in session.windows {
            self.pending_open.push((w.path.clone(), Some(w)));
        }
        Ok(())
    }

    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        if !self.fonts_installed {
            self.fonts = install_fonts(egui_ctx, &self.cfg);
            self.fonts_installed = true;
        }

        // Ouvertures en attente (CoreEvent, IPC, session).
        for (path, restore) in std::mem::take(&mut self.pending_open) {
            self.open_file(&path, restore);
        }
        // Commandes IPC.
        for cmd in std::mem::take(&mut self.pending_ipc) {
            self.run_ipc(cmd);
        }
        self.poll_watcher();
        self.autosave();

        // --- Tour des fenêtres. ---
        let mut actions = WindowActions::default();
        {
            let Self {
                windows,
                cfg,
                keybinds,
                snippets,
                syntaxes,
                fonts,
                index,
                glados,
                last_focused,
                ..
            } = self;
            // init() initialise toujours `syntaxes` avant le premier update ;
            // si absent (jamais observé), on saute le rendu des fenêtres cette
            // frame plutôt que de paniquer. Doctrine §5/§6 : zéro expect().
            if let Some(syn) = syntaxes.as_ref() {
                for win in windows.iter_mut() {
                    draw_window(
                        egui_ctx,
                        win,
                        cfg,
                        keybinds,
                        snippets,
                        syn,
                        fonts,
                        index,
                        glados,
                        last_focused,
                        &mut actions,
                    );
                }
            }
        }

        // Actions différées (mutations module-level hors de l'emprunt fenêtres).
        for shared in actions.save {
            self.save_buffer(&shared);
        }
        for id in actions.new_view {
            self.open_second_view(id);
        }
        for (name, valid) in actions.open_link {
            if valid {
                if let Some(p) = self.index.resolve(&name) {
                    self.pending.push(ModuleResponse::OpenFile(p));
                }
            } else if wikilinks::can_create_orphan(&self.index) {
                let path = self.index.orphan_path(&name);
                self.pending
                    .push(ModuleResponse::CreateAndOpenFile { path });
            } else {
                self.glados(format!(
                    "[[{name}]] est orphelin et aucun projet n'est ouvert : je ne \
                     sais pas où ranger sa création. Ouvre un projet dans le file tree."
                ));
            }
        }
        for e in actions.glados {
            self.glados(e);
        }
        if actions.session_dirty {
            self.session_dirty = true;
        }
        if actions.open_palette {
            self.pending.push(ModuleResponse::OpenPaletteRequested);
        }

        // Fenêtres fermées : on retire, on libère les buffers orphelins.
        let before = self.windows.len();
        self.windows.retain(|w| !w.closed);
        if self.windows.len() != before {
            self.buffers.drop_orphans();
            self.session_dirty = true;
        }

        // Mode focus global : annoncé au core quand il change (le file_tree
        // se masque/réapparaît).
        let any_focus = self.windows.iter().any(|w| w.focus.active);
        if any_focus != self.announced_focus {
            self.announced_focus = any_focus;
            self.pending
                .push(ModuleResponse::FocusModeChanged(any_focus));
        }

        // Session : sauvegarde au plus toutes les 2 s.
        if self.session_dirty && self.last_session_save.elapsed() > Duration::from_secs(2) {
            self.save_session();
        }

        out.append(&mut self.pending);
    }

    fn handle_event(&mut self, event: &CoreEvent) {
        match event {
            CoreEvent::OpenFileRequested(path) => {
                self.pending_open.push((path.clone(), None));
            }
            CoreEvent::FileIndexUpdated {
                project_root,
                files,
            } => {
                self.index = WikilinkIndex {
                    project_root: project_root.clone(),
                    files: std::sync::Arc::clone(files),
                };
            }
            CoreEvent::IpcCommand(cmd) if cmd.module == "editor" => {
                self.pending_ipc.push(cmd.clone());
            }
            _ => {}
        }
    }

    fn shutdown(&mut self) {
        // Sauvegarde de session, puis des buffers modifiés (on ne perd RIEN
        // sur un quit : philosophie auto-save d'un outil d'écriture).
        self.save_session();
        let dirty: Vec<SharedBuffer> = self
            .buffers
            .iter()
            .filter(|(_, b)| b.read_buf().dirty)
            .map(|(_, b)| std::sync::Arc::clone(b))
            .collect();
        for b in dirty {
            self.save_buffer(&b);
        }
    }
}

// ===========================================================================
// Le dessin d'UNE fenêtre (viewport immédiat).
// ===========================================================================

/// Mutations module-level décidées pendant le tour des fenêtres.
#[derive(Default)]
struct WindowActions {
    save: Vec<SharedBuffer>,
    new_view: Vec<u64>,
    open_link: Vec<(String, bool)>,
    glados: Vec<String>,
    session_dirty: bool,
    /// §7 — l'utilisateur a pressé Ctrl+Shift+P depuis un viewport éditeur ;
    /// on demande au core d'ouvrir la palette globale.
    open_palette: bool,
}

#[allow(clippy::too_many_arguments)]
fn draw_window(
    egui_ctx: &egui::Context,
    win: &mut EditorWindow,
    cfg: &config::Config,
    keybinds: &input::Keybinds,
    snippets: &snippets::SnippetSet,
    syn: &highlight::Syntaxes,
    fonts: &FontBook,
    index: &WikilinkIndex,
    glados: &mut Vec<String>,
    last_focused: &mut Option<u64>,
    actions: &mut WindowActions,
) {
    // Un fichier = une fenêtre OS : l'identité du viewport suit le brief.
    let viewport_id = egui::ViewportId::from_hash_of(format!("editor_{}", win.id));
    let dirty = win.buffer.read_buf().dirty;
    let title = format!(
        "{}{} — Engram Hive",
        win.file_name(),
        if dirty { " *" } else { "" }
    );
    let mut builder = egui::ViewportBuilder::default()
        .with_title(&title)
        .with_inner_size([900.0, 700.0]);
    if let Some((pos, size)) = win.restore.take() {
        if let Some((x, y)) = pos {
            builder = builder.with_position([x, y]);
        }
        if let Some((w, h)) = size {
            builder = builder.with_inner_size([w.max(300.0), h.max(200.0)]);
        }
    }

    let fullscreen_cmd = win.focus.pending_fullscreen.take();
    let request_focus = std::mem::take(&mut win.request_os_focus);

    egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
        if request_focus {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
        }
        if let Some(fs) = fullscreen_cmd {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Fullscreen(fs));
        }
        // Titre vivant (l'astérisque suit l'état dirty).
        if win.last_title != title {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Title(title.clone()));
            win.last_title = title.clone();
        }

        // Focus OS : cette fenêtre devient la cible des commandes IPC.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        if focused {
            *last_focused = Some(win.id);
        }

        // §7 — Ctrl+Shift+P depuis n'importe quelle fenêtre éditeur : on
        // demande au core d'ouvrir la command palette globale (le file_tree
        // l'héberge). Consommer la touche évite qu'un raccourci concurrent
        // du buffer ne la voie aussi.
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                egui::Key::P,
            )
        }) {
            actions.open_palette = true;
        }

        // Fermeture demandée par l'OS.
        if ctx.input(|i| i.viewport().close_requested()) && !win.closed {
            let dirty = win.buffer.read_buf().dirty;
            if dirty && !win.confirm_close {
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::CancelClose);
                win.confirm_close = true;
            } else {
                win.closed = true;
            }
        }

        // Curseur/version : le buffer a pu être modifié par une autre vue.
        {
            let buf = win.buffer.read_buf();
            if buf.version != win.seen_version {
                win.seen_version = buf.version;
                win.cursor = win.cursor.min(buf.rope.len_chars());
                if let Some(a) = win.anchor {
                    win.anchor = Some(a.min(buf.rope.len_chars()));
                }
            }
        }

        // --- Entrées clavier. ---
        let input_out = win.handle_input(ctx, cfg, keybinds, snippets, index);
        if input_out.save {
            actions.save.push(std::sync::Arc::clone(&win.buffer));
        }
        if input_out.reload {
            let res = win.buffer.write_buf().reload();
            match res {
                Ok(()) => {
                    let len = win.buffer.read_buf().rope.len_chars();
                    win.cursor = win.cursor.min(len);
                    win.anchor = None;
                }
                Err(e) => actions.glados.push(e),
            }
        }
        if input_out.toggle_focus {
            win.focus.toggle();
            actions.session_dirty = true;
        }
        if input_out.new_view {
            actions.new_view.push(win.id);
        }
        if input_out.toggle_toc {
            win.toc.toggle();
        }

        // Ticks (recherche débouncée par version, stats débouncées).
        {
            let buf = win.buffer.read_buf();
            win.search.tick(&buf.rope, buf.version);
            win.stats.tick(
                &buf.rope,
                buf.version,
                &cfg.vomi.comment_prefix,
                cfg.vomi.stats_debounce_ms,
            );
        }

        // --- Bandeau GLaDOS (module-level, visible partout). ---
        if !glados.is_empty() {
            egui::TopBottomPanel::top(egui::Id::new(("editor_glados", win.id))).show(ctx, |ui| {
                let mut dismiss = None;
                for (i, msg) in glados.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(255, 46, 136), "⚠");
                        ui.label(msg.as_str());
                        if ui.small_button("✕").clicked() {
                            dismiss = Some(i);
                        }
                    });
                }
                if let Some(i) = dismiss {
                    glados.remove(i);
                }
            });
        }

        // --- Panel Sommaire (Ctrl+Shift+O / menu contextuel). ---
        if win.toc.open {
            {
                let buf = win.buffer.read_buf();
                win.toc.refresh(&buf.rope);
            }
            if let Some(line) = toc::show(ctx, &mut win.toc, &cfg.theme) {
                // Scroll vers le titre + place le curseur à son début.
                let y = win.layout.y_of_line(line);
                win.pending_scroll = Some(y);
                let cstart = win.buffer.read_buf().rope.line_to_char(line);
                win.cursor = cstart;
                win.anchor = None;
                win.scroll_cursor_into_view = false;
                win.last_input = Instant::now();
            }
        }

        // --- Dialogue « Insérer un tableau » (menu contextuel / ;table). ---
        // On capture l'issue avant d'agir : `show` emprunte le dialogue, mais
        // l'insertion emprunte `win` — on sépare les deux.
        let table_outcome = win
            .table_dialog
            .as_mut()
            .and_then(|s| table_dialog::show(ctx, s));
        if let Some(outcome) = table_outcome {
            win.table_dialog = None;
            if let table_dialog::TableOutcome::Insert {
                typst,
                cursor_offset,
            } = outcome
            {
                win.insert_table(&typst, cursor_offset);
                win.refresh_autocomplete(cfg, index);
            }
        }

        // --- Panel de recherche (Ctrl+F / Ctrl+H). ---
        if win.search.open {
            draw_search_panel(ctx, win, actions);
        }

        // --- Status bar. ---
        let show_status = !win.focus.active || cfg.vomi.focus_show_status_bar;
        if show_status {
            draw_status_bar(ctx, win, cfg);
        }

        // §2 — Formulaire frontmatter custom supprimé (décision brief 2/7/2026).
        // Les fiches personnage sont désormais éditées directement dans le
        // buffer comme n'importe quel fichier avec frontmatter ; la coloration
        // (highlight::detect_frontmatter) reste appliquée dans le rendu du
        // texte pour distinguer `clé: valeur` du corps narratif.

        // --- Zone de texte. ---
        let area_out = egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let buf = win.buffer.read_buf();
                if buf.version != win.table_grids_version {
                    win.table_grids_version = buf.version;
                    let blocks = table_edit_assist::detect_all_tables(&buf.rope);
                    win.table_grids = blocks
                        .into_iter()
                        .map(|block| {
                            let cells = table_edit_assist::parse_table_cells(&buf.rope, &block);
                            TableGridState {
                                block,
                                cells,
                                dirty: false,
                            }
                        })
                        .collect();
                }
                let mut params = viewport::TextAreaParams {
                    window_id: win.id,
                    cursor: &mut win.cursor,
                    anchor: &mut win.anchor,
                    font_size: win.font_size,
                    layout: &mut win.layout,
                    highlight: &mut win.highlight,
                    pending_scroll: &mut win.pending_scroll,
                    last_input: win.last_input,
                    search_ranges: if win.search.open {
                        &win.search.ranges
                    } else {
                        &[]
                    },
                    search_current: if win.search.open {
                        Some(win.search.current)
                    } else {
                        None
                    },
                    focus_mode: win.focus.active,
                    pending_vmove: &mut win.pending_vmove,
                    desired_x: &mut win.desired_x,
                    scroll_cursor_into_view: &mut win.scroll_cursor_into_view,
                    typewriter: win.typewriter,
                    typewriter_position: cfg.vomi.typewriter_position,
                    focused,
                    table_grids: &mut win.table_grids,
                };
                viewport::show_text_area(ui, &mut params, &buf, cfg, syn, fonts, index)
            })
            .inner;

        // Sérialise les grilles de tableaux modifiées vers le buffer.
        {
            let dirty_grids: Vec<_> = win
                .table_grids
                .iter()
                .enumerate()
                .filter(|(_, g)| g.dirty)
                .map(|(i, g)| (i, g.block.clone(), g.cells.clone()))
                .collect();
            if !dirty_grids.is_empty() {
                let mut buf_w = win.buffer.write_buf();
                for (_idx, block, cells) in dirty_grids.iter().rev() {
                    let new_text = table_edit_assist::serialize_table(&cells, block.cells_per_row);
                    buf_w.begin_txn(buffer::TxnKind::Other, win.cursor);
                    buf_w.replace_line_range(block.first_line, block.last_line, &new_text);
                    buf_w.commit_txn();
                }
                drop(buf_w);
                win.table_grids_version = 0;
            }
        }

        win.scroll_offset = area_out.scroll_offset;
        win.viewport_h = area_out.viewport_height;
        if area_out.cursor_moved {
            win.last_input = Instant::now();
            win.desired_x = None;
            // Le curseur a bougé : l'auto-complétion suit ou se ferme.
            win.refresh_autocomplete(cfg, index);
        }
        if let Some(link) = area_out.clicked_wikilink {
            actions.open_link.push(link);
        }
        if let Some(action) = area_out.menu_action {
            if win.apply_edit_action(action, ctx) {
                win.refresh_autocomplete(cfg, index);
            }
        }
        if area_out.needs_repaint {
            ctx.request_repaint();
        }

        // --- Popup d'auto-complétion. ---
        if win.autocomplete.open {
            draw_autocomplete(ctx, win, area_out.cursor_screen, cfg, index);
        }

        // --- Dialogue de fermeture (fichier modifié non sauvé). ---
        if win.confirm_close {
            draw_confirm_close(ctx, win, actions);
        }

        // Suivi de la géométrie pour la session.
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let geom = ((rect.min.x, rect.min.y), (rect.width(), rect.height()));
            if win.last_rect != Some(geom) {
                win.last_rect = Some(geom);
                actions.session_dirty = true;
            }
        }

        // Le clignotement du curseur a besoin d'un battement de cœur.
        ctx.request_repaint_after(Duration::from_millis(cfg.vomi.cursor_blink_ms.max(50)));
    });
}

fn draw_search_panel(ctx: &egui::Context, win: &mut EditorWindow, actions: &mut WindowActions) {
    egui::TopBottomPanel::top(egui::Id::new(("editor_search", win.id))).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut win.search.query)
                    .hint_text("chercher…")
                    .desired_width(220.0),
            );
            if std::mem::take(&mut win.search.want_focus) {
                field.request_focus();
            }
            // Entrée dans le champ : occurrence suivante.
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                win.jump_to_match(true);
                field.request_focus();
            }
            if ui
                .button("‹")
                .on_hover_text("précédent (Shift+F3)")
                .clicked()
            {
                win.jump_to_match(false);
            }
            if ui.button("›").on_hover_text("suivant (F3)").clicked() {
                win.jump_to_match(true);
            }
            let counter = if win.search.ranges.is_empty() {
                "0/0".to_string()
            } else {
                format!("{}/{}", win.search.current + 1, win.search.ranges.len())
            };
            ui.weak(counter);
            ui.toggle_value(&mut win.search.case_sensitive, "Aa")
                .on_hover_text("respecter la casse");
            ui.toggle_value(&mut win.search.whole_word, "mot")
                .on_hover_text("mot entier");
            ui.toggle_value(&mut win.search.use_regex, ".*")
                .on_hover_text("expression régulière");
            if ui.button("✕").clicked() {
                win.search.close();
            }
        });
        if let Some(err) = &win.search.error {
            ui.colored_label(egui::Color32::from_rgb(255, 120, 120), err);
        }
        if win.search.replace_open {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut win.search.replace)
                        .hint_text("remplacer par…")
                        .desired_width(220.0),
                );
                if ui.button("remplacer").clicked() {
                    let mut buf = win.buffer.write_buf();
                    if let Some(c) = win.search.replace_current(&mut buf) {
                        drop(buf);
                        win.cursor = c;
                        win.anchor = None;
                        win.scroll_cursor_into_view = true;
                    }
                }
                if ui.button("remplacer tout").clicked() {
                    let mut buf = win.buffer.write_buf();
                    let n = win.search.replace_all(&mut buf);
                    drop(buf);
                    actions.glados.push(format!(
                        "{n} remplacement(s). Un seul Ctrl+Z les annule tous, si tu regrettes."
                    ));
                }
            });
        }
    });
}

fn draw_status_bar(ctx: &egui::Context, win: &mut EditorWindow, cfg: &config::Config) {
    egui::TopBottomPanel::bottom(egui::Id::new(("editor_status", win.id))).show(ctx, |ui| {
        let (line, col, len_chars, sel, version) = {
            let buf = win.buffer.read_buf();
            let rope = &buf.rope;
            let c = win.cursor.min(rope.len_chars());
            let line = rope.char_to_line(c);
            let col = c - rope.line_to_char(line);
            (
                line + 1,
                col + 1,
                rope.len_chars(),
                win.selection(),
                buf.version,
            )
        };
        let minimal = win.focus.active && cfg.vomi.focus_minimal_status;
        let color = cfg.theme.status_bar;
        ui.horizontal(|ui| {
            let mut parts: Vec<String> = Vec::new();
            parts.push(format!("mots: {}", win.stats.words));
            if !minimal {
                parts.push(format!("caractères: {len_chars}"));
                parts.push(format!("L{line} C{col}"));
                let delta = win.stats.session_delta();
                parts.push(format!(
                    "session: {}{delta}",
                    if delta >= 0 { "+" } else { "" }
                ));
            }
            if let Some(sel) = sel {
                let words = {
                    let buf = win.buffer.read_buf();
                    win.stats.selection_words(&buf.rope, sel, version)
                };
                parts.push(format!("sél: {words} mots"));
            }
            ui.colored_label(color, parts.join("  |  "));
            if let Some(goal) = win.stats.meta.goal {
                let reached = win.stats.words >= goal;
                let bar = stats::progress_bar(win.stats.words, goal);
                let bar_color = if reached {
                    cfg.theme.goal_reached
                } else {
                    color
                };
                ui.colored_label(bar_color, bar);
                if win.stats.goal_banner() {
                    ui.colored_label(cfg.theme.goal_reached, "Objectif atteint.");
                }
            }
            if win.typewriter && !minimal {
                ui.weak("⌨ typewriter");
            }
        });
    });
}

fn draw_autocomplete(
    ctx: &egui::Context,
    win: &mut EditorWindow,
    anchor: Option<egui::Pos2>,
    cfg: &config::Config,
    _index: &WikilinkIndex,
) {
    let Some(pos) = anchor else { return };
    let mut accept: Option<usize> = None;
    egui::Area::new(egui::Id::new(("editor_autocomplete", win.id)))
        .order(egui::Order::Foreground)
        .fixed_pos(pos + egui::vec2(0.0, 4.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(320.0);
                for (i, cand) in win.autocomplete.candidates.iter().enumerate() {
                    let selected = i == win.autocomplete.selected;
                    let resp = ui.selectable_label(selected, cand);
                    if resp.clicked() {
                        accept = Some(i);
                    }
                }
                ui.weak("↑↓ naviguer · Entrée valider · Échap fermer");
            });
        });
    if let Some(i) = accept {
        win.autocomplete.selected = i;
        win.accept_completion();
        win.scroll_cursor_into_view = true;
    }
    let _ = cfg;
}

fn draw_confirm_close(ctx: &egui::Context, win: &mut EditorWindow, actions: &mut WindowActions) {
    egui::Window::new("Fermer sans sauvegarder ?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "{} a des modifications non sauvées.",
                win.file_name()
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Enregistrer et fermer").clicked() {
                    actions.save.push(std::sync::Arc::clone(&win.buffer));
                    win.confirm_close = false;
                    win.closed = true;
                }
                if ui.button("Fermer sans enregistrer").clicked() {
                    win.confirm_close = false;
                    win.closed = true;
                }
                if ui.button("Annuler").clicked() {
                    win.confirm_close = false;
                }
            });
        });
}

// ===========================================================================
// Polices : famille de prose cherchée dans ~/.config/engram_hive/fonts/
// puis dans les polices système. Variante bold/italic si les fichiers
// existent, sinon famille de base (l'italique reste synthétique epaint).
// ===========================================================================

fn install_fonts(ctx: &egui::Context, cfg: &config::Config) -> FontBook {
    let family = cfg.simple.font_editor.trim();
    if family.is_empty() {
        return FontBook::default();
    }
    let mut dirs = vec![cfg.config_dir.join("fonts")];
    for d in ["/usr/share/fonts", "/usr/local/share/fonts"] {
        dirs.push(PathBuf::from(d));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    let found = find_font_files(&dirs, family);
    if found.regular.is_none() {
        tracing::info!(
            target: "editor",
            "Police '{family}' introuvable : je reste sur la police par défaut. \
             Pose un .ttf dans ~/.config/engram_hive/fonts/ si tu y tiens."
        );
        return FontBook::default();
    }

    let mut defs = egui::FontDefinitions::default();
    let default_prop: Vec<String> = defs
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let mut book = FontBook::default();
    let add = |defs: &mut egui::FontDefinitions, name: &str, path: &PathBuf| -> bool {
        match std::fs::read(path) {
            Ok(bytes) => {
                defs.font_data.insert(
                    name.to_string(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                let mut list = vec![name.to_string()];
                list.extend(default_prop.iter().cloned());
                defs.families
                    .insert(egui::FontFamily::Name(name.into()), list);
                true
            }
            Err(e) => {
                tracing::warn!(target: "editor", "Police {} illisible : {e}", path.display());
                false
            }
        }
    };

    if let Some(p) = &found.regular {
        if add(&mut defs, "prose", p) {
            book.prose = egui::FontFamily::Name("prose".into());
            book.bold = book.prose.clone();
            book.italic = book.prose.clone();
        }
    }
    if let Some(p) = &found.bold {
        if add(&mut defs, "prose-bold", p) {
            book.bold = egui::FontFamily::Name("prose-bold".into());
        }
    }
    if let Some(p) = &found.italic {
        if add(&mut defs, "prose-italic", p) {
            book.italic = egui::FontFamily::Name("prose-italic".into());
        }
    }
    ctx.set_fonts(defs);
    tracing::info!(target: "editor", "Police de prose : {family} ({:?})", found.regular);
    book
}

#[derive(Default, Debug)]
struct FoundFonts {
    regular: Option<PathBuf>,
    bold: Option<PathBuf>,
    italic: Option<PathBuf>,
}

/// Cherche <family>*.ttf/otf dans les dossiers donnés (parcours borné).
fn find_font_files(dirs: &[PathBuf], family: &str) -> FoundFonts {
    let needle = normalize(family);
    let mut found = FoundFonts::default();
    let mut budget = 20_000usize; // parcours borné : pas d'inventaire infini.
    let mut stack: Vec<PathBuf> = dirs.to_vec();
    while let Some(dir) = stack.pop() {
        if budget == 0 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            budget = budget.saturating_sub(1);
            if budget == 0 {
                break;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext_ok = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"));
            if !ext_ok {
                continue;
            }
            let stem = normalize(&path.file_stem().unwrap_or_default().to_string_lossy());
            if !stem.contains(&needle) {
                continue;
            }
            let bold = stem.contains("bold");
            let italic = stem.contains("italic") || stem.contains("oblique");
            match (bold, italic) {
                (true, false) if found.bold.is_none() => found.bold = Some(path),
                (false, true) if found.italic.is_none() => found.italic = Some(path),
                (false, false) => {
                    // Préférer le fichier le plus court ("georgia" avant
                    // "georgiapro-condensed").
                    let better = found.regular.as_ref().is_none_or(|cur| {
                        stem.len()
                            < normalize(&cur.file_stem().unwrap_or_default().to_string_lossy())
                                .len()
                    });
                    if better {
                        found.regular = Some(path);
                    }
                }
                _ => {}
            }
        }
    }
    found
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}
