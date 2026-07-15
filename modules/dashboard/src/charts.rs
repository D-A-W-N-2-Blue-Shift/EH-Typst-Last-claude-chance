// ============================================================================
// modules/dashboard/src/charts.rs — Rendu (peinture egui maison, doc §5.6)
//
// APIs déjà vérifiées contre la source vendue egui/epaint/ecolor 0.31.1
// lors de l'incrément santé (radar) : Painter::line_segment/text/add,
// Shape::closed_line, Color32::{from_rgb,from_gray,gamma_multiply},
// FontId::proportional, Ui::{allocate_exact_size,painter_at}. Aucune
// nouvelle API introduite ici — même patron, pas de nouvelle vérification
// nécessaire.
// ============================================================================

use crate::correlations::{MedObservanceWeek, WeeklyLoad};
use crate::logic::Dimensions;

/// Barres (durée) + ligne (moyenne glissante) + points colorés (qualité
/// 1-5) sur un axe X commun (une nuit = une position). Doc §5.6 : « Durée
/// nuit par nuit... Ligne de référence... Qualité subjective en overlay ».
pub fn draw_sleep_chart(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    nights: &[nexus_db::SleepLog],
    rolling_avg: &[Option<f64>],
) {
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if nights.is_empty() {
        return;
    }
    let painter = ui.painter_at(rect);
    let max_h = nights
        .iter()
        .map(|n| n.duration_h)
        .fold(1.0_f64, f64::max)
        .max(rolling_avg.iter().flatten().copied().fold(0.0, f64::max));
    let n = nights.len() as f32;
    let bar_w = (rect.width() / n).max(1.0);
    let bar_color = egui::Color32::from_rgb(123, 0, 255);
    let avg_color = egui::Color32::from_rgb(255, 46, 136);

    let y_for = |h: f64| -> f32 { rect.bottom() - (h / max_h) as f32 * rect.height() };

    // Barres.
    for (i, night) in nights.iter().enumerate() {
        let x0 = rect.left() + i as f32 * bar_w;
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(x0 + 1.0, y_for(night.duration_h)),
            egui::pos2(x0 + bar_w - 1.0, rect.bottom()),
        );
        painter.rect_filled(bar_rect, 0.0, bar_color.gamma_multiply(0.6));
        // Point qualité (1-5) : couleur du vert (bon) au rouge (mauvais).
        let q = night.quality.clamp(1, 5);
        let quality_color =
            egui::Color32::from_rgb((255 - (q - 1) * 40) as u8, (120 + (q - 1) * 25) as u8, 60);
        painter.circle_filled(
            egui::pos2(x0 + bar_w / 2.0, y_for(night.duration_h) - 5.0),
            3.0,
            quality_color,
        );
    }

    // Ligne moyenne glissante (segments entre points consécutifs connus —
    // un trou dans les données ne relie jamais deux points à travers lui).
    let points: Vec<Option<egui::Pos2>> = rolling_avg
        .iter()
        .enumerate()
        .map(|(i, avg)| avg.map(|a| egui::pos2(rect.left() + (i as f32 + 0.5) * bar_w, y_for(a))))
        .collect();
    for pair in points.windows(2) {
        if let [Some(a), Some(b)] = pair {
            painter.line_segment([*a, *b], egui::Stroke::new(2.0_f32, avg_color));
        }
    }
}

/// Jusqu'à 5 lignes (une par dimension), doc §5.6 : « tendance 30 jours par
/// dimension (ligne par ligne, pas empilées) ». `series` : une entrée par
/// jour affiché, valeurs 0-5.
pub fn draw_dimension_trend(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    series: &[(chrono::NaiveDate, Dimensions)],
) {
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if series.len() < 2 {
        return;
    }
    let painter = ui.painter_at(rect);
    let colors = [
        egui::Color32::from_rgb(255, 46, 136),
        egui::Color32::from_rgb(123, 0, 255),
        egui::Color32::from_rgb(0, 200, 255),
        egui::Color32::from_rgb(255, 200, 0),
        egui::Color32::from_rgb(120, 255, 180),
    ];
    let accessors: [fn(&Dimensions) -> f64; 5] = [
        |d| d.epuisement,
        |d| d.cognitif,
        |d| d.sensoriel,
        |d| d.masquage,
        |d| d.fonctionnement,
    ];
    let n = series.len() as f32;
    let step = rect.width() / (n - 1.0).max(1.0);
    let y_for = |v: f64| -> f32 { rect.bottom() - (v / 5.0) as f32 * rect.height() };

    for (dim_idx, accessor) in accessors.iter().enumerate() {
        let pts: Vec<egui::Pos2> = series
            .iter()
            .enumerate()
            .map(|(i, (_, dims))| egui::pos2(rect.left() + i as f32 * step, y_for(accessor(dims))))
            .collect();
        for pair in pts.windows(2) {
            painter.line_segment(
                [pair[0], pair[1]],
                egui::Stroke::new(1.5_f32, colors[dim_idx]),
            );
        }
    }

    // Légende.
    let mut legend_y = rect.top() + 4.0;
    for (label, color) in crate::logic::DIMENSION_LABELS.iter().zip(colors.iter()) {
        painter.text(
            egui::pos2(rect.right() - 90.0, legend_y),
            egui::Align2::LEFT_TOP,
            *label,
            egui::FontId::proportional(10.0),
            *color,
        );
        legend_y += 12.0;
    }
}

/// Radar 5 axes : semaine courante (trait plein) vs semaine précédente
/// (pointillé conceptuel — ici second tracé plus fin). Doc §5.6.
pub fn draw_week_radar(
    ui: &mut egui::Ui,
    size: f32,
    current: Dimensions,
    previous: Option<Dimensions>,
) {
    let (rect, _response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let radius = rect.width().min(rect.height()) / 2.0 - 28.0;
    let axis_color = egui::Color32::from_gray(90);
    let current_color = egui::Color32::from_rgb(255, 46, 136);
    let previous_color = egui::Color32::from_rgb(157, 0, 255);

    let as_array = |d: Dimensions| -> [f64; 5] {
        [
            d.epuisement,
            d.cognitif,
            d.sensoriel,
            d.masquage,
            d.fonctionnement,
        ]
    };
    let point_for = |value: f64, i: usize| -> egui::Pos2 {
        let angle = std::f32::consts::TAU * (i as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
        let r = radius * (value.clamp(0.0, 5.0) / 5.0) as f32;
        center + egui::vec2(angle.cos(), angle.sin()) * r
    };

    for (i, label) in crate::logic::DIMENSION_LABELS.iter().enumerate() {
        let angle = std::f32::consts::TAU * (i as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
        let edge = center + egui::vec2(angle.cos(), angle.sin()) * radius;
        painter.line_segment([center, edge], egui::Stroke::new(1.0_f32, axis_color));
        let label_pos = center + egui::vec2(angle.cos(), angle.sin()) * (radius + 16.0);
        painter.text(
            label_pos,
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(200),
        );
    }
    for ring in 1..=5 {
        let pts: Vec<egui::Pos2> = (0..5).map(|i| point_for(f64::from(ring), i)).collect();
        painter.add(egui::Shape::closed_line(
            pts,
            egui::Stroke::new(0.5_f32, axis_color.gamma_multiply(0.5)),
        ));
    }
    if let Some(prev) = previous {
        let vals = as_array(prev);
        let pts: Vec<egui::Pos2> = (0..5).map(|i| point_for(vals[i], i)).collect();
        painter.add(egui::Shape::closed_line(
            pts,
            egui::Stroke::new(1.5_f32, previous_color),
        ));
    }
    let vals = as_array(current);
    let pts: Vec<egui::Pos2> = (0..5).map(|i| point_for(vals[i], i)).collect();
    painter.add(egui::Shape::closed_line(
        pts,
        egui::Stroke::new(2.0_f32, current_color),
    ));
}

/// Barres groupées créées/terminées par semaine + ligne épuisement moyen
/// (échelle 0-5 secondaire). Doc §5.6 : « Charge tâches → épuisement
/// (créées vs terminées vs score épuisement) ».
pub fn draw_weekly_load_chart(ui: &mut egui::Ui, size: egui::Vec2, load: &[WeeklyLoad]) {
    let (outer, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if load.is_empty() {
        return;
    }
    let label_h = 14.0;
    let rect = egui::Rect::from_min_max(outer.min, egui::pos2(outer.max.x, outer.max.y - label_h));
    let painter = ui.painter_at(outer);
    let max_count = load
        .iter()
        .flat_map(|w| [w.created, w.done])
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let n = load.len() as f32;
    let group_w = (rect.width() / n).max(2.0);
    let created_color = egui::Color32::from_rgb(123, 0, 255);
    let done_color = egui::Color32::from_rgb(120, 255, 180);
    let epuisement_color = egui::Color32::from_rgb(255, 46, 136);
    // Au plus ~8 étiquettes affichées, réparties uniformément (évite le
    // chevauchement de texte quand la fenêtre couvre de nombreuses semaines).
    let label_every = (load.len().div_ceil(8)).max(1);

    let y_for_count = |c: u32| -> f32 { rect.bottom() - (c as f32 / max_count) * rect.height() };
    let y_for_epuisement = |e: f64| -> f32 { rect.bottom() - (e / 5.0) as f32 * rect.height() };

    let mut epuisement_pts: Vec<Option<egui::Pos2>> = Vec::new();
    for (i, w) in load.iter().enumerate() {
        let x0 = rect.left() + i as f32 * group_w;
        let bar_w = (group_w - 2.0) / 2.0;
        let created_rect = egui::Rect::from_min_max(
            egui::pos2(x0, y_for_count(w.created)),
            egui::pos2(x0 + bar_w, rect.bottom()),
        );
        painter.rect_filled(created_rect, 0.0, created_color.gamma_multiply(0.6));
        let done_rect = egui::Rect::from_min_max(
            egui::pos2(x0 + bar_w + 1.0, y_for_count(w.done)),
            egui::pos2(x0 + 2.0 * bar_w + 1.0, rect.bottom()),
        );
        painter.rect_filled(done_rect, 0.0, done_color.gamma_multiply(0.6));
        epuisement_pts.push(
            w.avg_epuisement
                .map(|e| egui::pos2(x0 + group_w / 2.0, y_for_epuisement(e))),
        );
        if i % label_every == 0 {
            painter.text(
                egui::pos2(x0 + group_w / 2.0, rect.bottom() + 2.0),
                egui::Align2::CENTER_TOP,
                crate::correlations::week_label(w.week),
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(180),
            );
        }
    }
    for pair in epuisement_pts.windows(2) {
        if let [Some(a), Some(b)] = pair {
            painter.line_segment([*a, *b], egui::Stroke::new(2.0_f32, epuisement_color));
        }
    }
}

/// Barres : tâches terminées par semaine (doc §5.6 : « vélocité
/// hebdomadaire »).
pub fn draw_velocity_chart(ui: &mut egui::Ui, size: egui::Vec2, load: &[WeeklyLoad]) {
    let (outer, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if load.is_empty() {
        return;
    }
    let label_h = 14.0;
    let rect = egui::Rect::from_min_max(outer.min, egui::pos2(outer.max.x, outer.max.y - label_h));
    let painter = ui.painter_at(outer);
    let max_done = load.iter().map(|w| w.done).max().unwrap_or(1).max(1) as f32;
    let n = load.len() as f32;
    let bar_w = (rect.width() / n).max(2.0);
    let color = egui::Color32::from_rgb(120, 255, 180);
    for (i, w) in load.iter().enumerate() {
        let x0 = rect.left() + i as f32 * bar_w;
        let y = rect.bottom() - (w.done as f32 / max_done) * rect.height();
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(x0 + 1.0, y),
            egui::pos2(x0 + bar_w - 1.0, rect.bottom()),
        );
        painter.rect_filled(bar_rect, 0.0, color.gamma_multiply(0.7));
        painter.text(
            egui::pos2(x0 + bar_w / 2.0, rect.bottom() + 2.0),
            egui::Align2::CENTER_TOP,
            crate::correlations::week_label(w.week),
            egui::FontId::proportional(9.0),
            egui::Color32::from_gray(180),
        );
    }
}

/// Barres GROUPÉES (une barre par médicament, par semaine) + ligne
/// fonctionnement moyen (une valeur par semaine, jamais mélangée entre
/// médicaments — c'est le point de grouper). Doc §5.6 : « Observance
/// médication → fonctionnement semaine (barres groupées) » — voir écart
/// documenté (correlations.rs) sur la fréquence attendue absente du schéma.
pub fn draw_med_observance_chart(ui: &mut egui::Ui, size: egui::Vec2, weeks: &[MedObservanceWeek]) {
    let (outer, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if weeks.is_empty() {
        return;
    }
    let label_h = 14.0;
    let rect = egui::Rect::from_min_max(outer.min, egui::pos2(outer.max.x, outer.max.y - label_h));
    let painter = ui.painter_at(outer);
    let max_count = weeks
        .iter()
        .flat_map(|w| w.doses_by_med.iter().map(|(_, c)| *c))
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let n = weeks.len() as f32;
    let group_w = (rect.width() / n).max(2.0);
    let dose_colors = [
        egui::Color32::from_rgb(123, 0, 255),
        egui::Color32::from_rgb(0, 200, 255),
        egui::Color32::from_rgb(255, 200, 0),
    ];
    let fonctionnement_color = egui::Color32::from_rgb(120, 255, 180);
    let label_every = (weeks.len().div_ceil(8)).max(1);

    let y_for_count = |c: u32| -> f32 { rect.bottom() - (c as f32 / max_count) * rect.height() };
    let y_for_fonctionnement = |f: f64| -> f32 { rect.bottom() - (f / 5.0) as f32 * rect.height() };

    let mut fonctionnement_pts: Vec<Option<egui::Pos2>> = Vec::new();
    for (i, w) in weeks.iter().enumerate() {
        let x0 = rect.left() + i as f32 * group_w;
        let med_count = w.doses_by_med.len().max(1) as f32;
        let bar_w = (group_w - 2.0) / med_count;
        for (j, (_, count)) in w.doses_by_med.iter().enumerate() {
            let bx0 = x0 + j as f32 * bar_w;
            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(bx0, y_for_count(*count)),
                egui::pos2(bx0 + bar_w - 1.0, rect.bottom()),
            );
            let color = dose_colors[j % dose_colors.len()];
            painter.rect_filled(bar_rect, 0.0, color.gamma_multiply(0.6));
        }
        fonctionnement_pts.push(
            w.avg_fonctionnement
                .map(|f| egui::pos2(x0 + group_w / 2.0, y_for_fonctionnement(f))),
        );
        if i % label_every == 0 {
            painter.text(
                egui::pos2(x0 + group_w / 2.0, rect.bottom() + 2.0),
                egui::Align2::CENTER_TOP,
                crate::correlations::week_label(w.week),
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(180),
            );
        }
    }
    for pair in fonctionnement_pts.windows(2) {
        if let [Some(a), Some(b)] = pair {
            painter.line_segment([*a, *b], egui::Stroke::new(2.0_f32, fonctionnement_color));
        }
    }
}

/// 4 barres (low/medium/high/sans énergie) — doc §5.6 : « Répartition par
/// énergie des tâches terminées ».
pub fn draw_energy_breakdown(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    low: u32,
    medium: u32,
    high: u32,
    none: u32,
) {
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let values = [low, medium, high, none];
    let labels = ["low", "medium", "high", "?"];
    let colors = [
        egui::Color32::from_rgb(120, 255, 180),
        egui::Color32::from_rgb(255, 200, 0),
        egui::Color32::from_rgb(255, 46, 136),
        egui::Color32::from_gray(120),
    ];
    let max = values.iter().copied().max().unwrap_or(1).max(1) as f32;
    let painter = ui.painter_at(rect);
    let bar_w = rect.width() / 4.0;
    for (i, (&v, &color)) in values.iter().zip(colors.iter()).enumerate() {
        let x0 = rect.left() + i as f32 * bar_w;
        let y = rect.bottom() - (v as f32 / max) * (rect.height() - 16.0);
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(x0 + 4.0, y),
            egui::pos2(x0 + bar_w - 4.0, rect.bottom() - 14.0),
        );
        painter.rect_filled(bar_rect, 0.0, color.gamma_multiply(0.7));
        painter.text(
            egui::pos2(x0 + bar_w / 2.0, rect.bottom() - 6.0),
            egui::Align2::CENTER_CENTER,
            format!("{} ({v})", labels[i]),
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(200),
        );
    }
}

/// Nuage de points générique, axes auto-échelonnés sur les bornes réelles
/// des données (doc §5.6 : « scatter » pour sommeil→cognitif et courbe
/// empirique médication). `points` : (x, y). Rien n'est peint si vide —
/// l'appelant affiche « référentiel insuffisant » lui-même. Un seul point,
/// ou des points alignés sur un axe, sont centrés sans division par zéro
/// (span nul remplacé par `f64::EPSILON`).
pub fn draw_scatter(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    points: &[(f64, f64)],
    color: egui::Color32,
) {
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if points.is_empty() {
        return;
    }
    let painter = ui.painter_at(rect);
    let (min_x, max_x) = min_max(points.iter().map(|(x, _)| *x));
    let (min_y, max_y) = min_max(points.iter().map(|(_, y)| *y));
    let span_x = (max_x - min_x).max(f64::EPSILON);
    let span_y = (max_y - min_y).max(f64::EPSILON);
    let margin = 8.0;
    let x_for = |x: f64| -> f32 {
        rect.left() + margin + ((x - min_x) / span_x) as f32 * (rect.width() - 2.0 * margin)
    };
    let y_for = |y: f64| -> f32 {
        rect.bottom() - margin - ((y - min_y) / span_y) as f32 * (rect.height() - 2.0 * margin)
    };
    for (x, y) in points {
        painter.circle_filled(
            egui::pos2(x_for(*x), y_for(*y)),
            3.0,
            color.gamma_multiply(0.75),
        );
    }
}

fn min_max(values: impl Iterator<Item = f64>) -> (f64, f64) {
    values.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
}
