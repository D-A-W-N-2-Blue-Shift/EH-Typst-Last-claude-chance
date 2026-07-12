use std::path::Path;

use crate::config::SortMode;
use crate::db::{NoteDraft, NoteRecord, NoteTag};

pub enum ListAction {
    None,
    Open(String),
    NewFromCurrent,
    Refresh,
}

pub enum NoteAction {
    None,
    Save,
    Delete,
    Close,
}

pub fn draw_list(
    ui: &mut egui::Ui,
    notes: &[NoteRecord],
    query: &mut String,
    tag_type_filter: &mut String,
    file_filter: &mut String,
    sort_mode: &mut SortMode,
    current_source: Option<&Path>,
    orphan_count: usize,
    last_sync: Option<&str>,
    last_error: Option<&str>,
) -> ListAction {
    let mut action = ListAction::None;
    ui.horizontal(|ui| {
        ui.label("Recherche");
        ui.text_edit_singleline(query);
        ui.label("Tag");
        ui.text_edit_singleline(tag_type_filter);
        ui.label("Fichier");
        ui.text_edit_singleline(file_filter);
    });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("sticky_notes_sort")
            .selected_text(sort_mode.label())
            .show_ui(ui, |ui| {
                for mode in SortMode::ALL {
                    ui.selectable_value(sort_mode, *mode, mode.label());
                }
            });
        if ui.button("Nouvelle note").clicked() {
            action = ListAction::NewFromCurrent;
        }
        if ui.button("Rafraîchir").clicked() {
            action = ListAction::Refresh;
        }
    });
    if let Some(src) = current_source {
        ui.weak(format!("Source courante : {}", src.display()));
    }
    if let Some(sync) = last_sync {
        ui.weak(sync);
    }
    if orphan_count > 0 {
        ui.colored_label(
            egui::Color32::from_rgb(255, 120, 120),
            format!("{orphan_count} note(s) orpheline(s) détectée(s)"),
        );
    }
    if let Some(err) = last_error {
        ui.colored_label(egui::Color32::from_rgb(255, 46, 136), err);
    }
    ui.separator();

    let mut filtered: Vec<&NoteRecord> = notes
        .iter()
        .filter(|n| matches_filters(n, query, tag_type_filter, file_filter))
        .collect();
    sort_notes(&mut filtered, *sort_mode);

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("sticky_notes_grid")
            .striped(true)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.strong("Date");
                ui.strong("Extrait");
                ui.strong("Tags");
                ui.strong("Fichiers liés");
                ui.end_row();
                for note in filtered {
                    if ui.button(short_date(&note.updated_at)).clicked() {
                        action = ListAction::Open(note.id.clone());
                    }
                    ui.label(excerpt(&note.contenu));
                    ui.label(join_tags(&note.tags));
                    ui.label(join_links(&note.links));
                    ui.end_row();
                }
            });
    });

    action
}

pub fn draw_note(
    ui: &mut egui::Ui,
    draft: &mut NoteDraft,
    original: Option<&NoteRecord>,
    current_source: Option<&Path>,
) -> NoteAction {
    let mut action = NoteAction::None;
    ui.horizontal(|ui| {
        ui.label("ID");
        ui.monospace(draft.id.as_str());
    });
    ui.horizontal(|ui| {
        ui.label("Source");
        let mut source_text = draft
            .source_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        if ui.text_edit_singleline(&mut source_text).changed() {
            let trimmed = source_text.trim();
            draft.source_path = if trimmed.is_empty() {
                None
            } else {
                Some(Path::new(trimmed).to_path_buf())
            };
        }
        if ui.button("Fichier courant").clicked() {
            draft.source_path = current_source.map(|p| p.to_path_buf());
        }
    });
    ui.horizontal(|ui| {
        ui.label("Ligne");
        let mut line = draft.anchor_line.unwrap_or(1).max(1);
        if ui.add(egui::DragValue::new(&mut line).speed(1)).changed() {
            draft.anchor_line = Some(line.max(1));
        }
        if let Some(orig) = original {
            ui.weak(format!("Créée le {}", orig.created_at));
        }
    });
    ui.separator();
    ui.label("Contenu");
    ui.add(
        egui::TextEdit::multiline(&mut draft.contenu)
            .desired_rows(10)
            .lock_focus(true),
    );
    ui.separator();
    ui.horizontal(|ui| {
        ui.strong("Tags");
        if ui.button("+").clicked() {
            draft.tags.push(NoteTag {
                type_: "autre".into(),
                valeur: String::new(),
            });
        }
    });
    edit_tags(ui, draft);
    ui.separator();
    ui.horizontal(|ui| {
        ui.strong("Liens");
        if ui.button("+").clicked() {
            draft.links.push(crate::db::NoteLink {
                target: String::new(),
            });
        }
    });
    edit_links(ui, draft);
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Enregistrer").clicked() {
            action = NoteAction::Save;
        }
        if ui.button("Supprimer").clicked() {
            action = NoteAction::Delete;
        }
        if ui.button("Fermer").clicked() {
            action = NoteAction::Close;
        }
    });
    action
}

fn edit_tags(ui: &mut egui::Ui, draft: &mut NoteDraft) {
    let mut remove = None;
    for (idx, tag) in draft.tags.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut tag.type_);
            ui.text_edit_singleline(&mut tag.valeur);
            if ui.small_button("✕").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some(idx) = remove {
        draft.tags.remove(idx);
    }
}

fn edit_links(ui: &mut egui::Ui, draft: &mut NoteDraft) {
    let mut remove = None;
    for (idx, link) in draft.links.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut link.target);
            if ui.small_button("✕").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some(idx) = remove {
        draft.links.remove(idx);
    }
}

fn matches_filters(note: &NoteRecord, query: &str, tag_type: &str, file_filter: &str) -> bool {
    let q = query.trim().to_lowercase();
    if !q.is_empty() {
        let mut hay = note.contenu.to_lowercase();
        hay.push_str(&note.id.to_lowercase());
        hay.push_str(&join_tags(&note.tags).to_lowercase());
        hay.push_str(&join_links(&note.links).to_lowercase());
        if let Some(path) = &note.source_path {
            hay.push_str(&path.display().to_string().to_lowercase());
        }
        if !hay.contains(&q) {
            return false;
        }
    }
    let tag_type = tag_type.trim().to_lowercase();
    if !tag_type.is_empty()
        && !note
            .tags
            .iter()
            .any(|t| t.type_.to_lowercase().contains(&tag_type))
    {
        return false;
    }
    let file_filter = file_filter.trim().to_lowercase();
    if !file_filter.is_empty() {
        let path = note
            .source_path
            .as_ref()
            .map(|p| p.display().to_string().to_lowercase())
            .unwrap_or_default();
        if !path.contains(&file_filter) {
            return false;
        }
    }
    true
}

fn sort_notes(notes: &mut Vec<&NoteRecord>, mode: SortMode) {
    notes.sort_by(|a, b| match mode {
        SortMode::DateDesc => b.updated_at.cmp(&a.updated_at),
        SortMode::DateAsc => a.updated_at.cmp(&b.updated_at),
        SortMode::File => a
            .source_path
            .as_ref()
            .map(|p| p.display().to_string())
            .cmp(&b.source_path.as_ref().map(|p| p.display().to_string())),
        SortMode::TagType => join_tags(&a.tags).cmp(&join_tags(&b.tags)),
    });
}

fn excerpt(content: &str) -> String {
    let mut s = content.trim().replace('\n', " ");
    if s.len() > 120 {
        s.truncate(117);
        s.push_str("...");
    }
    if s.is_empty() {
        "note vide".into()
    } else {
        s
    }
}

fn short_date(raw: &str) -> String {
    raw.chars().take(19).collect()
}

fn join_tags(tags: &[NoteTag]) -> String {
    tags.iter()
        .map(|t| format!("{}:{}", t.type_, t.valeur))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn join_links(links: &[crate::db::NoteLink]) -> String {
    links
        .iter()
        .map(|l| l.target.clone())
        .collect::<Vec<_>>()
        .join(" | ")
}
