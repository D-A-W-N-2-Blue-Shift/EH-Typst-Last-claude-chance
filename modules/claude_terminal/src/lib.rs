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
mod corpus;
mod process;
mod render;
mod token_log;

use config::{ClaudeTerminalConfig, ScopeMode};
use corpus::{read_excerpt, search_corpus, truncate_chars, CorpusHit};
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
    current_file: Option<PathBuf>,
    corpus_hits: Vec<CorpusHit>,
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
            current_file: None,
            corpus_hits: Vec::new(),
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

        let log_dir = self.data_dir.join("logs").join("coh2b");
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            self.status_msgs.push((
                StatusKind::Warn,
                format!("Création dossier logs impossible : {e}"),
            ));
        } else {
            let now = chrono::Local::now();
            let name = now.format("%Y-%m-%d.jsonl").to_string();
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
            .with_title("Engram Hive — COH2B")
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
                    scope_mode: self.config.scope,
                    analysis_mode: self.config.analysis_mode,
                    current_file: self.current_file.as_deref(),
                    cumulative_in: self.cumulative_in,
                    cumulative_out: self.cumulative_out,
                    corpus_hits: &self.corpus_hits,
                    history: &self.history,
                    pending: self.pending.is_some(),
                    question_draft: &mut self.question_draft,
                    status_msgs: &mut self.status_msgs,
                    input_cost_per_1k: self.config.input_cost_per_1k,
                    output_cost_per_1k: self.config.output_cost_per_1k,
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
                self.refresh_corpus_hits();
            }
            CoreEvent::OpenFileRequested(path) => {
                self.current_file = Some(path.clone());
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
        self.config.scope = out.scope_mode;
        self.config.analysis_mode = out.analysis_mode;
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

        self.refresh_corpus_hits_for_query(&question);
        let prompt = self.build_prompt(&question, &cwd);
        let rx = process::spawn_query(prompt, cwd, self.config.clone());
        self.pending = Some(rx);
    }

    fn build_prompt(&mut self, question: &str, cwd: &PathBuf) -> String {
        let scope = self.config.scope;
        let mode = self.config.analysis_mode;
        let mut out = String::new();
        out.push_str("Tu es un agent d'analyse en lecture seule. ");
        out.push_str("Tu ne dois produire aucune modification de fichier.\n\n");
        out.push_str(&format!("Mode d'analyse : {}\n", mode.label()));
        out.push_str(&format!("Périmètre : {}\n", scope.label()));
        out.push_str(&format!("Racine projet : {}\n\n", cwd.display()));

        match scope {
            ScopeMode::CurrentFile => {
                if let Some(path) = &self.current_file {
                    out.push_str(&format!("Fichier courant : {}\n\n", path.display()));
                    match read_excerpt(path, 18_000) {
                        Ok(text) => {
                            out.push_str("=== CONTENU DU FICHIER ===\n");
                            out.push_str(&truncate_chars(&text, 18_000));
                            out.push_str("\n\n");
                        }
                        Err(e) => {
                            out.push_str(&format!("(Contexte fichier indisponible : {e})\n\n"));
                        }
                    }
                } else {
                    out.push_str("(Aucun fichier courant connu)\n\n");
                }
            }
            ScopeMode::SelectedText => {
                out.push_str(
                    "(Le texte sélectionné n'est pas encore câblé ; fallback sur le fichier courant si disponible.)\n\n",
                );
                if let Some(path) = &self.current_file {
                    out.push_str(&format!("Fichier courant : {}\n\n", path.display()));
                }
            }
            ScopeMode::Corpus | ScopeMode::Project => {
                if self.corpus_hits.is_empty() {
                    self.refresh_corpus_hits();
                }
                if self.corpus_hits.is_empty() {
                    out.push_str("(Aucun extrait corpus indexé disponible.)\n\n");
                } else {
                    out.push_str("=== EXTRAITS CORPUS ===\n");
                    for hit in self.corpus_hits.iter().take(8) {
                        out.push_str(&format!(
                            "- {} [{} | {} | {} mots]\n  {}\n",
                            hit.path.display(),
                            hit.file_stem,
                            hit.section,
                            hit.words_body,
                            hit.snippet
                        ));
                    }
                    out.push('\n');
                }
            }
        }

        out.push_str("=== QUESTION ===\n");
        out.push_str(question);
        out.push_str("\n\nRéponds avec des preuves, contre-preuves et incertitudes. ");
        out.push_str("Si la réponse est factuelle, cite les fichiers et lignes si possible.");
        out
    }

    fn refresh_corpus_hits(&mut self) {
        let query = self.question_draft.trim().to_string();
        self.refresh_corpus_hits_for_query(&query);
    }

    fn refresh_corpus_hits_for_query(&mut self, query: &str) {
        let Some(root) = self.project_root.as_ref() else {
            self.corpus_hits.clear();
            return;
        };
        if query.is_empty() {
            self.corpus_hits.clear();
            return;
        }
        match search_corpus(root, query, 8) {
            Ok(hits) => {
                self.corpus_hits = hits;
                if !self.corpus_hits.is_empty() {
                    self.status_msgs.push((
                        StatusKind::Ok,
                        format!("Corpus mis à jour: {} résultat(s).", self.corpus_hits.len()),
                    ));
                }
            }
            Err(e) => {
                self.status_msgs.push((StatusKind::Warn, e));
                self.corpus_hits.clear();
            }
        }
    }

    fn export_cut(&mut self) {
        let log_dir = self.data_dir.join("logs").join("coh2b");
        let now = chrono::Local::now();
        let name = now.format("%Y-%m-%d.jsonl").to_string();
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
    let entry = serde_json::json!({
        "timestamp": ex.timestamp,
        "question": ex.question,
        "answer": ex.answer,
        "input_tokens": ex.input_tokens,
        "output_tokens": ex.output_tokens,
        "model": ex.model,
    });
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{entry}\n").as_bytes()))
    {
        tracing::warn!("Écriture log coh2b : {e}");
    }
}
