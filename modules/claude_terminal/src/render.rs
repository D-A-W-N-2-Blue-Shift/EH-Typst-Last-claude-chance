// ============================================================================
// modules/claude_terminal/src/render.rs — UI chat du provider CLI
//
// ScrollArea avec historique Q&A, TextEdit pour la saisie, spinner pendant
// l'attente. Bannière status en haut (GLaDOS). Pattern copié de wrapdrive_panel.
// ============================================================================

use egui::{Align, Layout, RichText, ScrollArea, TextEdit};

use crate::config::{AnalysisMode, AuthMode, ScopeMode};
use crate::corpus::CorpusHit;

#[derive(Debug, Clone)]
pub struct Exchange {
    pub timestamp: String,
    pub question: String,
    pub answer: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub model: Option<String>,
}

pub struct RenderState<'a> {
    pub provider: &'static str,
    pub command: &'a str,
    pub auth_mode: AuthMode,
    pub scope_mode: ScopeMode,
    pub analysis_mode: AnalysisMode,
    pub current_file: Option<&'a std::path::Path>,
    pub cumulative_in: u64,
    pub cumulative_out: u64,
    pub corpus_hits: &'a [CorpusHit],
    pub history: &'a [Exchange],
    pub pending: bool,
    pub question_draft: &'a mut String,
    pub status_msgs: &'a mut Vec<(StatusKind, String)>,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

#[derive(Clone, Copy)]
pub enum StatusKind {
    Ok,
    Warn,
}

pub struct RenderOutput {
    pub send_question: Option<String>,
    pub export_cut: bool,
    pub scope_mode: ScopeMode,
    pub analysis_mode: AnalysisMode,
}

pub fn draw(ui: &mut egui::Ui, state: &mut RenderState<'_>) -> RenderOutput {
    let mut output = RenderOutput {
        send_question: None,
        export_cut: false,
        scope_mode: state.scope_mode,
        analysis_mode: state.analysis_mode,
    };

    draw_status_banner(ui, state.status_msgs);

    ui.horizontal_wrapped(|ui| {
        ui.weak(format!("CLI : {} ({})", state.provider, state.command));
        ui.separator();
        let auth_label = match state.auth_mode {
            AuthMode::Subscription => "Abonnement",
            AuthMode::ApiKey => "Clé API",
        };
        ui.weak(format!("Auth : {auth_label}"));
        ui.separator();
        ui.weak(format!("Périmètre : {}", state.scope_mode.label()));
        ui.separator();
        ui.weak(format!("Mode : {}", state.analysis_mode.label()));
        ui.separator();
        if let Some(path) = state.current_file {
            ui.weak(format!("Fichier : {}", path.display()));
            ui.separator();
        }
        ui.weak(format!(
            "Tokens session : {} in / {} out",
            state.cumulative_in, state.cumulative_out
        ));
        if let Some(cost) = estimated_cost(
            state.cumulative_in,
            state.cumulative_out,
            state.input_cost_per_1k,
            state.output_cost_per_1k,
        ) {
            ui.separator();
            ui.weak(format!("Coût estimé : {cost:.4}"));
        }
    });
    ui.separator();

    if !state.corpus_hits.is_empty() {
        ui.collapsing("Contexte joint (corpus)", |ui| {
            for hit in state.corpus_hits.iter().take(8) {
                ui.group(|ui| {
                    ui.strong(format!(
                        "{} | {} | {} mots",
                        hit.path.display(),
                        hit.section,
                        hit.words_body
                    ));
                    ui.label(&hit.snippet);
                });
            }
        });
        ui.separator();
    }

    let available = ui.available_height() - 80.0;
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(available.max(100.0))
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if state.history.is_empty() && !state.pending {
                ui.weak(
                    "Posez une question sur le projet. Le provider CLI répondra en lecture seule.",
                );
            }
            for ex in state.history {
                draw_exchange(ui, ex);
            }
            if state.pending {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak("L'assistant réfléchit…");
                });
            }
        });

    ui.separator();

    ui.horizontal(|ui| {
        let te = TextEdit::multiline(state.question_draft)
            .hint_text("Votre question…")
            .desired_rows(2)
            .desired_width(ui.available_width() - 160.0);
        let resp = ui.add(te);

        let can_send = !state.question_draft.trim().is_empty() && !state.pending;
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            egui::ComboBox::from_id_salt("coh2b_scope")
                .selected_text(state.scope_mode.label())
                .show_ui(ui, |ui| {
                    for scope in ScopeMode::ALL {
                        ui.selectable_value(&mut state.scope_mode, *scope, scope.label());
                    }
                });
            egui::ComboBox::from_id_salt("coh2b_mode")
                .selected_text(state.analysis_mode.label())
                .show_ui(ui, |ui| {
                    for mode in AnalysisMode::ALL {
                        ui.selectable_value(&mut state.analysis_mode, *mode, mode.label());
                    }
                });
            if ui
                .add_enabled(can_send, egui::Button::new("Envoyer"))
                .clicked()
                || (resp.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl)
                    && can_send)
            {
                output.send_question = Some(state.question_draft.trim().to_string());
                state.question_draft.clear();
            }
            if ui.button("Export cut").clicked() {
                output.export_cut = true;
            }
        });
    });

    output.scope_mode = state.scope_mode;
    output.analysis_mode = state.analysis_mode;
    output
}

fn estimated_cost(
    input_tokens: u64,
    output_tokens: u64,
    input_cost_per_1k: Option<f64>,
    output_cost_per_1k: Option<f64>,
) -> Option<f64> {
    let in_rate = input_cost_per_1k?;
    let out_rate = output_cost_per_1k?;
    Some((input_tokens as f64 / 1000.0) * in_rate + (output_tokens as f64 / 1000.0) * out_rate)
}

fn draw_exchange(ui: &mut egui::Ui, ex: &Exchange) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(100, 180, 255),
                RichText::new("Q").strong(),
            );
            ui.weak(&ex.timestamp);
        });
        ui.label(&ex.question);
        ui.add_space(4.0);
        match &ex.answer {
            Some(answer) => {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 255, 180),
                    RichText::new("R").strong(),
                );
                ui.label(answer);
                if let (Some(i), Some(o)) = (ex.input_tokens, ex.output_tokens) {
                    ui.weak(format!("({i} in / {o} out)"));
                }
            }
            None => {
                ui.weak("En attente…");
            }
        }
    });
    ui.add_space(4.0);
}

fn draw_status_banner(ui: &mut egui::Ui, msgs: &mut Vec<(StatusKind, String)>) {
    if msgs.is_empty() {
        return;
    }
    let mut to_dismiss = None;
    for (i, (kind, msg)) in msgs.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            let (sym, color) = match kind {
                StatusKind::Ok => ("\u{2713}", egui::Color32::from_rgb(120, 255, 180)),
                StatusKind::Warn => ("\u{26a0}", egui::Color32::from_rgb(255, 46, 136)),
            };
            ui.colored_label(color, sym);
            ui.label(msg.as_str());
            if ui.small_button("\u{2715}").clicked() {
                to_dismiss = Some(i);
            }
        });
    }
    if let Some(i) = to_dismiss {
        msgs.remove(i);
    }
    ui.separator();
}
