// ============================================================================
// tools/nexus_inspect/src/correlations.rs — Onglet Corrélations (doc §5.8)
//
// Doc §4.2 nomme 4 vues : corr_sommeil_cognition, corr_medication_etat,
// weekly_load, med_observance. Seules les 2 dernières sont implémentées ici
// — DUPLIQUÉES depuis modules/dashboard/src/correlations.rs (logique
// identique, déjà prouvée à l'incrément 7), pas importées : nexus_inspect
// est un outil pair, pas un module, mais le principe est le même (§7.1) —
// pas de dépendance croisée vers une crate de module (dashboard::
// correlations est d'ailleurs un module PRIVÉ, non exposé hors de sa
// crate : l'import serait de toute façon impossible).
//
// corr_sommeil_cognition et corr_medication_etat : PAS implémentées ici.
// Aucune des deux n'a de précédent Rust éprouvé ailleurs dans ce dépôt
// (dashboard les a explicitement exclues de son propre périmètre à
// l'incrément 7 pour la même raison — titre littéral de session). Les
// inventer sous pression de temps pour cet incrément serait risquer une
// logique de corrélation non vérifiée. Les données brutes dont elles
// auraient besoin (sleep_log, mood_log, med_doses) restent lisibles sans
// SQL dans l'onglet Santé — la règle "no black box" du doc §5.8 tient donc
// toujours, juste sans la vue dérivée pour ces deux corrélations.
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
}
