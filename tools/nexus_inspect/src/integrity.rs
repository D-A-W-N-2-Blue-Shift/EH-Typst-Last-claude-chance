// ============================================================================
// tools/nexus_inspect/src/integrity.rs — Onglet Intégrité (doc §5.8)
//
// « clés orphelines, dates incohérentes, données manquantes ». Signale
// uniquement — ne corrige jamais rien (outil de lecture seule, doc §5.8).
// Les clés étrangères sont normalement appliquées par SQLite dès l'écriture
// (PRAGMA foreign_keys = ON, nexus_db::open_db) : ces contrôles sont un
// filet de sécurité pour l'audit d'une base modifiée hors de l'app (édition
// manuelle, ancien schéma, etc.) — pas une redite de ce que SQLite garantit
// déjà côté écriture.
// ============================================================================

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub category: &'static str,
    pub detail: String,
}

fn issue(category: &'static str, detail: impl Into<String>) -> Issue {
    Issue {
        category,
        detail: detail.into(),
    }
}

fn parse(raw: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S").ok()
}

fn orphan_med_doses(
    doses: &[nexus_db::MedDose],
    medications: &[nexus_db::Medication],
) -> Vec<Issue> {
    doses
        .iter()
        .filter(|d| !medications.iter().any(|m| m.id == d.med_id))
        .map(|d| {
            issue(
                "clé orpheline",
                format!(
                    "med_doses id={} référence medications id={} introuvable",
                    d.id, d.med_id
                ),
            )
        })
        .collect()
}

fn orphan_task_parents(tasks: &[nexus_db::Task]) -> Vec<Issue> {
    tasks
        .iter()
        .filter_map(|t| t.parent_id.as_ref().map(|pid| (t, pid)))
        .filter(|(_, pid)| !tasks.iter().any(|t2| &t2.id == *pid))
        .map(|(t, pid)| {
            issue(
                "clé orpheline",
                format!("tasks id={} : parent_id={pid} introuvable", t.id),
            )
        })
        .collect()
}

fn orphan_task_history(tasks: &[nexus_db::Task], history: &[nexus_db::TaskHistory]) -> Vec<Issue> {
    history
        .iter()
        .filter(|h| !tasks.iter().any(|t| t.id == h.task_id))
        .map(|h| {
            issue(
                "clé orpheline",
                format!(
                    "task_history id={} référence tasks id={} introuvable",
                    h.id, h.task_id
                ),
            )
        })
        .collect()
}

fn incoherent_task_dates(tasks: &[nexus_db::Task]) -> Vec<Issue> {
    let mut out = Vec::new();
    for t in tasks {
        match (parse(&t.created_at), parse(&t.updated_at)) {
            (Some(c), Some(u)) if u < c => out.push(issue(
                "date incohérente",
                format!(
                    "tasks id={} : updated_at ({}) antérieur à created_at ({})",
                    t.id, t.updated_at, t.created_at
                ),
            )),
            (None, _) => out.push(issue(
                "date incohérente",
                format!(
                    "tasks id={} : created_at illisible ('{}')",
                    t.id, t.created_at
                ),
            )),
            (_, None) => out.push(issue(
                "date incohérente",
                format!(
                    "tasks id={} : updated_at illisible ('{}')",
                    t.id, t.updated_at
                ),
            )),
            _ => {}
        }
    }
    out
}

fn missing_files(
    root: &Path,
    journal_entries: &[nexus_db::JournalEntry],
    articles: &[nexus_db::Article],
) -> Vec<Issue> {
    let mut out = Vec::new();
    for e in journal_entries {
        if !root.join(&e.file_path).is_file() {
            out.push(issue(
                "fichier manquant",
                format!(
                    "journal_entries date={} : {} introuvable sur disque",
                    e.date, e.file_path
                ),
            ));
        }
    }
    for a in articles {
        if !root.join(&a.file_path).is_file() {
            out.push(issue(
                "fichier manquant",
                format!(
                    "articles id={} : {} introuvable sur disque",
                    a.id, a.file_path
                ),
            ));
        }
    }
    out
}

/// Toutes les données brutes nécessaires aux contrôles — regroupées pour
/// que `run_all_checks` reste un seul point d'appel simple depuis l'UI.
pub struct Inputs<'a> {
    pub root: &'a Path,
    pub tasks: &'a [nexus_db::Task],
    pub task_history: &'a [nexus_db::TaskHistory],
    pub medications: &'a [nexus_db::Medication],
    pub med_doses: &'a [nexus_db::MedDose],
    pub journal_entries: &'a [nexus_db::JournalEntry],
    pub articles: &'a [nexus_db::Article],
}

pub fn run_all_checks(inputs: &Inputs) -> Vec<Issue> {
    let mut out = Vec::new();
    out.extend(orphan_med_doses(inputs.med_doses, inputs.medications));
    out.extend(orphan_task_parents(inputs.tasks));
    out.extend(orphan_task_history(inputs.tasks, inputs.task_history));
    out.extend(incoherent_task_dates(inputs.tasks));
    out.extend(missing_files(
        inputs.root,
        inputs.journal_entries,
        inputs.articles,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn med(id: &str) -> nexus_db::Medication {
        nexus_db::Medication {
            id: id.to_string(),
            nom: "M".into(),
            molecule: "m".into(),
            dose_default: 10.0,
            notes: None,
        }
    }

    fn dose(id: &str, med_id: &str) -> nexus_db::MedDose {
        nexus_db::MedDose {
            id: id.to_string(),
            med_id: med_id.to_string(),
            taken_at: "2026-07-12T09:00:00".into(),
            dose_mg: 10.0,
            ressenti: None,
            notes: None,
        }
    }

    fn task(
        id: &str,
        parent_id: Option<&str>,
        created_at: &str,
        updated_at: &str,
    ) -> nexus_db::Task {
        nexus_db::Task {
            id: id.to_string(),
            parent_id: parent_id.map(|s| s.to_string()),
            titre: "t".into(),
            statut: "backlog".into(),
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

    #[test]
    fn orphan_med_doses_detecte_reference_absente() {
        let doses = vec![dose("d1", "m_absent")];
        let issues = orphan_med_doses(&doses, &[med("m1")]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].category, "clé orpheline");
    }

    #[test]
    fn orphan_med_doses_rien_si_reference_valide() {
        let doses = vec![dose("d1", "m1")];
        assert!(orphan_med_doses(&doses, &[med("m1")]).is_empty());
    }

    #[test]
    fn orphan_task_parents_detecte_parent_absent() {
        let tasks = vec![task(
            "t1",
            Some("inexistant"),
            "2026-07-12T09:00:00",
            "2026-07-12T09:00:00",
        )];
        assert_eq!(orphan_task_parents(&tasks).len(), 1);
    }

    #[test]
    fn orphan_task_parents_rien_si_parent_present() {
        let tasks = vec![
            task("t1", None, "2026-07-12T09:00:00", "2026-07-12T09:00:00"),
            task(
                "t2",
                Some("t1"),
                "2026-07-12T09:00:00",
                "2026-07-12T09:00:00",
            ),
        ];
        assert!(orphan_task_parents(&tasks).is_empty());
    }

    #[test]
    fn incoherent_task_dates_detecte_updated_avant_created() {
        let tasks = vec![task(
            "t1",
            None,
            "2026-07-12T09:00:00",
            "2026-07-11T09:00:00",
        )];
        let issues = incoherent_task_dates(&tasks);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].category, "date incohérente");
    }

    #[test]
    fn incoherent_task_dates_detecte_date_illisible() {
        let tasks = vec![task("t1", None, "pas-une-date", "2026-07-12T09:00:00")];
        assert_eq!(incoherent_task_dates(&tasks).len(), 1);
    }

    #[test]
    fn incoherent_task_dates_rien_si_coherent() {
        let tasks = vec![task(
            "t1",
            None,
            "2026-07-11T09:00:00",
            "2026-07-12T09:00:00",
        )];
        assert!(incoherent_task_dates(&tasks).is_empty());
    }

    #[test]
    fn missing_files_detecte_fichier_absent() {
        let dir = tempfile::tempdir().expect("tmp");
        let entries = vec![nexus_db::JournalEntry {
            id: "j1".into(),
            date: "2026-07-12".into(),
            file_path: "01_journal/2026/2026-07-12.typst".into(),
            word_count: 0,
            tags: vec![],
        }];
        let issues = missing_files(dir.path(), &entries, &[]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].category, "fichier manquant");
    }

    #[test]
    fn missing_files_rien_si_fichier_present() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(dir.path().join("01_journal/2026")).expect("mkdir");
        std::fs::write(dir.path().join("01_journal/2026/2026-07-12.typst"), "x").expect("write");
        let entries = vec![nexus_db::JournalEntry {
            id: "j1".into(),
            date: "2026-07-12".into(),
            file_path: "01_journal/2026/2026-07-12.typst".into(),
            word_count: 0,
            tags: vec![],
        }];
        assert!(missing_files(dir.path(), &entries, &[]).is_empty());
    }

    #[test]
    fn run_all_checks_agrege_toutes_les_categories() {
        let dir = tempfile::tempdir().expect("tmp");
        let tasks = vec![task(
            "t1",
            Some("absent"),
            "pas-une-date",
            "2026-07-12T09:00:00",
        )];
        let inputs = Inputs {
            root: dir.path(),
            tasks: &tasks,
            task_history: &[],
            medications: &[],
            med_doses: &[dose("d1", "m_absent")],
            journal_entries: &[],
            articles: &[],
        };
        let issues = run_all_checks(&inputs);
        // parent orphelin + med_dose orpheline + created_at illisible : 3.
        assert_eq!(issues.len(), 3);
    }
}
