use file_tree::palette::PaletteAction;

#[derive(Default)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    pub want_focus: bool,
}

impl PaletteState {
    pub fn open(&mut self) {
        if !self.open {
            self.open = true;
            self.query.clear();
            self.selected = 0;
            self.want_focus = true;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
        self.want_focus = false;
    }

    fn filtered(&self) -> Vec<PaletteAction> {
        PaletteAction::ALL
            .iter()
            .copied()
            .filter(|a| fuzzy_match(a.label(), &self.query))
            .collect()
    }
}

pub fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let hay = haystack.to_lowercase();
    let mut hay_chars = hay.chars();
    for nc in needle.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        if !hay_chars.any(|hc| hc == nc) {
            return false;
        }
    }
    true
}

pub fn show(ctx: &egui::Context, state: &mut PaletteState) -> Option<PaletteAction> {
    if !state.open {
        return None;
    }
    let mut chosen = None;
    let viewport_id = egui::ViewportId::from_hash_of("command_palette");
    let builder = egui::ViewportBuilder::default()
        .with_title("Engram Hive — Palette")
        .with_inner_size([520.0, 420.0]);
    ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
        if ctx.input(|i| i.viewport().close_requested()) {
            state.close();
            return;
        }

        let (esc, up, down, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            )
        });
        if esc {
            state.close();
            return;
        }
        let filtered = state.filtered();
        if down && !filtered.is_empty() {
            state.selected = (state.selected + 1).min(filtered.len() - 1);
        }
        if up {
            state.selected = state.selected.saturating_sub(1);
        }
        if state.selected >= filtered.len() {
            state.selected = filtered.len().saturating_sub(1);
        }
        if enter {
            if let Some(a) = filtered.get(state.selected) {
                chosen = Some(*a);
                state.close();
            }
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(8, 8, 12))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        egui::Color32::from_rgb(123, 0, 255),
                    ))
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(480.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Palette");
                        ui.separator();
                        ui.weak("Ctrl+Shift+P ouvre cette fenêtre séparée.");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Fermer").clicked() {
                                state.close();
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut state.query)
                            .hint_text("Tape une commande…")
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut state.want_focus) {
                        edit.request_focus();
                    }
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (i, action) in filtered.iter().enumerate() {
                                let selected = i == state.selected;
                                let text = if selected {
                                    egui::RichText::new(action.label())
                                        .color(egui::Color32::from_rgb(255, 255, 255))
                                        .strong()
                                } else {
                                    egui::RichText::new(action.label())
                                };
                                let response = ui.selectable_label(selected, text);
                                if response.clicked() {
                                    chosen = Some(*action);
                                    state.close();
                                }
                            }
                            if filtered.is_empty() {
                                ui.add_space(4.0);
                                ui.weak("Rien ne correspond. Essaie autre chose.");
                            }
                        });
                });
            });
    });

    chosen
}
