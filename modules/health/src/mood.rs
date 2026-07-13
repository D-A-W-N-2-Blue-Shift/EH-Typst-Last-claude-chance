// ============================================================================
// modules/health/src/mood.rs — Sous-vue État psy (doc §5.3)
//
// 5 dimensions (épuisement/cognitif/sensoriel/masquage/fonctionnement),
// saisie 1-5, notes optionnelles. Base EBM : dimensions issues de l'ABM
// (AASPIRE Autistic Burnout Measure) + littérature Raymaker 2020 (doc §5.3).
// Affiche immédiatement un radar des 5 dimensions vs moyenne 7 jours.
// ============================================================================

use chrono::NaiveDate;

/// Les 5 dimensions de l'état psy, dans l'ordre d'affichage du radar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dimensions {
    pub epuisement: f64,
    pub cognitif: f64,
    pub sensoriel: f64,
    pub masquage: f64,
    pub fonctionnement: f64,
}

impl Dimensions {
    pub fn as_array(&self) -> [f64; 5] {
        [
            self.epuisement,
            self.cognitif,
            self.sensoriel,
            self.masquage,
            self.fonctionnement,
        ]
    }
}

pub const LABELS: [&str; 5] = [
    "Épuisement",
    "Cognitif",
    "Sensoriel",
    "Masquage",
    "Fonctionnement",
];

/// Moyenne des 5 dimensions sur les `window_days` derniers jours (bornes
/// incluses) depuis `as_of`. `logged_at` est un timestamp ISO complet
/// (ex: "2026-07-12T20:00:00") ; seule la partie date est comparée. `None`
/// si aucune entrée dans la fenêtre.
pub fn rolling_average_dimensions(
    logs: &[nexus_db::MoodLog],
    as_of: NaiveDate,
    window_days: i64,
) -> Option<Dimensions> {
    let cutoff = as_of - chrono::Duration::days(window_days.max(1) - 1);
    let mut sums = Dimensions {
        epuisement: 0.0,
        cognitif: 0.0,
        sensoriel: 0.0,
        masquage: 0.0,
        fonctionnement: 0.0,
    };
    let mut n: u32 = 0;
    for log in logs {
        let date_part = log.logged_at.split('T').next().unwrap_or(&log.logged_at);
        let Ok(d) = NaiveDate::parse_from_str(date_part, "%Y-%m-%d") else {
            continue;
        };
        if d < cutoff || d > as_of {
            continue;
        }
        sums.epuisement += log.epuisement as f64;
        sums.cognitif += log.cognitif as f64;
        sums.sensoriel += log.sensoriel as f64;
        sums.masquage += log.masquage as f64;
        sums.fonctionnement += log.fonctionnement as f64;
        n += 1;
    }
    if n == 0 {
        return None;
    }
    let n = f64::from(n);
    Some(Dimensions {
        epuisement: sums.epuisement / n,
        cognitif: sums.cognitif / n,
        sensoriel: sums.sensoriel / n,
        masquage: sums.masquage / n,
        fonctionnement: sums.fonctionnement / n,
    })
}

/// État du formulaire de saisie état psy (brouillon, pas encore enregistré).
pub struct MoodForm {
    pub epuisement: u8,
    pub cognitif: u8,
    pub sensoriel: u8,
    pub masquage: u8,
    pub fonctionnement: u8,
    pub notes: String,
}

impl Default for MoodForm {
    fn default() -> Self {
        Self {
            epuisement: 3,
            cognitif: 3,
            sensoriel: 3,
            masquage: 3,
            fonctionnement: 3,
            notes: String::new(),
        }
    }
}

/// Dessine un radar à 5 axes (valeurs 0-5). `average` optionnel s'affiche en
/// second tracé. APIs vérifiées contre la source vendue egui/epaint/ecolor
/// 0.31.1 (Shape::closed_line, Painter::text, FontId::proportional,
/// Color32::{from_gray,gamma_multiply}) — aucune n'est inventée.
pub fn draw_radar(ui: &mut egui::Ui, size: f32, current: Dimensions, average: Option<Dimensions>) {
    let (rect, _response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let radius = rect.width().min(rect.height()) / 2.0 - 28.0;
    let axis_color = egui::Color32::from_gray(90);
    let current_color = egui::Color32::from_rgb(255, 46, 136);
    let average_color = egui::Color32::from_rgb(157, 0, 255);

    let point_for = |value: f64, i: usize| -> egui::Pos2 {
        let angle = std::f32::consts::TAU * (i as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
        let r = radius * (value.clamp(0.0, 5.0) / 5.0) as f32;
        center + egui::vec2(angle.cos(), angle.sin()) * r
    };

    // Axes + étiquettes.
    for (i, label) in LABELS.iter().enumerate() {
        let angle = std::f32::consts::TAU * (i as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
        let edge = center + egui::vec2(angle.cos(), angle.sin()) * radius;
        painter.line_segment([center, edge], egui::Stroke::new(1.0, axis_color));
        let label_pos = center + egui::vec2(angle.cos(), angle.sin()) * (radius + 16.0);
        painter.text(
            label_pos,
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(200),
        );
    }
    // Anneaux de fond (graduations 1..5).
    for ring in 1..=5 {
        let pts: Vec<egui::Pos2> = (0..5).map(|i| point_for(f64::from(ring), i)).collect();
        painter.add(egui::Shape::closed_line(
            pts,
            egui::Stroke::new(0.5, axis_color.gamma_multiply(0.5)),
        ));
    }

    if let Some(avg) = average {
        let vals = avg.as_array();
        let pts: Vec<egui::Pos2> = (0..5).map(|i| point_for(vals[i], i)).collect();
        painter.add(egui::Shape::closed_line(
            pts,
            egui::Stroke::new(1.5, average_color),
        ));
    }
    let vals = current.as_array();
    let pts: Vec<egui::Pos2> = (0..5).map(|i| point_for(vals[i], i)).collect();
    painter.add(egui::Shape::closed_line(
        pts,
        egui::Stroke::new(2.0, current_color),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(logged_at: &str, v: [i64; 5]) -> nexus_db::MoodLog {
        nexus_db::MoodLog {
            id: logged_at.to_string(),
            logged_at: logged_at.to_string(),
            epuisement: v[0],
            cognitif: v[1],
            sensoriel: v[2],
            masquage: v[3],
            fonctionnement: v[4],
            notes: None,
        }
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date de test valide")
    }

    #[test]
    fn moyenne_vide_sans_entree() {
        assert_eq!(rolling_average_dimensions(&[], date("2026-07-12"), 7), None);
    }

    #[test]
    fn moyenne_calcule_chaque_dimension_independamment() {
        let logs = vec![
            log("2026-07-11T09:00:00", [4, 2, 3, 5, 2]),
            log("2026-07-12T09:00:00", [2, 4, 1, 3, 4]),
        ];
        let avg = rolling_average_dimensions(&logs, date("2026-07-12"), 7).expect("moyenne");
        assert!((avg.epuisement - 3.0).abs() < 1e-9);
        assert!((avg.cognitif - 3.0).abs() < 1e-9);
        assert!((avg.sensoriel - 2.0).abs() < 1e-9);
        assert!((avg.masquage - 4.0).abs() < 1e-9);
        assert!((avg.fonctionnement - 3.0).abs() < 1e-9);
    }

    #[test]
    fn moyenne_ignore_hors_fenetre_7_jours() {
        let logs = vec![
            log("2026-06-01T09:00:00", [5, 5, 5, 5, 5]), // hors fenêtre
            log("2026-07-12T09:00:00", [1, 1, 1, 1, 1]),
        ];
        let avg = rolling_average_dimensions(&logs, date("2026-07-12"), 7).expect("moyenne");
        assert!(
            (avg.epuisement - 1.0).abs() < 1e-9,
            "l'entrée hors fenêtre a pollué la moyenne"
        );
    }

    #[test]
    fn moyenne_ignore_timestamps_illisibles() {
        let logs = vec![
            log("2026-07-12T09:00:00", [3, 3, 3, 3, 3]),
            log("pas-un-timestamp", [5, 5, 5, 5, 5]),
        ];
        let avg = rolling_average_dimensions(&logs, date("2026-07-12"), 7).expect("moyenne");
        assert!((avg.epuisement - 3.0).abs() < 1e-9);
    }
}
