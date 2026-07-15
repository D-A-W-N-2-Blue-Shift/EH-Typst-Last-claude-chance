// ============================================================================
// tools/nexus_inspect/src/correlations.rs — Onglet Corrélations (doc §5.8)
//
// Doc §4.2 nomme 4 vues : corr_sommeil_cognition, corr_medication_etat,
// weekly_load, med_observance. Les 4 sont ICI — DUPLIQUÉES depuis
// modules/dashboard/src/correlations.rs (logique identique, déjà prouvée),
// pas importées : nexus_inspect est un outil pair, pas un module, mais le
// principe est le même (§7.1) — pas de dépendance croisée vers une crate de
// module (dashboard::correlations est d'ailleurs un module PRIVÉ, non
// exposé hors de sa crate : l'import serait de toute façon impossible).
//
// corr_sommeil_cognition/corr_medication_etat étaient absentes jusqu'à une
// relecture complète doc-vs-code : aucune des deux n'était en fait hors de
// portée (JOIN + r² en forme close pour la première, JOIN identique à
// med_observance pour la seconde). Ici, contrairement au dashboard, pas de
// scatter peint — l'onglet liste les points bruts en texte (doc §5.8 :
// « exécution des vues SQL nommées », no black box), la visualisation
// vivant côté dashboard.
// ============================================================================

use chrono::{Datelike, NaiveDate, NaiveDateTime};

pub type WeekKey = (i32, u32);

fn week_key(d: NaiveDate) -> WeekKey {
    let w = d.iso_week();
    (w.year(), w.week())
}

pub fn week_label(w: WeekKey) -> String {
    format!("{}-W{:02}", w.0, w.1)
}

fn parse_datetime(raw: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S").ok()
}

fn parse_date_or_datetime(raw: &str) -> Option<NaiveDate> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt.date());
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()
}

/// Charge hebdomadaire : tâches créées, terminées, épuisement moyen (doc
/// §4.2, vue `weekly_load`).
#[derive(Debug, Clone, PartialEq)]
pub struct WeeklyLoad {
    pub week: WeekKey,
    pub created: u32,
    pub done: u32,
    pub avg_epuisement: Option<f64>,
}

pub fn weekly_load(
    tasks: &[nexus_db::Task],
    history: &[nexus_db::TaskHistory],
    mood: &[nexus_db::MoodLog],
) -> Vec<WeeklyLoad> {
    let mut created: std::collections::BTreeMap<WeekKey, u32> = std::collections::BTreeMap::new();
    for t in tasks {
        if let Some(d) = parse_date_or_datetime(&t.created_at) {
            *created.entry(week_key(d)).or_default() += 1;
        }
    }
    let mut done: std::collections::BTreeMap<WeekKey, u32> = std::collections::BTreeMap::new();
    for h in history {
        if h.nouveau_statut != "done" {
            continue;
        }
        if let Some(d) = parse_date_or_datetime(&h.changed_at) {
            *done.entry(week_key(d)).or_default() += 1;
        }
    }
    let mut epuisement_sum: std::collections::BTreeMap<WeekKey, (f64, u32)> =
        std::collections::BTreeMap::new();
    for m in mood {
        if let Some(d) = parse_date_or_datetime(&m.logged_at) {
            let entry = epuisement_sum.entry(week_key(d)).or_default();
            entry.0 += m.epuisement as f64;
            entry.1 += 1;
        }
    }

    let mut weeks: std::collections::BTreeSet<WeekKey> = std::collections::BTreeSet::new();
    weeks.extend(created.keys());
    weeks.extend(done.keys());
    weeks.extend(epuisement_sum.keys());

    weeks
        .into_iter()
        .map(|w| WeeklyLoad {
            week: w,
            created: created.get(&w).copied().unwrap_or(0),
            done: done.get(&w).copied().unwrap_or(0),
            avg_epuisement: epuisement_sum.get(&w).map(|(sum, n)| sum / f64::from(*n)),
        })
        .collect()
}

/// Observance médication : prises réelles par médicament, groupées par
/// semaine, appariées au fonctionnement moyen (doc §4.2, vue
/// `med_observance` — moitié observable, `medications` n'a pas de champ de
/// fréquence attendue, voir dashboard/README_MODULE.md pour le détail de
/// cette décision, identique ici).
#[derive(Debug, Clone, PartialEq)]
pub struct MedObservanceWeek {
    pub week: WeekKey,
    pub doses_by_med: Vec<(String, u32)>,
    pub avg_fonctionnement: Option<f64>,
}

pub fn med_observance(
    doses: &[nexus_db::MedDose],
    medications: &[nexus_db::Medication],
    mood: &[nexus_db::MoodLog],
) -> Vec<MedObservanceWeek> {
    let mut fonctionnement_sum: std::collections::BTreeMap<WeekKey, (f64, u32)> =
        std::collections::BTreeMap::new();
    for m in mood {
        if let Some(d) = parse_date_or_datetime(&m.logged_at) {
            let entry = fonctionnement_sum.entry(week_key(d)).or_default();
            entry.0 += m.fonctionnement as f64;
            entry.1 += 1;
        }
    }
    let mut counts: std::collections::BTreeMap<WeekKey, std::collections::BTreeMap<String, u32>> =
        std::collections::BTreeMap::new();
    for d in doses {
        let Some(date) = parse_date_or_datetime(&d.taken_at) else {
            continue;
        };
        let med_nom = medications
            .iter()
            .find(|m| m.id == d.med_id)
            .map(|m| m.nom.clone())
            .unwrap_or_else(|| d.med_id.clone());
        *counts
            .entry(week_key(date))
            .or_default()
            .entry(med_nom)
            .or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(week, by_med)| MedObservanceWeek {
            week,
            doses_by_med: by_med.into_iter().collect(),
            avg_fonctionnement: fonctionnement_sum
                .get(&week)
                .map(|(sum, n)| sum / f64::from(*n)),
        })
        .collect()
}

/// Tâches bloquées depuis plus de `days` jours — même logique que
/// dashboard, réutilisée ici pour l'onglet Tâches (doc §5.8).
pub fn blocked_over(
    tasks: &[nexus_db::Task],
    now: NaiveDateTime,
    days: i64,
) -> Vec<&nexus_db::Task> {
    let cutoff = now - chrono::Duration::days(days);
    tasks
        .iter()
        .filter(|t| t.statut == "blocked")
        .filter(|t| parse_datetime(&t.updated_at).is_some_and(|d| d < cutoff))
        .collect()
}

/// Sommeil J-1 → cognitif J (doc §4.2, vue `corr_sommeil_cognition`) — même
/// logique que dashboard/src/correlations.rs.
#[derive(Debug, Clone, PartialEq)]
pub struct SleepCognitionPoint {
    pub duration_h: f64,
    pub quality: i64,
    pub cognitif: i64,
    pub epuisement: i64,
}

pub fn corr_sommeil_cognition(
    sleep: &[nexus_db::SleepLog],
    mood: &[nexus_db::MoodLog],
) -> Vec<SleepCognitionPoint> {
    let mut points = Vec::new();
    for s in sleep {
        let Ok(sleep_date) = NaiveDate::parse_from_str(&s.date, "%Y-%m-%d") else {
            continue;
        };
        for m in mood {
            if parse_date_or_datetime(&m.logged_at) == Some(sleep_date) {
                points.push(SleepCognitionPoint {
                    duration_h: s.duration_h,
                    quality: s.quality,
                    cognitif: m.cognitif,
                    epuisement: m.epuisement,
                });
            }
        }
    }
    points
}

/// r² d'une régression linéaire simple (doc §4.2/§5.6) — même logique que
/// dashboard/src/correlations.rs.
pub fn r_squared(points: &[(f64, f64)]) -> Option<f64> {
    let n = points.len();
    if n < 2 {
        return None;
    }
    let n_f = n as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n_f;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n_f;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (x, y) in points {
        let dx = x - mean_x;
        let dy = y - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    if var_x <= f64::EPSILON || var_y <= f64::EPSILON {
        return None;
    }
    let r = cov / (var_x.sqrt() * var_y.sqrt());
    Some(r * r)
}

/// Courbe empirique médication (doc §4.2, vue `corr_medication_etat`) —
/// même logique que dashboard/src/correlations.rs. Points bruts uniquement
/// (pas de lissage LOESS ici non plus, même écart documenté).
#[derive(Debug, Clone, PartialEq)]
pub struct MedEtatPoint {
    pub delta_minutes: i64,
    pub cognitif: i64,
    pub fonctionnement: i64,
}

pub fn corr_medication_etat(
    doses: &[nexus_db::MedDose],
    mood: &[nexus_db::MoodLog],
) -> Vec<MedEtatPoint> {
    let mut points = Vec::new();
    for d in doses {
        let Some(taken_at) = parse_datetime(&d.taken_at) else {
            continue;
        };
        for m in mood {
            let Some(logged_at) = parse_datetime(&m.logged_at) else {
                continue;
            };
            let delta = logged_at.signed_duration_since(taken_at);
            if delta >= chrono::Duration::zero() && delta <= chrono::Duration::hours(12) {
                points.push(MedEtatPoint {
                    delta_minutes: delta.num_minutes(),
                    cognitif: m.cognitif,
                    fonctionnement: m.fonctionnement,
                });
            }
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(created_at: &str, statut: &str, updated_at: &str) -> nexus_db::Task {
        nexus_db::Task {
            id: format!("{created_at}-{statut}"),
            parent_id: None,
            titre: "t".into(),
            statut: statut.into(),
            priorite: None,
            energie: None,
            contexte: None,
            duree_min: None,
            echeance: None,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
            notes: None,
        }
    }

    fn history(changed_at: &str, nouveau_statut: &str) -> nexus_db::TaskHistory {
        nexus_db::TaskHistory {
            id: changed_at.to_string(),
            task_id: "t1".into(),
            ancien_statut: "doing".into(),
            nouveau_statut: nouveau_statut.into(),
            changed_at: changed_at.into(),
        }
    }

    fn mood(logged_at: &str, epuisement: i64, fonctionnement: i64) -> nexus_db::MoodLog {
        nexus_db::MoodLog {
            id: logged_at.to_string(),
            logged_at: logged_at.to_string(),
            epuisement,
            cognitif: 3,
            sensoriel: 3,
            masquage: 3,
            fonctionnement,
            notes: None,
        }
    }

    fn mood_full(
        logged_at: &str,
        epuisement: i64,
        cognitif: i64,
        fonctionnement: i64,
    ) -> nexus_db::MoodLog {
        nexus_db::MoodLog {
            id: logged_at.to_string(),
            logged_at: logged_at.to_string(),
            epuisement,
            cognitif,
            sensoriel: 3,
            masquage: 3,
            fonctionnement,
            notes: None,
        }
    }

    fn sleep_log(date: &str, duration_h: f64, quality: i64) -> nexus_db::SleepLog {
        nexus_db::SleepLog {
            id: date.to_string(),
            date: date.to_string(),
            duration_h,
            quality,
            notes: None,
        }
    }

    #[test]
    fn weekly_load_compte_cree_et_termine() {
        let tasks = vec![task("2026-07-06T09:00:00", "done", "2026-07-06T09:00:00")];
        let history = vec![history("2026-07-08T09:00:00", "done")];
        let mood = vec![mood("2026-07-07T09:00:00", 4, 3)];
        let load = weekly_load(&tasks, &history, &mood);
        assert_eq!(load.len(), 1);
        assert_eq!(load[0].created, 1);
        assert_eq!(load[0].done, 1);
        assert_eq!(load[0].avg_epuisement, Some(4.0));
    }

    #[test]
    fn med_observance_groupe_plusieurs_medicaments_dans_la_meme_semaine() {
        let meds = vec![
            nexus_db::Medication {
                id: "m1".into(),
                nom: "A".into(),
                molecule: "a".into(),
                dose_default: 10.0,
                notes: None,
            },
            nexus_db::Medication {
                id: "m2".into(),
                nom: "B".into(),
                molecule: "b".into(),
                dose_default: 5.0,
                notes: None,
            },
        ];
        let doses = vec![
            nexus_db::MedDose {
                id: "d1".into(),
                med_id: "m1".into(),
                taken_at: "2026-07-06T09:00:00".into(),
                dose_mg: 10.0,
                ressenti: None,
                notes: None,
            },
            nexus_db::MedDose {
                id: "d2".into(),
                med_id: "m2".into(),
                taken_at: "2026-07-07T09:00:00".into(),
                dose_mg: 5.0,
                ressenti: None,
                notes: None,
            },
        ];
        let obs = med_observance(&doses, &meds, &[]);
        assert_eq!(obs.len(), 1);
        assert_eq!(
            obs[0].doses_by_med,
            vec![("A".to_string(), 1), ("B".to_string(), 1)]
        );
    }

    #[test]
    fn blocked_over_filtre_par_anciennete() {
        let tasks = vec![
            task("2026-06-01T00:00:00", "blocked", "2026-06-01T00:00:00"),
            task("2026-07-11T00:00:00", "blocked", "2026-07-11T00:00:00"),
        ];
        let now = NaiveDateTime::parse_from_str("2026-07-12T12:00:00", "%Y-%m-%dT%H:%M:%S")
            .expect("datetime");
        let blocked = blocked_over(&tasks, now, 7);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].created_at, "2026-06-01T00:00:00");
    }

    #[test]
    fn week_label_forme_attendue() {
        assert_eq!(week_label((2026, 5)), "2026-W05");
    }

    #[test]
    fn corr_sommeil_cognition_apparie_sur_la_meme_date() {
        let sleep = vec![sleep_log("2026-07-12", 7.5, 4)];
        let mood = vec![
            mood_full("2026-07-12T09:00:00", 2, 3, 3),
            mood_full("2026-07-13T09:00:00", 5, 1, 5),
        ];
        let points = corr_sommeil_cognition(&sleep, &mood);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].cognitif, 3);
    }

    #[test]
    fn r_squared_correlation_parfaite() {
        let points = vec![(1.0, 2.0), (2.0, 4.0), (3.0, 6.0)];
        let r2 = r_squared(&points).expect("calculable");
        assert!((r2 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn corr_medication_etat_filtre_la_fenetre_12h() {
        let doses = vec![nexus_db::MedDose {
            id: "d1".into(),
            med_id: "m1".into(),
            taken_at: "2026-07-12T08:00:00".into(),
            dose_mg: 20.0,
            ressenti: None,
            notes: None,
        }];
        let mood = vec![
            mood_full("2026-07-12T09:30:00", 2, 4, 4),
            mood_full("2026-07-12T21:00:00", 3, 1, 3),
        ];
        let points = corr_medication_etat(&doses, &mood);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].delta_minutes, 90);
    }
}
