// ============================================================================
// modules/dashboard/src/correlations.rs — Corrélations (doc §5.6/§4.2)
//
// Les 4 vues nommées par le doc §4.2 sont ICI TOUTES les 4 : weekly_load,
// med_observance, corr_sommeil_cognition, corr_medication_etat. Les deux
// dernières étaient absentes jusqu'à une relecture complète doc-vs-code —
// ni l'une ni l'autre n'était en fait hors de portée : `corr_sommeil_
// cognition` est un JOIN + un r² en forme close (rien d'inventé), et
// `corr_medication_etat` est un JOIN identique à `med_observance` (déjà
// bâti) — seule sa « courbe de tendance locale (LOESS si N>20) » reste NON
// implémentée (voir la note sur `MedEtatPoint`/`corr_medication_etat` :
// les points bruts, que le doc lui-même prescrit comme repli sous N=20,
// sont TOUJOURS affichés ; c'est la LISSE LOESS elle-même, algorithme
// itératif non trivial sans précédent dans ce dépôt, qui reste un écart
// honnête plutôt qu'une approximation inventée, §A2).
//
// Écart documenté restant (§A2) : la vue `med_observance` du doc (§4.2)
// suppose un "taux de prise effectif vs fréquence attendue", mais
// `medications` (doc §8, schéma déjà construit à l'incrément 1) n'a AUCUN
// champ de fréquence attendue. Je ne l'invente pas : je calcule ce qui EST
// dérivable du schéma réel (nombre de prises effectives par semaine),
// apparié au fonctionnement hebdomadaire moyen — la moitié observable de la
// vue documentée, pas l'intégralité qu'un champ manquant rend impossible à
// produire.
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

/// Un point de sommeil apparié à une saisie d'état psy (doc §5.6/§4.2, vue
/// `corr_sommeil_cognition`).
#[derive(Debug, Clone, PartialEq)]
pub struct SleepCognitionPoint {
    pub duration_h: f64,
    pub quality: i64,
    pub cognitif: i64,
    pub epuisement: i64,
}

/// Sommeil J-1 → cognitif J (doc §5.6, vue `corr_sommeil_cognition` §4.2).
/// `sleep_log.date` est déjà daté au réveil (doc §5.3 : « saisie idéalement
/// au réveil »), donc la nuit "J-1→J" porte la date J — le rapprochement se
/// fait sur la MÊME date, cohérent avec le JOIN "sur date" du §4.2 (pas un
/// décalage +1 explicite à inventer). Une nuit peut apparaître plusieurs
/// fois si plusieurs saisies d'état psy ont lieu le même jour — chacune est
/// un point d'observation distinct.
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

/// Coefficient de détermination r² d'une régression linéaire simple (doc
/// §5.6 : « scatter, r² affiché »). `None` si non calculable — moins de 2
/// points, ou variance nulle sur x ou y (une droite non définie n'a pas de
/// r², mieux vaut l'absence qu'une valeur inventée, §A2).
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

/// Un point de la courbe empirique médication (doc §5.6/§4.2, vue
/// `corr_medication_etat`).
#[derive(Debug, Clone, PartialEq)]
pub struct MedEtatPoint {
    pub delta_minutes: i64,
    pub cognitif: i64,
    pub fonctionnement: i64,
}

/// Courbe empirique médication (doc §5.6, vue `corr_medication_etat` §4.2) :
/// chaque paire (prise, saisie d'état psy DANS les 12h suivantes) devient un
/// point brut. Le doc prescrit lui-même les points bruts comme forme de
/// repli sous N=20 (« sinon points bruts uniquement ») — LA COURBE DE
/// TENDANCE LOCALE (LOESS) ELLE-MÊME N'EST PAS CALCULÉE ICI : algorithme de
/// lissage itératif sans aucun précédent dans ce dépôt, ajouter une
/// approximation maison non vérifiée serait moins honnête qu'afficher les
/// points bruts seuls (§A2/§A3). Une prise peut apparaître plusieurs fois si
/// plusieurs saisies suivent dans la fenêtre — chacune reste un point
/// distinct, aucune n'est privilégiée arbitrairement.
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

    /// Comme `mood`, mais avec `cognitif` contrôlable — nécessaire aux tests
    /// de `corr_sommeil_cognition`/`corr_medication_etat`, qui portent
    /// spécifiquement sur ce champ (`mood` le fixe à 3 pour les tests plus
    /// anciens qui ne s'y intéressaient pas).
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
    fn corr_sommeil_cognition_apparie_sur_la_meme_date() {
        let sleep = vec![sleep_log("2026-07-12", 7.5, 4)];
        let mood = vec![
            mood_full("2026-07-12T09:00:00", 2, 3, 3), // même date : apparié
            mood_full("2026-07-13T09:00:00", 5, 1, 5), // date différente : ignoré
        ];
        let points = corr_sommeil_cognition(&sleep, &mood);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].duration_h, 7.5);
        assert_eq!(points[0].quality, 4);
        assert_eq!(points[0].cognitif, 3);
        assert_eq!(points[0].epuisement, 2);
    }

    #[test]
    fn corr_sommeil_cognition_une_nuit_peut_apparier_plusieurs_saisies() {
        let sleep = vec![sleep_log("2026-07-12", 7.0, 3)];
        let mood = vec![
            mood_full("2026-07-12T09:00:00", 2, 3, 3),
            mood_full("2026-07-12T20:00:00", 2, 4, 4),
        ];
        assert_eq!(corr_sommeil_cognition(&sleep, &mood).len(), 2);
    }

    #[test]
    fn corr_sommeil_cognition_vide_sans_correspondance() {
        let sleep = vec![sleep_log("2026-07-12", 7.0, 3)];
        let mood = vec![mood_full("2026-08-01T09:00:00", 2, 3, 3)];
        assert!(corr_sommeil_cognition(&sleep, &mood).is_empty());
    }

    #[test]
    fn r_squared_correlation_parfaite() {
        // y = 2x exactement : corrélation parfaite, r² = 1.
        let points = vec![(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0)];
        let r2 = r_squared(&points).expect("calculable");
        assert!((r2 - 1.0).abs() < 1e-9, "r²={r2}, attendu ~1.0");
    }

    #[test]
    fn r_squared_aucune_variance_x_est_none() {
        let points = vec![(5.0, 1.0), (5.0, 2.0), (5.0, 3.0)];
        assert_eq!(r_squared(&points), None);
    }

    #[test]
    fn r_squared_moins_de_deux_points_est_none() {
        assert_eq!(r_squared(&[]), None);
        assert_eq!(r_squared(&[(1.0, 1.0)]), None);
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
            mood_full("2026-07-12T09:30:00", 2, 4, 4), // +1h30 : dans la fenêtre
            mood_full("2026-07-12T21:00:00", 3, 1, 3), // +13h : hors fenêtre
            mood_full("2026-07-12T07:00:00", 1, 5, 2), // avant la prise : hors fenêtre
        ];
        let points = corr_medication_etat(&doses, &mood);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].delta_minutes, 90);
        assert_eq!(points[0].cognitif, 4);
        assert_eq!(points[0].fonctionnement, 4);
    }

    #[test]
    fn corr_medication_etat_borne_incluse_a_12h_pile() {
        let doses = vec![nexus_db::MedDose {
            id: "d1".into(),
            med_id: "m1".into(),
            taken_at: "2026-07-12T08:00:00".into(),
            dose_mg: 20.0,
            ressenti: None,
            notes: None,
        }];
        let mood = vec![mood_full("2026-07-12T20:00:00", 3, 3, 3)];
        assert_eq!(corr_medication_etat(&doses, &mood).len(), 1);
    }
}
