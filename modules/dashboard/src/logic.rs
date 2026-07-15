// ============================================================================
// modules/dashboard/src/logic.rs — Logique pure du dashboard (doc §5.6)
//
// Aucune donnée inventée : chaque fonction retourne `None`/une liste vide
// plutôt qu'une extrapolation quand l'échantillon est insuffisant. Rien
// ici ne touche SQLite ni egui : tout est testable en isolation.
// ============================================================================

use chrono::NaiveDate;

/// Seuil minimal de points pour afficher un référentiel (doc §5.6 : «
/// indication explicite quand N < 7 »).
pub const MIN_SAMPLE_SLEEP: usize = 7;

fn sleep_date(log: &nexus_db::SleepLog) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&log.date, "%Y-%m-%d").ok()
}

fn mood_date(log: &nexus_db::MoodLog) -> Option<NaiveDate> {
    let date_part = log.logged_at.split('T').next().unwrap_or(&log.logged_at);
    NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()
}

/// Nuits dans la fenêtre `[as_of - window_days + 1, as_of]`, triées par
/// date croissante (ordre d'affichage d'un graphique).
pub fn nights_in_window(
    logs: &[nexus_db::SleepLog],
    as_of: NaiveDate,
    window_days: i64,
) -> Vec<nexus_db::SleepLog> {
    let cutoff = as_of - chrono::Duration::days(window_days.max(1) - 1);
    let mut out: Vec<nexus_db::SleepLog> = logs
        .iter()
        .filter(|l| sleep_date(l).is_some_and(|d| d >= cutoff && d <= as_of))
        .cloned()
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date));
    out
}

/// Moyenne glissante 28 jours calculée À CHAQUE nuit affichée (pas une
/// valeur unique figée) — doc §5.6 : « moyenne personnelle glissante 28
/// jours ». `None` par point si moins de `MIN_SAMPLE_SLEEP` nuits dans SA
/// propre fenêtre de 28 jours à cette date.
pub fn rolling_average_series(
    all_logs: &[nexus_db::SleepLog],
    displayed: &[nexus_db::SleepLog],
) -> Vec<Option<f64>> {
    displayed
        .iter()
        .map(|night| {
            let Some(d) = sleep_date(night) else {
                return None;
            };
            let window = nights_in_window(all_logs, d, 28);
            if window.len() < MIN_SAMPLE_SLEEP {
                return None;
            }
            let sum: f64 = window.iter().map(|l| l.duration_h).sum();
            Some(sum / window.len() as f64)
        })
        .collect()
}

/// Les 5 dimensions de l'état psy (copie locale, dashboard n'importe pas
/// health::mood — modules pairs du même registre, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Dimensions {
    pub epuisement: f64,
    pub cognitif: f64,
    pub sensoriel: f64,
    pub masquage: f64,
    pub fonctionnement: f64,
}

pub const DIMENSION_LABELS: [&str; 5] = [
    "Épuisement",
    "Cognitif",
    "Sensoriel",
    "Masquage",
    "Fonctionnement",
];

/// Moyenne des 5 dimensions sur les 7 jours se terminant à `week_end`
/// (bornes incluses). `None` si aucune entrée.
pub fn week_average(logs: &[nexus_db::MoodLog], week_end: NaiveDate) -> Option<Dimensions> {
    let cutoff = week_end - chrono::Duration::days(6);
    let in_week: Vec<&nexus_db::MoodLog> = logs
        .iter()
        .filter(|l| mood_date(l).is_some_and(|d| d >= cutoff && d <= week_end))
        .collect();
    if in_week.is_empty() {
        return None;
    }
    let n = in_week.len() as f64;
    Some(Dimensions {
        epuisement: in_week.iter().map(|l| l.epuisement as f64).sum::<f64>() / n,
        cognitif: in_week.iter().map(|l| l.cognitif as f64).sum::<f64>() / n,
        sensoriel: in_week.iter().map(|l| l.sensoriel as f64).sum::<f64>() / n,
        masquage: in_week.iter().map(|l| l.masquage as f64).sum::<f64>() / n,
        fonctionnement: in_week.iter().map(|l| l.fonctionnement as f64).sum::<f64>() / n,
    })
}

/// Moyenne quotidienne des 5 dimensions sur `window_days` jours (une entrée
/// par jour où au moins une saisie existe — doc §5.4 : plusieurs saisies/j
/// possibles). Triée par date croissante — doc §5.6 : « tendance 30 jours
/// par dimension (ligne par ligne) ».
pub fn daily_trend(
    logs: &[nexus_db::MoodLog],
    as_of: NaiveDate,
    window_days: i64,
) -> Vec<(NaiveDate, Dimensions)> {
    let cutoff = as_of - chrono::Duration::days(window_days.max(1) - 1);
    let mut by_day: std::collections::BTreeMap<NaiveDate, (Dimensions, u32)> =
        std::collections::BTreeMap::new();
    for l in logs {
        let Some(d) = mood_date(l) else { continue };
        if d < cutoff || d > as_of {
            continue;
        }
        let entry = by_day.entry(d).or_default();
        entry.0.epuisement += l.epuisement as f64;
        entry.0.cognitif += l.cognitif as f64;
        entry.0.sensoriel += l.sensoriel as f64;
        entry.0.masquage += l.masquage as f64;
        entry.0.fonctionnement += l.fonctionnement as f64;
        entry.1 += 1;
    }
    by_day
        .into_iter()
        .map(|(d, (sum, n))| {
            let n = f64::from(n);
            (
                d,
                Dimensions {
                    epuisement: sum.epuisement / n,
                    cognitif: sum.cognitif / n,
                    sensoriel: sum.sensoriel / n,
                    masquage: sum.masquage / n,
                    fonctionnement: sum.fonctionnement / n,
                },
            )
        })
        .collect()
}

/// Dimensions sous `threshold` pendant `consecutive_days` jours consécutifs
/// jusqu'à `as_of` (doc §5.6 : « alerte visuelle si une dimension < 2 sur 3
/// jours consécutifs » — seuil et fenêtre configurables, doc §5.7, section
/// "dashboard" de Hive_RBMK.ron, valeurs par défaut 2.0/3 identiques au
/// comportement précédemment codé en dur). Nécessite une saisie chaque jour
/// de la fenêtre : un jour manquant casse la séquence (aucune extrapolation).
pub fn low_dimension_alerts(
    logs: &[nexus_db::MoodLog],
    as_of: NaiveDate,
    threshold: f64,
    consecutive_days: i64,
) -> Vec<&'static str> {
    if consecutive_days < 1 {
        return Vec::new();
    }
    let window = consecutive_days as usize;
    let trend = daily_trend(logs, as_of, consecutive_days);
    if trend.len() < window {
        return Vec::new();
    }
    let last_n = &trend[trend.len() - window..];
    let all_consecutive = last_n
        .iter()
        .enumerate()
        .all(|(i, (d, _))| *d == as_of - chrono::Duration::days((window - 1 - i) as i64));
    if !all_consecutive {
        return Vec::new();
    }
    let mut alerts = Vec::new();
    let checks: [(fn(&Dimensions) -> f64, &'static str); 5] = [
        (|d| d.epuisement, "Épuisement"),
        (|d| d.cognitif, "Cognitif"),
        (|d| d.sensoriel, "Sensoriel"),
        (|d| d.masquage, "Masquage"),
        (|d| d.fonctionnement, "Fonctionnement"),
    ];
    for (accessor, label) in checks {
        if last_n.iter().all(|(_, dim)| accessor(dim) < threshold) {
            alerts.push(label);
        }
    }
    alerts
}

/// Sérialise les nuits en CSV (doc §5.6 : « les données brutes de chaque
/// vue sont exportables en CSV »). Échappement minimal : guillemets doublés
/// si le champ contient un guillemet, une virgule ou un saut de ligne.
pub fn sleep_logs_to_csv(logs: &[nexus_db::SleepLog]) -> String {
    let mut out = String::from("date,duration_h,quality,notes\n");
    for l in logs {
        out.push_str(&format!(
            "{},{},{},{}\n",
            csv_field(&l.date),
            l.duration_h,
            l.quality,
            csv_field(l.notes.as_deref().unwrap_or(""))
        ));
    }
    out
}

pub fn mood_logs_to_csv(logs: &[nexus_db::MoodLog]) -> String {
    let mut out =
        String::from("logged_at,epuisement,cognitif,sensoriel,masquage,fonctionnement,notes\n");
    for l in logs {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_field(&l.logged_at),
            l.epuisement,
            l.cognitif,
            l.sensoriel,
            l.masquage,
            l.fonctionnement,
            csv_field(l.notes.as_deref().unwrap_or(""))
        ));
    }
    out
}

fn csv_field(raw: &str) -> String {
    if raw.contains(['"', ',', '\n']) {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date de test valide")
    }

    fn sleep(d: &str, duration_h: f64) -> nexus_db::SleepLog {
        nexus_db::SleepLog {
            id: d.to_string(),
            date: d.to_string(),
            duration_h,
            quality: 3,
            notes: None,
        }
    }

    fn mood(logged_at: &str, v: [i64; 5]) -> nexus_db::MoodLog {
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

    #[test]
    fn nights_in_window_trie_et_filtre() {
        let logs = vec![
            sleep("2026-07-05", 6.0),
            sleep("2026-06-01", 8.0), // hors fenêtre
            sleep("2026-07-01", 7.0),
        ];
        let win = nights_in_window(&logs, date("2026-07-05"), 7);
        assert_eq!(win.len(), 2);
        assert_eq!(win[0].date, "2026-07-01");
        assert_eq!(win[1].date, "2026-07-05");
    }

    #[test]
    fn rolling_average_series_none_sous_le_seuil() {
        let logs = vec![sleep("2026-07-05", 7.0)];
        let displayed = nights_in_window(&logs, date("2026-07-05"), 30);
        let series = rolling_average_series(&logs, &displayed);
        assert_eq!(series, vec![None], "1 nuit < MIN_SAMPLE_SLEEP (7)");
    }

    #[test]
    fn rolling_average_series_calcule_quand_suffisant() {
        let mut logs = Vec::new();
        for day in 1..=7 {
            logs.push(sleep(&format!("2026-07-{day:02}"), 8.0));
        }
        let displayed = nights_in_window(&logs, date("2026-07-07"), 30);
        let series = rolling_average_series(&logs, &displayed);
        assert_eq!(series.last().copied().flatten(), Some(8.0));
    }

    #[test]
    fn week_average_none_sans_donnee() {
        assert_eq!(week_average(&[], date("2026-07-12")), None);
    }

    #[test]
    fn week_average_calcule_sur_7_jours() {
        let logs = vec![
            mood("2026-07-06T09:00:00", [4, 4, 4, 4, 4]),
            mood("2026-07-12T09:00:00", [2, 2, 2, 2, 2]),
            mood("2026-06-01T09:00:00", [5, 5, 5, 5, 5]), // hors semaine
        ];
        let avg = week_average(&logs, date("2026-07-12")).expect("moyenne");
        assert!((avg.epuisement - 3.0).abs() < 1e-9);
    }

    #[test]
    fn daily_trend_moyenne_les_saisies_multiples_du_meme_jour() {
        let logs = vec![
            mood("2026-07-12T09:00:00", [2, 2, 2, 2, 2]),
            mood("2026-07-12T20:00:00", [4, 4, 4, 4, 4]),
        ];
        let trend = daily_trend(&logs, date("2026-07-12"), 30);
        assert_eq!(trend.len(), 1);
        assert!((trend[0].1.epuisement - 3.0).abs() < 1e-9);
    }

    #[test]
    fn low_dimension_alerts_declenche_sur_3_jours_consecutifs() {
        let logs = vec![
            mood("2026-07-10T09:00:00", [1, 5, 5, 5, 5]),
            mood("2026-07-11T09:00:00", [1, 5, 5, 5, 5]),
            mood("2026-07-12T09:00:00", [1, 5, 5, 5, 5]),
        ];
        let alerts = low_dimension_alerts(&logs, date("2026-07-12"), 2.0, 3);
        assert_eq!(alerts, vec!["Épuisement"]);
    }

    #[test]
    fn low_dimension_alerts_pas_declenche_si_jour_manquant() {
        let logs = vec![
            mood("2026-07-10T09:00:00", [1, 5, 5, 5, 5]),
            // 07-11 manquant : séquence cassée.
            mood("2026-07-12T09:00:00", [1, 5, 5, 5, 5]),
        ];
        assert_eq!(
            low_dimension_alerts(&logs, date("2026-07-12"), 2.0, 3),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn low_dimension_alerts_pas_declenche_si_score_repasse_a_2() {
        let logs = vec![
            mood("2026-07-10T09:00:00", [1, 5, 5, 5, 5]),
            mood("2026-07-11T09:00:00", [2, 5, 5, 5, 5]), // repasse à 2 : plus < 2
            mood("2026-07-12T09:00:00", [1, 5, 5, 5, 5]),
        ];
        assert_eq!(
            low_dimension_alerts(&logs, date("2026-07-12"), 2.0, 3),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn low_dimension_alerts_seuil_et_fenetre_configurables() {
        let logs = vec![
            mood("2026-07-11T09:00:00", [3, 5, 5, 5, 5]),
            mood("2026-07-12T09:00:00", [3, 5, 5, 5, 5]),
        ];
        // Défauts (seuil 2.0, fenêtre 3j) : ni le score (3 >= 2) ni la
        // fenêtre (2 jours de saisie < 3) ne suffisent — pas d'alerte.
        assert_eq!(
            low_dimension_alerts(&logs, date("2026-07-12"), 2.0, 3),
            Vec::<&str>::new()
        );
        // Seuil et fenêtre élargis : l'alerte se déclenche — preuve que les
        // paramètres sont réellement consommés, pas juste acceptés et
        // ignorés (doc §5.7 : le Cockpit n'expose que du config réel).
        assert_eq!(
            low_dimension_alerts(&logs, date("2026-07-12"), 4.0, 2),
            vec!["Épuisement"]
        );
    }

    #[test]
    fn csv_sleep_echappe_les_virgules() {
        let logs = vec![nexus_db::SleepLog {
            id: "1".into(),
            date: "2026-07-12".into(),
            duration_h: 7.5,
            quality: 4,
            notes: Some("nuit, agitée".into()),
        }];
        let csv = sleep_logs_to_csv(&logs);
        assert!(csv.contains("\"nuit, agitée\""));
    }

    #[test]
    fn csv_mood_en_tete_et_une_ligne_par_entree() {
        let logs = vec![mood("2026-07-12T09:00:00", [1, 2, 3, 4, 5])];
        let csv = mood_logs_to_csv(&logs);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("logged_at,epuisement"));
        assert!(lines[1].starts_with("2026-07-12T09:00:00,1,2,3,4,5"));
    }
}
