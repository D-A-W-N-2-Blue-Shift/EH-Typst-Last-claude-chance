// ============================================================================
// modules/dashboard/src/ics.rs — Export .ics des tâches avec échéance (doc §7)
//
// « Les tâches avec échéance dans tasks sont exportables en fichier .ics
// standard depuis le dashboard [...] Zéro réseau, zéro authentification,
// zéro dépendance. » RFC 5545 minimal écrit à la main (un VEVENT par tâche
// à échéance connue, événement journée entière — `echeance` est une DATE,
// pas un horodatage, doc §8). CRLF (RFC 5545 §3.1) : forme correcte, la
// plupart des agendas acceptent aussi LF mais autant faire juste du premier
// coup pour un format qui sera importé dans des agendas tiers non testables
// depuis cet environnement headless.
// ============================================================================

const CRLF: &str = "\r\n";

/// Échappe une valeur TEXT selon RFC 5545 §3.3.11 : `\`, `;`, `,` et les
/// sauts de ligne doivent être échappés.
fn escape_text(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

/// Génère le contenu .ics pour les tâches ayant une `echeance` valide
/// (YYYY-MM-DD). Les tâches sans échéance ou à date illisible sont
/// silencieusement omises de l'export — rien à exporter pour elles, pas
/// une erreur (§7.5 : ce n'est pas un cas d'échec, juste un cas vide).
pub fn tasks_to_ics(
    tasks: &[nexus_db::Task],
    generated_at: chrono::DateTime<chrono::Utc>,
) -> String {
    let dtstamp = generated_at.format("%Y%m%dT%H%M%SZ").to_string();
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR");
    out.push_str(CRLF);
    out.push_str("VERSION:2.0");
    out.push_str(CRLF);
    out.push_str("PRODID:-//Nexus//Hive-RBMK-mod-Tcherenkov//FR");
    out.push_str(CRLF);
    for t in tasks {
        let Some(echeance) = &t.echeance else {
            continue;
        };
        let Ok(date) = chrono::NaiveDate::parse_from_str(echeance, "%Y-%m-%d") else {
            continue;
        };
        out.push_str("BEGIN:VEVENT");
        out.push_str(CRLF);
        out.push_str(&format!("UID:{}@nexus.hive-rbmk-tcherenkov", t.id));
        out.push_str(CRLF);
        out.push_str(&format!("DTSTAMP:{dtstamp}"));
        out.push_str(CRLF);
        out.push_str(&format!("DTSTART;VALUE=DATE:{}", date.format("%Y%m%d")));
        out.push_str(CRLF);
        out.push_str(&format!("SUMMARY:{}", escape_text(&t.titre)));
        out.push_str(CRLF);
        out.push_str("END:VEVENT");
        out.push_str(CRLF);
    }
    out.push_str("END:VCALENDAR");
    out.push_str(CRLF);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn task(id: &str, titre: &str, echeance: Option<&str>) -> nexus_db::Task {
        nexus_db::Task {
            id: id.to_string(),
            parent_id: None,
            titre: titre.to_string(),
            statut: "today".into(),
            priorite: None,
            energie: None,
            contexte: None,
            duree_min: None,
            echeance: echeance.map(|s| s.to_string()),
            created_at: "2026-07-12T09:00:00".into(),
            updated_at: "2026-07-12T09:00:00".into(),
            notes: None,
        }
    }

    fn ts() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 7, 12, 10, 0, 0).unwrap()
    }

    #[test]
    fn enveloppe_calendar_toujours_presente_meme_vide() {
        let ics = tasks_to_ics(&[], ts());
        assert!(ics.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(ics.trim_end().ends_with("END:VCALENDAR"));
        assert!(!ics.contains("BEGIN:VEVENT"));
    }

    #[test]
    fn tache_sans_echeance_omise() {
        let tasks = vec![task("t1", "Sans date", None)];
        let ics = tasks_to_ics(&tasks, ts());
        assert!(!ics.contains("BEGIN:VEVENT"));
    }

    #[test]
    fn tache_avec_echeance_illisible_omise() {
        let tasks = vec![task("t1", "Date cassée", Some("pas-une-date"))];
        let ics = tasks_to_ics(&tasks, ts());
        assert!(!ics.contains("BEGIN:VEVENT"));
    }

    #[test]
    fn tache_avec_echeance_genere_un_vevent() {
        let tasks = vec![task("t1", "Dossier MDPH", Some("2026-07-20"))];
        let ics = tasks_to_ics(&tasks, ts());
        assert!(ics.contains("BEGIN:VEVENT"));
        assert!(ics.contains("UID:t1@nexus.hive-rbmk-tcherenkov"));
        assert!(ics.contains("DTSTART;VALUE=DATE:20260720"));
        assert!(ics.contains("SUMMARY:Dossier MDPH"));
        assert!(ics.contains("DTSTAMP:20260712T100000Z"));
        assert!(ics.contains("END:VEVENT"));
    }

    #[test]
    fn titre_echappe_les_caracteres_speciaux() {
        let tasks = vec![task("t1", "Appel; urgent, vite\\!", Some("2026-07-20"))];
        let ics = tasks_to_ics(&tasks, ts());
        assert!(ics.contains("SUMMARY:Appel\\; urgent\\, vite\\\\!"));
    }

    #[test]
    fn plusieurs_taches_generent_plusieurs_vevent() {
        let tasks = vec![
            task("t1", "Un", Some("2026-07-20")),
            task("t2", "Deux", Some("2026-07-21")),
        ];
        let ics = tasks_to_ics(&tasks, ts());
        assert_eq!(ics.matches("BEGIN:VEVENT").count(), 2);
    }
}
