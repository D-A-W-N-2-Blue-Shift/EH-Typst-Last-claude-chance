// ============================================================================
// modules/todo/src/logic.rs — Logique pure du kanban (doc §5.4)
//
// Rien ici ne touche SQLite ni egui : tout est testable en isolation.
// ============================================================================

use chrono::NaiveDate;

/// Les 6 statuts du kanban (doc §5.4) — `backlog → today → doing → done`,
/// avec `blocked`/`dropped` comme branches distinctes de `doing` (obstacle
/// externe vs décision active d'abandon — jamais un "oubli silencieux").
pub const COLUMNS: &[&str] = &["backlog", "today", "doing", "blocked", "done", "dropped"];

pub fn column_label(statut: &str) -> &'static str {
    match statut {
        "backlog" => "Backlog",
        "today" => "Aujourd'hui",
        "doing" => "En cours",
        "blocked" => "Bloqué",
        "done" => "Terminé",
        "dropped" => "Abandonné",
        _ => "?",
    }
}

/// Énergies autorisées par le filtre « maintenant » (doc §5.4), en fonction
/// du score `cognitif` (mood_log) le plus récent.
///
/// Hypothèse signalée (§4.4 — le doc ne donne qu'UN point exact : « si
/// cognitif = 2, affiche uniquement les tâches energie: low », sans préciser
/// la courbe complète 1-5 ni le sens de l'échelle). Lecture retenue,
/// cohérente avec la convention déjà utilisée dans health::mood (1 = jour
/// favorable, 5 = jour difficile, même sens que « épuisement ») : lecture
/// conservatrice, adaptée à un profil en burnout sévère (doc §1) — seul le
/// meilleur score possible (1) débloque medium/high, tout le reste (2 à 5)
/// reste sur low. Politique isolée dans cette fonction pure, triviale à
/// corriger si l'intention réelle diffère (§A4 réversibilité).
pub fn allowed_energy_levels(cognitif: i64) -> &'static [&'static str] {
    if cognitif <= 1 {
        &["low", "medium", "high"]
    } else {
        &["low"]
    }
}

/// Une tâche sans durée estimée est signalée (doc §5.4 : « une tâche sans
/// durée est une tâche impossible à initier pour un profil AuDHD »).
pub fn duration_missing(duree_min: Option<i64>) -> bool {
    duree_min.is_none()
}

/// Le contexte de la tâche correspond-il au filtre choisi ? `None` (pas de
/// filtre) ou contexte de tâche `"n'importe"` correspondent toujours.
pub fn matches_context(task_contexte: Option<&str>, filter: Option<&str>) -> bool {
    let Some(filter) = filter else { return true };
    match task_contexte {
        Some(c) => c == filter || c == "n'importe",
        None => true,
    }
}

/// Prochaine occurrence d'une règle de récurrence (doc §5.4 : « quotidien /
/// hebdo / mensuel / règle custom »). `None` pour une règle custom non
/// reconnue — pas de régénération automatique plutôt qu'une interprétation
/// inventée (§A2).
pub fn next_occurrence(rule: &str, from: NaiveDate) -> Option<NaiveDate> {
    match rule {
        "quotidien" => Some(from + chrono::Duration::days(1)),
        "hebdo" => Some(from + chrono::Duration::days(7)),
        "mensuel" => from.checked_add_months(chrono::Months::new(1)),
        _ => None,
    }
}

/// Sépare les tâches de premier niveau des sous-tâches, groupées par
/// `parent_id` (doc §5.4 : un seul niveau de sous-tâches). Prend des
/// références (`&[&Task]`) : côté appelant, la liste visible est déjà
/// filtrée par emprunt (contexte/énergie), pas la peine de cloner.
pub fn group_by_parent<'a>(
    tasks: &[&'a nexus_db::Task],
) -> (
    Vec<&'a nexus_db::Task>,
    std::collections::HashMap<String, Vec<&'a nexus_db::Task>>,
) {
    let mut top = Vec::new();
    let mut children: std::collections::HashMap<String, Vec<&'a nexus_db::Task>> =
        std::collections::HashMap::new();
    for t in tasks {
        match &t.parent_id {
            Some(pid) => children.entry(pid.clone()).or_default().push(t),
            None => top.push(*t),
        }
    }
    (top, children)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, parent_id: Option<&str>) -> nexus_db::Task {
        nexus_db::Task {
            id: id.to_string(),
            parent_id: parent_id.map(|s| s.to_string()),
            titre: id.to_string(),
            statut: "backlog".into(),
            priorite: None,
            energie: None,
            contexte: None,
            duree_min: None,
            echeance: None,
            created_at: "2026-07-12T00:00:00".into(),
            updated_at: "2026-07-12T00:00:00".into(),
            notes: None,
        }
    }

    #[test]
    fn allowed_energy_levels_exemple_du_doc() {
        // Le SEUL point donné explicitement par le doc.
        assert_eq!(allowed_energy_levels(2), &["low"]);
    }

    #[test]
    fn allowed_energy_levels_meilleur_score_debloque_tout() {
        assert_eq!(allowed_energy_levels(1), &["low", "medium", "high"]);
    }

    #[test]
    fn allowed_energy_levels_pire_score_reste_low() {
        assert_eq!(allowed_energy_levels(5), &["low"]);
    }

    #[test]
    fn duration_missing_detecte_absence() {
        assert!(duration_missing(None));
        assert!(!duration_missing(Some(30)));
    }

    #[test]
    fn matches_context_sans_filtre_tout_passe() {
        assert!(matches_context(Some("maison"), None));
        assert!(matches_context(None, None));
    }

    #[test]
    fn matches_context_nimporte_toujours_vrai() {
        assert!(matches_context(Some("n'importe"), Some("dehors")));
    }

    #[test]
    fn matches_context_filtre_exact() {
        assert!(matches_context(Some("dehors"), Some("dehors")));
        assert!(!matches_context(Some("maison"), Some("dehors")));
    }

    #[test]
    fn matches_context_tache_sans_contexte_passe_toujours() {
        assert!(matches_context(None, Some("dehors")));
    }

    #[test]
    fn next_occurrence_quotidien() {
        let d = NaiveDate::parse_from_str("2026-07-12", "%Y-%m-%d").expect("date");
        assert_eq!(
            next_occurrence("quotidien", d),
            Some(NaiveDate::parse_from_str("2026-07-13", "%Y-%m-%d").expect("date"))
        );
    }

    #[test]
    fn next_occurrence_hebdo() {
        let d = NaiveDate::parse_from_str("2026-07-12", "%Y-%m-%d").expect("date");
        assert_eq!(
            next_occurrence("hebdo", d),
            Some(NaiveDate::parse_from_str("2026-07-19", "%Y-%m-%d").expect("date"))
        );
    }

    #[test]
    fn next_occurrence_mensuel() {
        let d = NaiveDate::parse_from_str("2026-07-12", "%Y-%m-%d").expect("date");
        assert_eq!(
            next_occurrence("mensuel", d),
            Some(NaiveDate::parse_from_str("2026-08-12", "%Y-%m-%d").expect("date"))
        );
    }

    #[test]
    fn next_occurrence_regle_inconnue_pas_de_regen() {
        let d = NaiveDate::parse_from_str("2026-07-12", "%Y-%m-%d").expect("date");
        assert_eq!(next_occurrence("tous les mardis pairs", d), None);
    }

    #[test]
    fn group_by_parent_separe_correctement() {
        let tasks = [
            task("p1", None),
            task("p2", None),
            task("c1", Some("p1")),
            task("c2", Some("p1")),
        ];
        let refs: Vec<&nexus_db::Task> = tasks.iter().collect();
        let (top, children) = group_by_parent(&refs);
        assert_eq!(top.len(), 2);
        assert_eq!(children.get("p1").map(|v| v.len()), Some(2));
        assert!(!children.contains_key("p2"));
    }
}
