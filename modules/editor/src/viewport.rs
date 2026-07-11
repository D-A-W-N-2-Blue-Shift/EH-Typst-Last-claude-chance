// ============================================================================
// modules/editor/src/viewport.rs — Virtual scrolling et rendu du texte
//
// Règle absolue : par frame, O(visible). Jamais O(N fichiers entiers).
//
// - LayoutCache : hauteur de chaque ligne (mesurée quand la ligne a été
//   visible, estimée sinon) + sommes préfixes → total_height O(1),
//   line_at_y O(log N), y_of_line O(1). Une frappe n'invalide que l'aval.
// - Rendu : seules les lignes qui intersectent le rect visible sont
//   colorées (highlight), mises en page (galley) et peintes. Les hauteurs
//   mesurées corrigent le cache au fil du scroll.
// - Colonne centrée : column_width (en caractères) → pixels, marges en
//   couleur de fond. 0 = pleine largeur.
// - Curseur clignotant, sélection (rects par row de galley), ligne
//   courante, fond des blocs de code, barre de citation, surlignage
//   recherche : tout est peint ici.
// - La souris (clic, drag, double/triple clic, clic sur wikilink) est
//   traduite en intentions retournées à lib.rs — le viewport ne décide pas.
// ============================================================================

use egui::text::{CCursor, LayoutJob, TextFormat};
use ropey::Rope;

use crate::buffer::Buffer;
use crate::config::Config;
use crate::highlight::{self, HighlightCache, Syntaxes};
use crate::wikilinks::WikilinkIndex;

/// Une ligne rendue à cette frame : (index de ligne, position écran de son
/// coin haut-gauche, galley mis en page, indice caractère global du début
/// de ligne). Partagé par le hit-testing souris et le déplacement vertical.
type LineGalley = (usize, egui::Pos2, std::sync::Arc<egui::Galley>, usize);

/// Les familles de polices installées par lib.rs au démarrage.
#[derive(Clone)]
pub struct FontBook {
    pub prose: egui::FontFamily,
    pub bold: egui::FontFamily,
    pub italic: egui::FontFamily,
}

impl Default for FontBook {
    fn default() -> Self {
        Self {
            prose: egui::FontFamily::Proportional,
            bold: egui::FontFamily::Proportional,
            italic: egui::FontFamily::Proportional,
        }
    }
}

// ===========================================================================
// LayoutCache.
// ===========================================================================

pub struct LayoutCache {
    version: u64,
    font_size: f32,
    wrap_width: f32,
    /// Hauteur de chaque ligne. Mesurée si la ligne a déjà été visible,
    /// estimée sinon (l'estimation se corrige toute seule au scroll).
    heights: Vec<f32>,
    measured: Vec<bool>,
    /// prefix[i] = y du HAUT de la ligne i. len = heights.len() + 1.
    prefix: Vec<f32>,
    prefix_dirty: bool,
    row_h: f32,
    para_spacing: f32,
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self {
            version: u64::MAX,
            font_size: 0.0,
            wrap_width: 0.0,
            heights: Vec::new(),
            measured: Vec::new(),
            prefix: vec![0.0],
            prefix_dirty: false,
            row_h: 20.0,
            para_spacing: 0.0,
        }
    }
}

impl LayoutCache {
    /// Synchronise avec le buffer + paramètres d'affichage. Invalide le
    /// minimum : l'aval de la première ligne éditée, ou tout si la police
    /// ou la largeur ont changé.
    pub fn sync(&mut self, buf: &Buffer, font_size: f32, wrap_width: f32, row_h: f32, para: f32) {
        let display_changed = (self.font_size - font_size).abs() > f32::EPSILON
            || (self.wrap_width - wrap_width).abs() > 0.5
            || (self.para_spacing - para).abs() > f32::EPSILON;
        self.row_h = row_h;
        if display_changed {
            self.font_size = font_size;
            self.wrap_width = wrap_width;
            self.para_spacing = para;
            self.heights.clear();
            self.measured.clear();
        } else if buf.version != self.version {
            match buf.min_line_since(self.version) {
                Some(line) if self.version != u64::MAX => {
                    self.heights.truncate(line);
                    self.measured.truncate(line);
                }
                _ => {
                    self.heights.clear();
                    self.measured.clear();
                }
            }
        } else if self.heights.len() == buf.rope.len_lines() {
            return;
        }
        self.version = buf.version;
        let n = buf.rope.len_lines();
        // Ré-estimation des lignes manquantes : O(lignes invalidées), pas O(N)
        // sur une frappe ordinaire (le journal d'éditions borne l'aval).
        while self.heights.len() < n {
            let i = self.heights.len();
            let chars = buf.rope.line(i).len_chars();
            self.heights.push(self.estimate(chars));
            self.measured.push(false);
        }
        self.heights.truncate(n);
        self.measured.truncate(n);
        self.prefix_dirty = true;
    }

    fn estimate(&self, chars: usize) -> f32 {
        let per_row = if self.wrap_width > 0.0 {
            (self.wrap_width / (self.font_size * 0.5)).max(8.0)
        } else {
            120.0
        };
        let rows = ((chars as f32 / per_row).ceil()).max(1.0);
        rows * self.row_h + self.para_spacing
    }

    fn ensure_prefix(&mut self) {
        if !self.prefix_dirty {
            return;
        }
        self.prefix.clear();
        self.prefix.reserve(self.heights.len() + 1);
        let mut y = 0.0;
        self.prefix.push(0.0);
        for h in &self.heights {
            y += h;
            self.prefix.push(y);
        }
        self.prefix_dirty = false;
    }

    pub fn total_height(&mut self) -> f32 {
        self.ensure_prefix();
        *self.prefix.last().unwrap_or(&0.0)
    }

    pub fn y_of_line(&mut self, line: usize) -> f32 {
        self.ensure_prefix();
        self.prefix
            .get(line)
            .copied()
            .unwrap_or_else(|| self.total_height())
    }

    /// Ligne au point y (recherche binaire sur les sommes préfixes).
    pub fn line_at_y(&mut self, y: f32) -> usize {
        self.ensure_prefix();
        if self.prefix.len() < 2 {
            return 0;
        }
        match self.prefix.binary_search_by(|p| p.total_cmp(&y)) {
            Ok(i) => i.min(self.heights.len().saturating_sub(1)),
            Err(i) => i
                .saturating_sub(1)
                .min(self.heights.len().saturating_sub(1)),
        }
    }

    /// Une ligne visible vient d'être mesurée précisément.
    fn set_measured(&mut self, line: usize, h: f32) {
        if let Some(old) = self.heights.get_mut(line) {
            if (*old - h).abs() > 0.5 {
                *old = h;
                self.prefix_dirty = true;
            }
            self.measured[line] = true;
        }
    }
}

// ===========================================================================
// Rendu + interactions.
// ===========================================================================

/// Action déclenchée par le menu contextuel (clic droit). lib.rs la relaie
/// à la fenêtre, qui la traduit en édition — exactement les mêmes effets que
/// les hotkeys (qui restent). Le menu est le complément de discoverability.
#[derive(Clone, Copy, Debug)]
pub enum EditAction {
    Bold,
    Italic,
    Code,
    Link,
    Wikilink,
    Cut,
    Copy,
    Paste,
    Toc,
    InsertTable,
}

/// Ce que la souris a voulu dire cette frame. lib.rs décide quoi en faire.
#[derive(Default)]
pub struct TextAreaOutput {
    /// Position écran du curseur texte (ancre du popup d'auto-complétion).
    pub cursor_screen: Option<egui::Pos2>,
    /// Clic sur un [[wikilink]] : (nom, résout vers un fichier connu ?).
    pub clicked_wikilink: Option<(String, bool)>,
    /// Le curseur/la sélection ont bougé à la souris.
    pub cursor_moved: bool,
    /// Offset de scroll courant (persisté en session).
    pub scroll_offset: f32,
    /// Hauteur visible du viewport texte.
    pub viewport_height: f32,
    /// Le rattrapage de coloration n'est pas fini : re-peindre.
    pub needs_repaint: bool,
    /// Le menu contextuel (clic droit) a demandé une action d'édition.
    pub menu_action: Option<EditAction>,
}

/// Tout ce que le rendu demande à la fenêtre, sans dépendre de lib.rs
/// (évite le cycle lib ↔ viewport : lib construit cette vue par frame).
pub struct TextAreaParams<'a> {
    pub window_id: u64,
    pub cursor: &'a mut usize,
    pub anchor: &'a mut Option<usize>,
    pub font_size: f32,
    pub layout: &'a mut LayoutCache,
    pub highlight: &'a mut HighlightCache,
    pub pending_scroll: &'a mut Option<f32>,
    pub last_input: std::time::Instant,
    /// Résultats de recherche (chars globaux, triés) + occurrence courante.
    pub search_ranges: &'a [(usize, usize)],
    pub search_current: Option<usize>,
    pub focus_mode: bool,
    /// Déplacement vertical demandé au clavier (±1 ligne VISUELLE, ±page).
    /// Résolu ici : les lignes wrappées exigent la géométrie des galleys.
    pub pending_vmove: &'a mut Option<i32>,
    /// Colonne x (en pixels galley) que le curseur cherche à garder en
    /// montant/descendant.
    pub desired_x: &'a mut Option<f32>,
    /// Amener le curseur dans la vue (après frappe/déplacement clavier).
    pub scroll_cursor_into_view: &'a mut bool,
    pub typewriter: bool,
    pub typewriter_position: f32,
    /// La fenêtre a le focus OS. Sinon le curseur reste fixe (pas de
    /// clignotement) : 4 fenêtres ouvertes ne font plus « boîte de nuit ».
    pub focused: bool,
    /// Blocs tableaux actifs (rendu grille au lieu du rendu ligne par ligne).
    pub table_grids: &'a mut Vec<crate::TableGridState>,
}

pub fn show_text_area(
    ui: &mut egui::Ui,
    p: &mut TextAreaParams<'_>,
    buf: &Buffer,
    cfg: &Config,
    syn: &Syntaxes,
    fonts: &FontBook,
    index: &WikilinkIndex,
) -> TextAreaOutput {
    let mut out = TextAreaOutput::default();
    let rope = &buf.rope;
    let theme = &cfg.theme;
    let row_h = p.font_size * cfg.vomi.line_height;
    let avail_w = ui.available_width();
    let padding = cfg.vomi.editor_padding.max(4.0);

    // Largeur de colonne : column_width caractères → pixels (approximation
    // par la largeur du 'o' dans la police de prose).
    let char_w =
        ui.fonts(|f| f.glyph_width(&egui::FontId::new(p.font_size, fonts.prose.clone()), 'o'));
    let wrap_px = if cfg.simple.column_width > 0 {
        (cfg.simple.column_width as f32 * char_w).min(avail_w - 2.0 * padding)
    } else {
        avail_w - 2.0 * padding
    }
    .max(80.0);

    p.layout
        .sync(buf, p.font_size, wrap_px, row_h, cfg.vomi.paragraph_spacing);
    p.highlight.sync(buf);

    let mut scroll = egui::ScrollArea::vertical()
        .id_salt(("editor_scroll", p.window_id))
        .auto_shrink([false, false]);
    if let Some(off) = p.pending_scroll.take() {
        scroll = scroll.vertical_scroll_offset(off.max(0.0));
    }

    let cursor_line = rope.char_to_line((*p.cursor).min(rope.len_chars()));
    let sel = selection_range(*p.cursor, *p.anchor);

    let scroll_out = scroll.show_viewport(ui, |ui, rect| {
        let total = p.layout.total_height() + row_h * 2.0;
        ui.set_height(total);
        ui.set_width(avail_w);

        let origin = ui.min_rect().min;
        let x0 = origin.x + ((avail_w - wrap_px) / 2.0).max(padding);
        let id = egui::Id::new(("editor_text", p.window_id));
        let content_rect = egui::Rect::from_min_size(origin, egui::vec2(avail_w, total));
        let resp = ui.interact(content_rect, id, egui::Sense::click_and_drag());

        // Menu contextuel (clic droit) : discoverability du système 0-menu.
        // La section « Formater » n'apparaît que sur une sélection active.
        let has_sel = sel.is_some();
        resp.context_menu(|ui| {
            if ui.button("Sommaire\tCtrl+Shift+O").clicked() {
                out.menu_action = Some(EditAction::Toc);
                ui.close_menu();
            }
            if ui.button("Insérer tableau\t;table").clicked() {
                out.menu_action = Some(EditAction::InsertTable);
                ui.close_menu();
            }
            ui.separator();
            if has_sel {
                ui.label(egui::RichText::new("Formater").weak());
                if ui.button("Gras\t\tCtrl+B").clicked() {
                    out.menu_action = Some(EditAction::Bold);
                    ui.close_menu();
                }
                if ui.button("Italique\t\tCtrl+I").clicked() {
                    out.menu_action = Some(EditAction::Italic);
                    ui.close_menu();
                }
                if ui.button("Code inline\tCtrl+`").clicked() {
                    out.menu_action = Some(EditAction::Code);
                    ui.close_menu();
                }
                if ui.button("Lien\t\tCtrl+K").clicked() {
                    out.menu_action = Some(EditAction::Link);
                    ui.close_menu();
                }
                if ui.button("Wikilink\tCtrl+Shift+K").clicked() {
                    out.menu_action = Some(EditAction::Wikilink);
                    ui.close_menu();
                }
                ui.separator();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Couper\t\tCtrl+X"))
                .clicked()
            {
                out.menu_action = Some(EditAction::Cut);
                ui.close_menu();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Copier\t\tCtrl+C"))
                .clicked()
            {
                out.menu_action = Some(EditAction::Copy);
                ui.close_menu();
            }
            if ui.button("Coller\t\tCtrl+V").clicked() {
                out.menu_action = Some(EditAction::Paste);
                ui.close_menu();
            }
        });

        let painter = ui.painter().clone();

        let first = p.layout.line_at_y(rect.top());
        let margin = cfg.vomi.syntax_highlight_buffer_lines;
        // Rattrapage budgété du cache d'états : jamais de freeze.
        let catchup_target = p.layout.line_at_y(rect.bottom()).saturating_add(margin + 2);
        if !p
            .highlight
            .ensure(rope, syn, catchup_target.min(rope.len_lines()))
        {
            out.needs_repaint = true;
        }

        // Fond de la colonne en mode focus : noir pur, déjà le cas (thème).
        let mut y = p.layout.y_of_line(first) + origin.y;
        let mut line_idx = first;
        let mut frame_galleys: Vec<LineGalley> = Vec::new();

        while line_idx < rope.len_lines() && y < origin.y + rect.bottom() + row_h {
            // Table grid bypass: si la ligne courante est le début d'un bloc
            // tableau, on rend une grille egui et on saute au-delà du bloc.
            if let Some(grid_idx) = p
                .table_grids
                .iter()
                .position(|g| g.block.first_line == line_idx)
            {
                let grid = &mut p.table_grids[grid_idx];
                let grid_resp = ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                        egui::pos2(x0, y),
                        egui::vec2(wrap_px, 0.0),
                    )),
                    |ui| {
                        draw_table_grid(ui, &mut grid.cells, &mut grid.dirty, grid_idx);
                    },
                );
                let grid_h = grid_resp.response.rect.height().max(row_h);
                let block_lines = grid.block.last_line - grid.block.first_line + 1;
                p.layout.set_measured(line_idx, grid_h);
                for skip in 1..block_lines {
                    p.layout.set_measured(line_idx + skip, 0.0);
                }
                y += grid_h;
                line_idx += block_lines;
                continue;
            }

            let line_char_start = rope.line_to_char(line_idx);
            let line_text = highlight::line_without_newline(rope, line_idx);
            let spans = p
                .highlight
                .line_spans(rope, syn, line_idx, theme, cfg, |name| {
                    index.resolve(name).is_some()
                });

            // LayoutJob : un section par tronçon stylé.
            let mut job = LayoutJob {
                wrap: egui::text::TextWrapping {
                    max_width: wrap_px,
                    ..Default::default()
                },
                ..Default::default()
            };
            if spans.spans.is_empty() {
                job.append(
                    &line_text,
                    0.0,
                    text_format(
                        &highlight::SpanStyle {
                            color: theme.foreground,
                            background: None,
                            italics: false,
                            bold: false,
                            underline: false,
                            scale: 1.0,
                        },
                        p.font_size,
                        fonts,
                        row_h,
                    ),
                );
            }
            for span in &spans.spans {
                let text =
                    &line_text[span.start.min(line_text.len())..span.end.min(line_text.len())];
                if text.is_empty() {
                    continue;
                }
                job.append(
                    text,
                    0.0,
                    text_format(&span.style, p.font_size, fonts, row_h),
                );
            }
            // Une ligne vide doit quand même occuper une hauteur de row.
            if line_text.is_empty() {
                job.append(
                    " ",
                    0.0,
                    text_format(
                        &highlight::SpanStyle {
                            color: egui::Color32::TRANSPARENT,
                            background: None,
                            italics: false,
                            bold: false,
                            underline: false,
                            scale: 1.0,
                        },
                        p.font_size,
                        fonts,
                        row_h,
                    ),
                );
            }

            let galley = ui.fonts(|f| f.layout_job(job));
            let h = galley.size().y + cfg.vomi.paragraph_spacing;
            p.layout.set_measured(line_idx, h);
            let pos = egui::pos2(x0, y);

            // --- Décors sous le texte ---
            let line_rect = egui::Rect::from_min_size(
                egui::pos2(x0 - 6.0, y),
                egui::vec2(wrap_px + 12.0, h - cfg.vomi.paragraph_spacing * 0.5),
            );
            if spans.in_code_block {
                painter.rect_filled(line_rect, 2.0, theme.code_block_bg);
            } else if cfg.simple.highlight_current_line && line_idx == cursor_line && !p.focus_mode
            {
                painter.rect_filled(line_rect, 2.0, theme.current_line_bg);
            }
            if spans.is_blockquote {
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x0 - 14.0, y), egui::vec2(3.0, h)),
                    1.0,
                    theme.blockquote_border,
                );
            }

            // Sélection.
            if let Some((s, e)) = sel {
                paint_char_range(
                    &painter,
                    &galley,
                    pos,
                    line_char_start,
                    line_text.chars().count(),
                    s,
                    e,
                    theme.selection_bg,
                );
            }
            // Occurrences de recherche.
            for (i, (s, e)) in p.search_ranges.iter().enumerate() {
                let color = if Some(i) == p.search_current {
                    theme.search_current_bg
                } else {
                    theme.search_bg
                };
                paint_char_range(
                    &painter,
                    &galley,
                    pos,
                    line_char_start,
                    line_text.chars().count(),
                    *s,
                    *e,
                    color,
                );
            }

            // Numéro de ligne (option).
            if cfg.simple.line_numbers {
                painter.text(
                    egui::pos2(x0 - 18.0, y),
                    egui::Align2::RIGHT_TOP,
                    format!("{}", line_idx + 1),
                    egui::FontId::new(p.font_size * 0.7, fonts.prose.clone()),
                    theme.markers,
                );
            }

            painter.galley(pos, galley.clone(), theme.foreground);

            // Curseur.
            if line_idx == cursor_line {
                let col = (*p.cursor).saturating_sub(line_char_start);
                let ccur = galley.from_ccursor(CCursor::new(col));
                let crect = galley.pos_from_cursor(&ccur);
                let cpos = egui::pos2(pos.x + crect.min.x, pos.y + crect.min.y);
                out.cursor_screen = Some(egui::pos2(cpos.x, cpos.y + crect.height()));
                // Clignotement uniquement si la fenêtre a le focus OS. Sinon le
                // curseur reste affiché en fixe (pas de disco multi-fenêtres).
                let blink_on = !p.focused
                    || (p.last_input.elapsed().as_millis() as u64
                        / cfg.vomi.cursor_blink_ms.max(50))
                    .is_multiple_of(2);
                if blink_on {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            cpos,
                            egui::vec2(
                                cfg.vomi.cursor_thickness.max(1.0),
                                crect.height().max(row_h * 0.8),
                            ),
                        ),
                        0.0,
                        theme.cursor,
                    );
                }
            }

            frame_galleys.push((line_idx, pos, galley, line_char_start));
            y += h;
            line_idx += 1;
        }

        // --- Déplacement vertical visuel demandé par input.rs. ---
        if let Some(delta) = p.pending_vmove.take() {
            if let Some(c) = visual_move(&frame_galleys, rope, *p.cursor, p.desired_x, delta) {
                *p.cursor = c;
                *p.scroll_cursor_into_view = true;
                out.needs_repaint = true;
            }
        }

        // --- Curseur dans la vue / typewriter. ---
        if std::mem::take(p.scroll_cursor_into_view) {
            let line = rope.char_to_line((*p.cursor).min(rope.len_chars()));
            // Position du curseur en coordonnées contenu (précise si la
            // ligne a été rendue, estimée sinon).
            let cursor_y = frame_galleys
                .iter()
                .find(|(li, _, _, _)| *li == line)
                .map(|(_, gpos, galley, line_start)| {
                    let col = (*p.cursor).saturating_sub(*line_start);
                    let r = galley.pos_from_cursor(&galley.from_ccursor(CCursor::new(col)));
                    gpos.y - origin.y + r.min.y
                })
                .unwrap_or_else(|| p.layout.y_of_line(line));
            let view_top = rect.top();
            let view_h = rect.height();
            let margin = row_h * 2.0;
            let target = if p.typewriter {
                Some(crate::focus_mode::typewriter_offset(
                    cursor_y,
                    view_h,
                    p.typewriter_position,
                ))
            } else if cursor_y < view_top + margin {
                Some((cursor_y - margin).max(0.0))
            } else if cursor_y + row_h > view_top + view_h - margin {
                Some(cursor_y + row_h + margin - view_h)
            } else {
                None
            };
            if let Some(t) = target {
                if (t - view_top).abs() > 1.0 {
                    *p.pending_scroll = Some(t.max(0.0));
                    out.needs_repaint = true;
                }
            }
        }

        // --- Souris ---
        handle_mouse(
            p,
            rope,
            &resp,
            &frame_galleys,
            origin,
            index,
            cfg,
            &mut out,
            ui,
        );

        // Curseur main au-dessus d'un wikilink.
        if let Some(hover) = resp.hover_pos() {
            if find_wikilink_at(&frame_galleys, rope, hover).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            } else {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
        }
    });

    out.scroll_offset = scroll_out.state.offset.y;
    out.viewport_height = scroll_out.inner_rect.height();
    out
}

fn text_format(
    style: &highlight::SpanStyle,
    base_size: f32,
    fonts: &FontBook,
    row_h: f32,
) -> TextFormat {
    let family = if style.bold {
        fonts.bold.clone()
    } else if style.italics {
        fonts.italic.clone()
    } else {
        fonts.prose.clone()
    };
    TextFormat {
        font_id: egui::FontId::new(base_size * style.scale, family),
        color: style.color,
        background: style.background.unwrap_or(egui::Color32::TRANSPARENT),
        // Italique synthétique d'epaint si pas de vraie police italique.
        italics: style.italics,
        underline: if style.underline {
            egui::Stroke::new(1.0_f32, style.color)
        } else {
            egui::Stroke::NONE
        },
        line_height: Some(row_h * style.scale),
        ..Default::default()
    }
}

/// Peint un intervalle de caractères GLOBAL [s, e) sur cette ligne, row par
/// row (les lignes wrappées ont plusieurs rows).
#[allow(clippy::too_many_arguments)]
fn paint_char_range(
    painter: &egui::Painter,
    galley: &egui::Galley,
    pos: egui::Pos2,
    line_start: usize,
    line_chars: usize,
    s: usize,
    e: usize,
    color: egui::Color32,
) {
    let line_end = line_start + line_chars + 1; // +1 : le \n sélectionnable
    if e <= line_start || s >= line_end {
        return;
    }
    let from = s.max(line_start) - line_start;
    let to = (e.min(line_end)) - line_start;
    let mut row_start = 0usize;
    for row in &galley.rows {
        let count = row.char_count_including_newline();
        let row_end = row_start + count;
        let a = from.max(row_start);
        let b = to.min(row_end);
        if a < b {
            let x1 = row.x_offset(a - row_start);
            let x2 = if b >= row_end && to > row_end {
                row.rect.right() + 4.0 // la sélection déborde sur le \n
            } else {
                row.x_offset(b - row_start)
            };
            let r = egui::Rect::from_min_max(
                egui::pos2(pos.x + x1, pos.y + row.rect.top()),
                egui::pos2(pos.x + x2.max(x1 + 2.0), pos.y + row.rect.bottom()),
            );
            painter.rect_filled(r, 1.0, color);
        }
        row_start = row_end;
    }
}

fn selection_range(cursor: usize, anchor: Option<usize>) -> Option<(usize, usize)> {
    let a = anchor?;
    if a == cursor {
        return None;
    }
    Some((a.min(cursor), a.max(cursor)))
}

/// Position écran → indice caractère global, via les galleys de la frame.
fn char_at_pos(galleys: &[LineGalley], rope: &Rope, pos: egui::Pos2) -> Option<usize> {
    // La ligne dont le rect vertical contient pos.y, sinon la plus proche.
    let mut best: Option<(f32, &LineGalley)> = None;
    for entry in galleys {
        let (_, gpos, galley, _) = entry;
        let top = gpos.y;
        let bottom = gpos.y + galley.size().y;
        let d = if pos.y < top {
            top - pos.y
        } else if pos.y > bottom {
            pos.y - bottom
        } else {
            0.0
        };
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, entry));
        }
        if d == 0.0 {
            break;
        }
    }
    let (_, (line_idx, gpos, galley, line_start)) = best?;
    let local = pos - *gpos;
    let cur = galley.cursor_from_pos(local);
    let col = cur.ccursor.index;
    let line_chars = rope.line(*line_idx).len_chars();
    // Ne pas dépasser le \n de la ligne.
    let max_col = if *line_idx + 1 == rope.len_lines() {
        line_chars
    } else {
        line_chars.saturating_sub(1)
    };
    Some((line_start + col.min(max_col)).min(rope.len_chars()))
}

/// Y a-t-il un wikilink sous la souris ? Retourne (nom, char_start global).
fn find_wikilink_at(galleys: &[LineGalley], rope: &Rope, pos: egui::Pos2) -> Option<String> {
    for (line_idx, gpos, galley, line_start) in galleys {
        let top = gpos.y;
        let bottom = gpos.y + galley.size().y;
        if pos.y < top || pos.y > bottom {
            continue;
        }
        let local = pos - *gpos;
        let col = galley.cursor_from_pos(local).ccursor.index;
        let line = highlight::line_without_newline(rope, *line_idx);
        // col est en caractères ; link_at travaille en octets.
        let byte = line
            .char_indices()
            .nth(col)
            .map(|(b, _)| b)
            .unwrap_or(line.len());
        let _ = line_start;
        if let Some((_, _, name)) = crate::wikilinks::link_at(&line, byte) {
            return Some(name);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn handle_mouse(
    p: &mut TextAreaParams<'_>,
    rope: &Rope,
    resp: &egui::Response,
    galleys: &[LineGalley],
    _origin: egui::Pos2,
    index: &WikilinkIndex,
    _cfg: &Config,
    out: &mut TextAreaOutput,
    ui: &egui::Ui,
) {
    let pointer = resp.interact_pointer_pos();

    if resp.triple_clicked() {
        if let Some(pos) = pointer {
            if let Some(c) = char_at_pos(galleys, rope, pos) {
                let line = rope.char_to_line(c);
                let start = rope.line_to_char(line);
                let end = start + rope.line(line).len_chars();
                *p.anchor = Some(start);
                *p.cursor = end;
                out.cursor_moved = true;
            }
        }
        return;
    }
    if resp.double_clicked() {
        if let Some(pos) = pointer {
            if let Some(c) = char_at_pos(galleys, rope, pos) {
                let (s, e) = word_bounds(rope, c);
                *p.anchor = Some(s);
                *p.cursor = e;
                out.cursor_moved = true;
            }
        }
        return;
    }
    if resp.clicked() {
        if let Some(pos) = pointer {
            // Clic sur un wikilink → navigation (pas de déplacement curseur).
            if let Some(name) = find_wikilink_at(galleys, rope, pos) {
                let valid = index.resolve(&name).is_some();
                out.clicked_wikilink = Some((name, valid));
                return;
            }
            if let Some(c) = char_at_pos(galleys, rope, pos) {
                let shift = ui.input(|i| i.modifiers.shift);
                if shift {
                    if p.anchor.is_none() {
                        *p.anchor = Some(*p.cursor);
                    }
                } else {
                    *p.anchor = None;
                }
                *p.cursor = c;
                out.cursor_moved = true;
            }
        }
    }
    if resp.drag_started() {
        if let Some(pos) = pointer {
            if let Some(c) = char_at_pos(galleys, rope, pos) {
                *p.anchor = Some(c);
                *p.cursor = c;
                out.cursor_moved = true;
            }
        }
    } else if resp.dragged() {
        if let Some(pos) = pointer {
            if let Some(c) = char_at_pos(galleys, rope, pos) {
                if p.anchor.is_none() {
                    *p.anchor = Some(*p.cursor);
                }
                *p.cursor = c;
                out.cursor_moved = true;
            }
        }
    }
}

/// Déplacement vertical VISUEL : monte/descend de `delta` rows à travers
/// les lignes wrappées, en gardant la colonne x désirée.
fn visual_move(
    galleys: &[LineGalley],
    rope: &Rope,
    cursor: usize,
    desired_x: &mut Option<f32>,
    delta: i32,
) -> Option<usize> {
    let cursor_line = rope.char_to_line(cursor.min(rope.len_chars()));
    // Rows aplatis : (line_idx, line_start, galley, row_idx).
    let mut flat: Vec<(usize, usize, &std::sync::Arc<egui::Galley>, usize)> = Vec::new();
    let mut current_flat: Option<usize> = None;
    let mut cur_x = 0.0;
    for (line_idx, _, galley, line_start) in galleys {
        let cursor_here = *line_idx == cursor_line;
        let (cur_row, x) = if cursor_here {
            let col = cursor.saturating_sub(*line_start);
            let r = galley.pos_from_cursor(&galley.from_ccursor(CCursor::new(col)));
            let yc = (r.min.y + r.max.y) / 2.0;
            let mut idx = 0;
            for (ri, row) in galley.rows.iter().enumerate() {
                if yc >= row.rect.top() - 0.5 && yc <= row.rect.bottom() + 0.5 {
                    idx = ri;
                    break;
                }
            }
            (Some(idx), r.min.x)
        } else {
            (None, 0.0)
        };
        for ri in 0..galley.rows.len() {
            if cur_row == Some(ri) {
                current_flat = Some(flat.len());
                cur_x = x;
            }
            flat.push((*line_idx, *line_start, galley, ri));
        }
    }
    let from = current_flat?; // ligne du curseur hors écran : on laisse tel quel.
    let x = *desired_x.get_or_insert(cur_x);
    let target = (from as i64 + delta as i64).clamp(0, flat.len() as i64 - 1) as usize;
    if target == from && delta != 0 {
        return None; // déjà au bord du rendu : pas mieux à offrir.
    }
    let (line_idx, line_start, galley, row_idx) = flat[target];
    let row = &galley.rows[row_idx];
    let yc = (row.rect.top() + row.rect.bottom()) / 2.0;
    let col = galley.cursor_from_pos(egui::vec2(x, yc)).ccursor.index;
    let line_chars = rope.line(line_idx).len_chars();
    let max_col = if line_idx + 1 == rope.len_lines() {
        line_chars
    } else {
        line_chars.saturating_sub(1)
    };
    Some((line_start + col.min(max_col)).min(rope.len_chars()))
}

/// Frontières du mot autour de `c` (alphanumérique + apostrophes du français).
pub fn word_bounds(rope: &Rope, c: usize) -> (usize, usize) {
    let len = rope.len_chars();
    if len == 0 {
        return (0, 0);
    }
    let c = c.min(len.saturating_sub(1));
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '\u{2019}' || ch == '\'';
    let at = rope.char(c);
    if !is_word(at) {
        return (c, (c + 1).min(len));
    }
    let mut s = c;
    while s > 0 && is_word(rope.char(s - 1)) {
        s -= 1;
    }
    let mut e = c + 1;
    while e < len && is_word(rope.char(e)) {
        e += 1;
    }
    (s, e)
}

fn draw_table_grid(
    ui: &mut egui::Ui,
    cells: &mut Vec<Vec<String>>,
    dirty: &mut bool,
    grid_idx: usize,
) {
    if cells.is_empty() {
        return;
    }
    let cols = cells.first().map(|r| r.len()).unwrap_or(0);
    if cols == 0 {
        return;
    }

    // §1 — Largeur par colonne = max du contenu (header + toutes les lignes)
    // en caractères, converti en pixels via la métrique de la police body.
    // On laisse le tableau dépasser la largeur disponible et on lui donne un
    // scroll horizontal, sinon les colonnes restent comprimées et inutilisables.
    const COL_MIN_PX: f32 = 96.0;
    const COL_MAX_PX: f32 = 800.0;
    const COL_PADDING_PX: f32 = 20.0;
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let char_w = ui.fonts(|f| f.glyph_width(&font_id, 'M'));
    let mut col_widths = vec![COL_MIN_PX; cols];
    for row in cells.iter() {
        for (i, cell) in row.iter().enumerate() {
            if i >= cols {
                break;
            }
            let chars = cell.chars().count().max(1) as f32;
            let px = (chars * char_w + COL_PADDING_PX).clamp(COL_MIN_PX, COL_MAX_PX);
            if px > col_widths[i] {
                col_widths[i] = px;
            }
        }
    }
    let total_width = col_widths.iter().sum::<f32>() + (cols.saturating_sub(1) as f32) * 4.0;

    ui.push_id(("table_grid", grid_idx), |ui| {
        egui::ScrollArea::horizontal()
            .id_salt(("table_grid_scroll", grid_idx))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(total_width.max(ui.available_width()));
                egui::Grid::new(("tg", grid_idx))
                    .num_columns(cols)
                    .striped(true)
                    .spacing([4.0, 2.0])
                    .show(ui, |ui| {
                        for row in cells.iter_mut() {
                            while row.len() < cols {
                                row.push(String::new());
                            }
                            for (i, cell) in row.iter_mut().enumerate() {
                                let w = col_widths.get(i).copied().unwrap_or(COL_MIN_PX);
                                let resp = ui.add_sized(
                                    [w, ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(cell).frame(false),
                                );
                                if resp.changed() {
                                    *dirty = true;
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_with(buf: &Buffer) -> LayoutCache {
        let mut c = LayoutCache::default();
        c.sync(buf, 15.0, 500.0, 24.0, 10.0);
        c
    }

    fn make_buf(text: &str) -> Result<Buffer, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("t.typ");
        std::fs::write(&path, text)?;
        Ok(crate::buffer::open_standalone(&path)?)
    }

    #[test]
    fn prefix_sums_consistent() -> Result<(), Box<dyn std::error::Error>> {
        let buf = make_buf("a\nbb\nccc\n")?;
        let mut c = cache_with(&buf);
        let total = c.total_height();
        assert!(total > 0.0);
        assert_eq!(c.y_of_line(0), 0.0);
        assert!(c.y_of_line(1) > 0.0);
        assert_eq!(c.line_at_y(0.0), 0);
        assert_eq!(c.line_at_y(total + 100.0), 3); // 4 lignes (la dernière vide)
        Ok(())
    }

    #[test]
    fn edit_invalidates_downstream_only() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = make_buf("ligne 0\nligne 1\nligne 2\n")?;
        let mut c = cache_with(&buf);
        c.set_measured(0, 99.0);
        c.set_measured(1, 88.0);
        let _ = c.total_height();
        buf.begin_txn(crate::buffer::TxnKind::Typing, 10);
        buf.insert(10, "x");
        buf.end_txn(11);
        c.sync(&buf, 15.0, 500.0, 24.0, 10.0);
        // La ligne 0 (mesurée, en amont) garde sa hauteur.
        assert_eq!(c.heights[0], 99.0);
        assert!(c.measured[0]);
        // La ligne 1 (touchée) est ré-estimée.
        assert!(!c.measured[1]);
        Ok(())
    }

    #[test]
    fn word_bounds_french() {
        let rope = Rope::from_str("l\u{2019}hiver arrive");
        let (s, e) = word_bounds(&rope, 3);
        assert_eq!((s, e), (0, 7)); // l’hiver : l + apostrophe + hiver
    }
}
