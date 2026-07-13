// ============================================================================
// modules/health/src/sleep.rs — Sous-vue Sommeil (doc §5.3)
//
// Formulaire minimal : durée en heures (décimal), qualité 1-5, notes
// optionnelles. Affiche immédiatement la durée vs moyenne personnelle
// glissante 28 jours, recalculée depuis la DB à chaque saisie — jamais
// stockée en dur (doc §5.3 : « La moyenne est recalculée à chaque saisie
// depuis la DB »).
// ============================================================================

use chrono::NaiveDate;

/// Moyenne des durées de sommeil sur les `window_days` derniers jours
/// (bornes incluses), calculée depuis `as_of`. `None` si aucune entrée dans
/// la fenêtre — jamais de moyenne inventée sur un échantillon vide.
pub fn rolling_average_duration(
    logs: &[nexus_db::SleepLog],
    as_of: NaiveDate,
    window_days: i64,
) -> Option<f64> {
    let cutoff = as_of - chrono::Duration::days(window_days.max(1) - 1);
    let mut sum = 0.0;
    let mut n: u32 = 0;
    for log in logs {
        let Ok(d) = NaiveDate::parse_from_str(&log.date, "%Y-%m-%d") else {
            continue;
        };
        if d >= cutoff && d <= as_of {
            sum += log.duration_h;
            n += 1;
        }
    }
    if n == 0 {
        None
    } else {
        Some(sum / f64::from(n))
    }
}

/// Écart en % entre `duration_h` et `average`. `None` si pas de moyenne
/// disponible (échantillon insuffisant) ou moyenne nulle (division
/// impossible — ne jamais diviser par zéro).
pub fn pct_deviation(duration_h: f64, average: Option<f64>) -> Option<f64> {
    let avg = average?;
    if avg == 0.0 {
        return None;
    }
    Some((duration_h - avg) / avg * 100.0)
}

/// État du formulaire de saisie sommeil (brouillon, pas encore enregistré).
pub struct SleepForm {
    pub duration_h: f32,
    pub quality: u8,
    pub notes: String,
}

impl Default for SleepForm {
    fn default() -> Self {
        Self {
            duration_h: 7.5,
            quality: 3,
            notes: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(date: &str, duration_h: f64) -> nexus_db::SleepLog {
        nexus_db::SleepLog {
            id: date.to_string(),
            date: date.to_string(),
            duration_h,
            quality: 3,
            notes: None,
        }
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date de test valide")
    }

    #[test]
    fn moyenne_vide_sans_entree() {
        assert_eq!(rolling_average_duration(&[], date("2026-07-12"), 28), None);
    }

    #[test]
    fn moyenne_ignore_les_entrees_hors_fenetre() {
        let logs = vec![
            log("2026-06-01", 4.0), // hors fenêtre 28 jours avant le 12/07
            log("2026-07-10", 8.0),
            log("2026-07-11", 6.0),
        ];
        // Fenêtre [2026-06-15 .. 2026-07-12] : seules les deux dernières comptent.
        let avg = rolling_average_duration(&logs, date("2026-07-12"), 28).expect("moyenne");
        assert!((avg - 7.0).abs() < 1e-9, "attendu 7.0, obtenu {avg}");
    }

    #[test]
    fn moyenne_ignore_les_dates_illisibles() {
        let mut logs = vec![log("2026-07-11", 8.0)];
        logs.push(nexus_db::SleepLog {
            id: "bad".into(),
            date: "pas-une-date".into(),
            duration_h: 99.0,
            quality: 3,
            notes: None,
        });
        let avg = rolling_average_duration(&logs, date("2026-07-12"), 28).expect("moyenne");
        assert!(
            (avg - 8.0).abs() < 1e-9,
            "l'entrée illisible doit être ignorée : {avg}"
        );
    }

    #[test]
    fn deviation_positive_et_negative() {
        // Comparaison à epsilon, pas assert_eq! : l'ordre des opérations
        // flottantes ((a-b)/b*100 vs a calcul équivalent) peut différer au
        // dernier bit sans que le résultat soit faux.
        let up = pct_deviation(8.0, Some(7.0)).expect("déviation calculable");
        assert!((up - 100.0 / 7.0).abs() < 1e-9, "obtenu {up}");
        let down = pct_deviation(6.0, Some(7.0)).expect("déviation calculable");
        assert!((down - (-100.0 / 7.0)).abs() < 1e-9, "obtenu {down}");
    }

    #[test]
    fn deviation_none_sans_moyenne() {
        assert_eq!(pct_deviation(8.0, None), None);
    }

    #[test]
    fn deviation_none_si_moyenne_nulle() {
        assert_eq!(pct_deviation(8.0, Some(0.0)), None);
    }
}
