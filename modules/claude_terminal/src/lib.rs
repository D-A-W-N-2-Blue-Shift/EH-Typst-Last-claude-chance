// ============================================================================
// modules/claude_terminal/src/lib.rs — Module provider CLI
//
// Fenêtre chat Q&A invoquant un CLI d'assistant en arrière-plan (one-shot via
// --print). Remplace COH2B. Lecture seule du projet. Viewport propre,
// fermé au démarrage (doctrine "fenêtre à la demande").
// ============================================================================

use std::path::PathBuf;
use std::sync::mpsc;

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse, RenderMode};

pub mod config;
mod process;
mod render;
mod token_log;

use config::ClaudeTerminalConfig;
use process::ClaudeResponse;
use render::{Exchange, RenderOutput, StatusKind};

pub struct ClaudeTerminalModule {
    config: ClaudeTerminalConfig,
    data_dir: PathBuf,
    project_root: Option<PathBuf>,
    history: Vec<Exchange>,
    pending: Option<mpsc::Receiver<Result<ClaudeResponse, String>>>,
    question_draft: String,
    log_file: Option<PathBuf>,
    status_msgs: Vec<(StatusKind, String)>,
    cumulative_in: u64,
    cumulative_out: u64,
    closed: bool,
}

impl Default for ClaudeTerminalModule {
    fn default() -> Self {
        Self {
            config: ClaudeTerminalConfig::default(),
            data_dir: PathBuf::new(),
            project_root: None,
            history: Vec::new(),
            pending: None,
            question_draft: String::new(),
            log_file: None,
            status_msgs: Vec::new(),
            cumulative_in: 0,
            cumulative_out: 0,
            closed: true,
        }
    }
}

impl Module for ClaudeTerminalModule {
    fn name(&self) -> &'static str {
        "claude_terminal"
    }

    fn init(&mut self, ctx: &CoreContext) -> Result<(), String> {
        self.data_dir = ctx.data_dir.clone();

        let (cfg, errors) = ClaudeTerminalConfig::load(&ctx.licorne);
        self.config = cfg;
        for e in errors {
            self.status_msgs.push((StatusKind::Warn, e));
        }

        let (ci, co) = token_log::read_cumulative(&self.data_dir);
        self.cumulative_in = ci;
        self.cumulative_out = co;

        let log_dir = self.data_dir.join("logs").join("claude_terminal");
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            self.status_msgs.push((
                StatusKind::Warn,
                format!("Création dossier logs impossible : {e}"),
            ));
        } else {
            let now = chrono::Local::now();
            let name = now.format("%d-%m-%Y-%Hh%M.typ").to_string();
            let path = log_dir.join(name);
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(_) => self.log_file = Some(path),
                Err(e) => self.status_msgs.push((
                    StatusKind::Warn,
                    format!("Ouverture log {} : {e}", path.display()),
                )),
            }
        }

        Ok(())
    }

    fn render_mode(&self) -> RenderMode {
        RenderMode::OwnViewport
    }

    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        self.poll_pending();

        if self.closed {
            return;
        }

        let viewport_id = egui::ViewportId::from_hash_of("claude_terminal");
        let builder = egui::ViewportBuilder::default()
            .with_title("Engram Hive — Assistant CLI")
            .with_inner_size([700.0, 520.0]);

        let mut open_palette = false;
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                self.closed = true;
                return;
            }
            // §7 — Ctrl+Shift+P depuis la fenêtre provider CLI.
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                    egui::Key::P,
                )
            }) {
                open_palette = true;
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut state = render::RenderState {
                    provider: self.config.provider.label(),
                    command: &self.config.command,
                    auth_mode: self.config.auth_mode,
                    cumulative_in: self.cumulative_in,
                    cumulative_out: self.cumulative_out,
                    history: &self.history,
                    pending: self.pending.is_some(),
                    question_draft: &mut self.question_draft,
                    status_msgs: &mut self.status_msgs,
                };
                let out = render::draw(ui, &mut state);
                self.handle_render_output(out);
            });
        });
        if open_palette {
            out.push(ModuleResponse::OpenPaletteRequested);
        }
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
            CoreEvent::FileIndexUpdated { project_root, .. } => {
                self.project_root = Some(project_root.clone());
            }
            CoreEvent::OpenModuleWindowRequested(name) => {
                if name == self.name() {
                    self.closed = false;
                }
            }
            _ => {}
        }
    }

    fn shutdown(&mut self) {}
}

impl ClaudeTerminalModule {
    fn poll_pending(&mut self) {
        let Some(rx) = &self.pending else { return };
        match rx.try_recv() {
            Ok(Ok(resp)) => {
                if let Some(ex) = self.history.last_mut() {
                    ex.answer = Some(resp.text);
                    ex.input_tokens = resp.input_tokens;
                    ex.output_tokens = resp.output_tokens;
                    ex.model = resp.model.clone();

                    let i = resp.input_tokens.unwrap_or(0);
                    let o = resp.output_tokens.unwrap_or(0);
                    self.cumulative_in = self.cumulative_in.saturating_add(i);
                    self.cumulative_out = self.cumulative_out.saturating_add(o);

                    if i > 0 || o > 0 {
                        let entry = token_log::TokenEntry {
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            input_tokens: i,
                            output_tokens: o,
                            model: resp.model,
                        };
                        if let Err(e) = token_log::log_tokens(&self.data_dir, &entry) {
                            self.status_msgs.push((StatusKind::Warn, e));
                        }
                    }

                    append_log(&self.log_file, ex);
                }
                self.pending = None;
            }
            Ok(Err(err)) => {
                if let Some(ex) = self.history.last_mut() {
                    ex.answer = Some(format!("[Erreur] {err}"));
                    append_log(&self.log_file, ex);
                }
                self.status_msgs.push((StatusKind::Warn, err));
                self.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(ex) = self.history.last_mut() {
                    if ex.answer.is_none() {
                        ex.answer = Some("[Erreur] Canal interrompu".into());
                        append_log(&self.log_file, ex);
                    }
                }
                self.pending = None;
            }
        }
    }

    fn handle_render_output(&mut self, out: RenderOutput) {
        if let Some(question) = out.send_question {
            self.send_question(question);
        }
        if out.export_cut {
            self.export_cut();
        }
    }

    fn send_question(&mut self, question: String) {
        let cwd = self
            .project_root
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let now = chrono::Local::now();
        let ts = now.format("%H:%M:%S").to_string();

        self.history.push(Exchange {
            timestamp: ts,
            question: question.clone(),
            answer: None,
            input_tokens: None,
            output_tokens: None,
            model: None,
        });

        let rx = process::spawn_query(question, cwd, self.config.clone());
        self.pending = Some(rx);
    }

    fn export_cut(&mut self) {
        let log_dir = self.data_dir.join("logs").join("claude_terminal");
        let now = chrono::Local::now();
        let name = now.format("%d-%m-%Y-%Hh%M.typ").to_string();
        let path = log_dir.join(name);
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(_) => {
                self.log_file = Some(path);
                self.status_msgs
                    .push((StatusKind::Ok, "Nouveau fichier log ouvert.".into()));
            }
            Err(e) => {
                self.status_msgs
                    .push((StatusKind::Warn, format!("Export cut impossible : {e}")));
            }
        }
    }
}

fn append_log(log_file: &Option<PathBuf>, ex: &Exchange) {
    let Some(path) = log_file else { return };
    let mut content = format!("## {} — Question\n\n{}\n\n", ex.timestamp, ex.question);
    if let Some(ref answer) = ex.answer {
        content.push_str(&format!("### Réponse\n\n{}\n\n", answer));
    }
    if let (Some(i), Some(o)) = (ex.input_tokens, ex.output_tokens) {
        content.push_str(&format!("_Tokens : {} in / {} out_\n\n", i, o));
    }
    content.push_str("---\n\n");
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, content.as_bytes()))
    {
        tracing::warn!("Écriture log claude_terminal : {e}");
    }
}
