// ============================================================================
// modules/dashboard/src/correlations.rs — Corrélations médication + tâches
// (doc §5.6, périmètre exact de la session 7 du §10 : "corrélations
// médication + tâches" — la corrélation sommeil→cognitif listée dans la
// même sous-section du doc §5.6 n'est PAS dans ce périmètre littéral,
// différée explicitement, voir README_MODULE.md).
//
// Écart documenté (§A2) : la vue `med_observance` du doc (§4.2) suppose un
// "taux de prise effectif vs fréquence attendue", mais `medications` (doc
// §8, schéma déjà construit à l'incrément 1) n'a AUCUN champ de fréquence
// attendue. Je ne l'invente pas : je calcule ce qui EST dérivable du schéma
// réel (nombre de prises effectives par semaine), apparié au fonctionnement
// hebdomadaire moyen — la moitié observable de la vue documentée, pas
// l'intégralité qu'un champ manquant rend impossible à produire.
// ============================================================================

use chrono::{Datelike, NaiveDate, NaiveDateTime};

/// Clé de semaine ISO (année, numéro de semaine) — triable, groupable,
/// insensible aux limites de mois/année civile.
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

/// Charge hebdomadaire : tâches créées, tâches terminées (transitions vers
/// `done` dans `task_history`), épuisement moyen — une entrée par semaine
/// où AU MOINS un de ces trois signaux existe. Trié par semaine croissante.
/// Doc §5.6 : « Charge tâches → épuisement (créées vs terminées vs score
/// épuisement) », vue `weekly_load` (§4.2).
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

/// Vélocité hebdomadaire : tâches terminées par semaine, les
/// `max_weeks` dernières semaines (doc §5.6 : « sur 8 semaines »).
pub fn velocity(load: &[WeeklyLoad], max_weeks: usize) -> Vec<WeeklyLoad> {
    let n = load.len();
    if n <= max_weeks {
        load.to_vec()
    } else {
        load[n - max_weeks..].to_vec()
    }
}

/// Répartition par énergie des tâches TERMINÉES (doc §5.6 : « est-ce qu'on
/// ne finit que les low ? »). `(low, medium, high, sans_energie)`.
pub fn done_by_energy(tasks: &[nexus_db::Task]) -> (u32, u32, u32, u32) {
    let mut low = 0;
    let mut medium = 0;
    let mut high = 0;
    let mut none = 0;
    for t in tasks {
        if t.statut != "done" {
            continue;
        }
        match t.energie.as_deref() {
            Some("low") => low += 1,
            Some("medium") => medium += 1,
            Some("high") => high += 1,
            _ => none += 1,
        }
    }
    (low, medium, high, none)
}

/// Tâches bloquées depuis plus de `days` jours (doc §5.6 : « liste
/// explicite »). Compare `updated_at` (dernière transition de statut, doc
/// §5.4) à `now - days`.
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

/// Observance médication (moitié observable, voir écart documenté en tête
/// de fichier) : nombre de prises réelles PAR MÉDICAMENT, groupées par
/// semaine (une entrée = une semaine, `doses_by_med` = une barre par
/// médicament dans le groupe — doc §5.6 : « barres groupées »). Le
/// fonctionnement moyen est UNE valeur par semaine, jamais mélangé entre
/// médicaments différents.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn task(
        created_at: &str,
        statut: &str,
        energie: Option<&str>,
        updated_at: &str,
    ) -> nexus_db::Task {
        nexus_db::Task {
            id: format!("{created_at}-{statut}"),
            parent_id: None,
            titre: "t".into(),
            statut: statut.into(),
            priorite: None,
            energie: energie.map(|s| s.to_string()),
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
    fn week_key_meme_semaine_pour_dates_proches() {
        let d1 = NaiveDate::parse_from_str("2026-07-06", "%Y-%m-%d").expect("date"); // lundi
        let d2 = NaiveDate::parse_from_str("2026-07-12", "%Y-%m-%d").expect("date"); // dimanche
        assert_eq!(week_key(d1), week_key(d2));
    }

    #[test]
    fn weekly_load_compte_cree_et_termine() {
        let tasks = vec![task(
            "2026-07-06T09:00:00",
            "done",
            Some("low"),
            "2026-07-06T09:00:00",
        )];
        let history = vec![history("2026-07-08T09:00:00", "done")];
        let mood = vec![mood("2026-07-07T09:00:00", 4, 3)];
        let load = weekly_load(&tasks, &history, &mood);
        assert_eq!(load.len(), 1);
        assert_eq!(load[0].created, 1);
        assert_eq!(load[0].done, 1);
        assert_eq!(load[0].avg_epuisement, Some(4.0));
    }

    #[test]
    fn weekly_load_ignore_les_transitions_non_done() {
        let history = vec![history("2026-07-08T09:00:00", "doing")];
        let load = weekly_load(&[], &history, &[]);
        assert!(load.is_empty());
    }

    #[test]
    fn velocity_tronque_aux_dernieres_semaines() {
        let load: Vec<WeeklyLoad> = (1..=10)
            .map(|i| WeeklyLoad {
                week: (2026, i),
                created: 0,
                done: i,
                avg_epuisement: None,
            })
            .collect();
        let v = velocity(&load, 8);
        assert_eq!(v.len(), 8);
        assert_eq!(v[0].week, (2026, 3));
        assert_eq!(v[7].week, (2026, 10));
    }

    #[test]
    fn done_by_energy_compte_correctement() {
        let tasks = vec![
            task(
                "2026-07-01T00:00:00",
                "done",
                Some("low"),
                "2026-07-01T00:00:00",
            ),
            task(
                "2026-07-02T00:00:00",
                "done",
                Some("low"),
                "2026-07-02T00:00:00",
            ),
            task(
                "2026-07-03T00:00:00",
                "done",
                Some("high"),
                "2026-07-03T00:00:00",
            ),
            task(
                "2026-07-04T00:00:00",
                "backlog",
                Some("low"),
                "2026-07-04T00:00:00",
            ), // pas done
            task("2026-07-05T00:00:00", "done", None, "2026-07-05T00:00:00"),
        ];
        assert_eq!(done_by_energy(&tasks), (2, 0, 1, 1));
    }

    #[test]
    fn blocked_over_filtre_par_anciennete() {
        let tasks = vec![
            task(
                "2026-06-01T00:00:00",
                "blocked",
                None,
                "2026-06-01T00:00:00",
            ), // vieux : bloqué
            task(
                "2026-07-11T00:00:00",
                "blocked",
                None,
                "2026-07-11T00:00:00",
            ), // récent : pas signalé
            task("2026-06-01T00:00:00", "done", None, "2026-06-01T00:00:00"), // pas bloqué
        ];
        let now = NaiveDateTime::parse_from_str("2026-07-12T12:00:00", "%Y-%m-%dT%H:%M:%S")
            .expect("datetime");
        let blocked = blocked_over(&tasks, now, 7);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].created_at, "2026-06-01T00:00:00");
    }

    #[test]
    fn med_observance_compte_les_prises_reelles_par_semaine() {
        let med = nexus_db::Medication {
            id: "m1".into(),
            nom: "Méthylphénidate".into(),
            molecule: "mph".into(),
            dose_default: 20.0,
            notes: None,
        };
        let doses = vec![
            nexus_db::MedDose {
                id: "d1".into(),
                med_id: "m1".into(),
                taken_at: "2026-07-06T09:00:00".into(),
                dose_mg: 20.0,
                ressenti: None,
                notes: None,
            },
            nexus_db::MedDose {
                id: "d2".into(),
                med_id: "m1".into(),
                taken_at: "2026-07-07T09:00:00".into(),
                dose_mg: 20.0,
                ressenti: None,
                notes: None,
            },
        ];
        let mood = vec![mood("2026-07-06T09:00:00", 3, 4)];
        let obs = med_observance(&doses, &[med], &mood);
        assert_eq!(obs.len(), 1, "une seule semaine concernée");
        assert_eq!(
            obs[0].doses_by_med,
            vec![("Méthylphénidate".to_string(), 2)]
        );
        assert_eq!(obs[0].avg_fonctionnement, Some(4.0));
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
        assert_eq!(obs.len(), 1, "même semaine ISO : un seul groupe");
        assert_eq!(
            obs[0].doses_by_med,
            vec![("A".to_string(), 1), ("B".to_string(), 1)],
            "les deux médicaments doivent apparaître séparément, pas fusionnés"
        );
    }
}
