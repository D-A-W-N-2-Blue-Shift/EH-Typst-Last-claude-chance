// ============================================================================
// modules/file_tree/src/temporal.rs — Extraction temporelle (Matrice Timeline)
//
// Deux sources, un seul format de saisie côté auteur :
//
//   Fichier SCÈNE (frontmatter YAML) :
//     ---
//     date: 15-03-1942 08h30
//     lieu: Berlin
//     ordre: 12
//     personnages: [Anna, Karl]
//     ---
//
//   Fichier PERSONNAGE (frontmatter YAML) :
//     ---
//     chronologie:
//       - 15-03-1942 - doute, veut fuir
//       - 1943 - rupture définitive
//     ---
//
// FORMAT DE DATE (saisie) : `dd-mm-yyyy HHhMM`, précision variable :
//     1942                  année seule
//     03-1942               mois + année
//     15-03-1942            jour complet
//     15-03-1942 08h30      jour + heure
//
// Le format jour-mois-année NE se trie PAS chronologiquement en lexicographie.
// On stocke donc une CLÉ TRIABLE séparée (`sort`, big-endian zero-paddée
// YYYYMMDDHHMM) tout en conservant l'affichage tel que saisi (`display`).
// L'application ne modifie JAMAIS les fichiers sources (lecture seule).
// ============================================================================

/// Une date d'événement : clé de tri + forme affichée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDate {
    /// Clé lexicographiquement = chronologiquement triable : `YYYYMMDDHHMM`,
    /// composantes inconnues à `00`.
    pub sort: String,
    /// Forme affichée, normalisée à la précision saisie (`dd-mm-yyyy HHhMM`).
    pub display: String,
}

/// Métadonnées d'un fichier scène (présence du champ `date` dans le YAML).
#[derive(Debug, Clone)]
pub struct SceneMeta {
    pub date: EventDate,
    pub lieu: Option<String>,
    pub ordre: Option<i64>,
    pub personnages: Vec<String>,
}

/// Une ligne de la chronologie interne d'une fiche personnage.
#[derive(Debug, Clone)]
pub struct ChronoEntry {
    pub date: EventDate,
    pub note: String,
}

/// Résultat d'extraction d'un frontmatter.
#[derive(Debug, Default)]
pub struct Temporal {
    pub scene: Option<SceneMeta>,
    pub chrono: Vec<ChronoEntry>,
}

/// Frontmatter brut : tous les champs optionnels, champs inconnus ignorés
/// (serde_yaml n'impose pas `deny_unknown_fields`). Un YAML existant
/// (`goal:`, `statut:`…) se désérialise donc sans erreur.
#[derive(serde::Deserialize, Default)]
struct RawFront {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    lieu: Option<String>,
    #[serde(default)]
    ordre: Option<i64>,
    #[serde(default)]
    personnages: Vec<String>,
    #[serde(default)]
    chronologie: Vec<String>,
}

/// Extrait scène + chronologie depuis le bloc YAML d'un fichier.
/// YAML vide ou invalide → résultat vide (jamais de panique : doctrine §5/§6).
pub fn extract(yaml: &str) -> Temporal {
    if yaml.trim().is_empty() {
        return Temporal::default();
    }
    let raw: RawFront = match serde_yaml::from_str(yaml) {
        Ok(r) => r,
        Err(_) => return Temporal::default(),
    };

    let scene = raw
        .date
        .as_deref()
        .and_then(parse_event_date)
        .map(|date| SceneMeta {
            date,
            lieu: raw.lieu.clone().filter(|s| !s.trim().is_empty()),
            ordre: raw.ordre,
            personnages: raw
                .personnages
                .iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
        });

    let chrono = raw
        .chronologie
        .iter()
        .filter_map(|line| parse_chrono_line(line))
        .collect();

    Temporal { scene, chrono }
}

/// `"15-03-1942 08h30 - doute, veut fuir"` → (date, note).
/// Séparateur date/note : ` - ` (espace-tiret-espace). Les tirets internes de
/// la date (`15-03-1942`) n'ont pas d'espaces autour : pas d'ambiguïté.
fn parse_chrono_line(line: &str) -> Option<ChronoEntry> {
    let (date_s, note) = line.split_once(" - ")?;
    let date = parse_event_date(date_s)?;
    Some(ChronoEntry {
        date,
        note: note.trim().to_string(),
    })
}

/// Parse `dd-mm-yyyy HHhMM` à précision variable. `None` si malformé (la ligne
/// est alors ignorée, jamais de panique).
pub fn parse_event_date(raw: &str) -> Option<EventDate> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut sp = raw.splitn(2, char::is_whitespace);
    let date_part = sp.next()?.trim();
    let time_part = sp.next().map(str::trim).filter(|s| !s.is_empty());

    let comps: Vec<&str> = date_part.split('-').collect();
    let (day, month, year) = match comps.as_slice() {
        [y] => (None, None, parse_bounded(y, 1, 9999)?),
        [m, y] => (
            None,
            Some(parse_bounded(m, 1, 12)?),
            parse_bounded(y, 1, 9999)?,
        ),
        [d, m, y] => (
            Some(parse_bounded(d, 1, 31)?),
            Some(parse_bounded(m, 1, 12)?),
            parse_bounded(y, 1, 9999)?,
        ),
        _ => return None,
    };

    // L'heure n'a de sens que sur une date complète (jour connu).
    let (hour, minute) = match time_part {
        Some(t) => {
            if day.is_none() {
                return None;
            }
            let mut th = t.split('h');
            let h = parse_bounded(th.next()?, 0, 23)?;
            let m = parse_bounded(th.next()?, 0, 59)?;
            if th.next().is_some() {
                return None;
            }
            (Some(h), Some(m))
        }
        None => (None, None),
    };

    let sort = format!(
        "{:04}{:02}{:02}{:02}{:02}",
        year,
        month.unwrap_or(0),
        day.unwrap_or(0),
        hour.unwrap_or(0),
        minute.unwrap_or(0)
    );
    let display = match (day, month, hour) {
        (Some(d), Some(m), Some(h)) => {
            format!(
                "{:02}-{:02}-{:04} {:02}h{:02}",
                d,
                m,
                year,
                h,
                minute.unwrap_or(0)
            )
        }
        (Some(d), Some(m), None) => format!("{:02}-{:02}-{:04}", d, m, year),
        (None, Some(m), _) => format!("{:02}-{:04}", m, year),
        _ => format!("{:04}", year),
    };
    Some(EventDate { sort, display })
}

fn parse_bounded(s: &str, lo: u32, hi: u32) -> Option<u32> {
    let v: u32 = s.trim().parse().ok()?;
    (lo..=hi).contains(&v).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sort_of(raw: &str) -> Option<String> {
        parse_event_date(raw).map(|d| d.sort)
    }

    #[test]
    fn date_precision_variable() {
        assert_eq!(sort_of("1942").as_deref(), Some("194200000000"));
        assert_eq!(sort_of("03-1942").as_deref(), Some("194203000000"));
        assert_eq!(sort_of("15-03-1942").as_deref(), Some("194203150000"));
        assert_eq!(sort_of("15-03-1942 08h30").as_deref(), Some("194203150830"));
    }

    #[test]
    fn tri_lexicographique_est_chronologique() -> Result<(), Box<dyn std::error::Error>> {
        // Le format jour-premier saisi par l'auteur trie quand même juste.
        let mut v = vec![
            sort_of("15-03-1943").ok_or("date invalide")?,
            sort_of("01-01-1942").ok_or("date invalide")?,
            sort_of("1942").ok_or("date invalide")?,
            sort_of("15-03-1942 08h30").ok_or("date invalide")?,
            sort_of("15-03-1942 06h00").ok_or("date invalide")?,
        ];
        v.sort();
        assert_eq!(
            v,
            vec![
                "194200000000", // 1942 (année seule, avant tout jour de 1942)
                "194201010000", // 01-01-1942
                "194203150600", // 15-03-1942 06h00
                "194203150830", // 15-03-1942 08h30
                "194303150000", // 15-03-1943
            ]
        );
        Ok(())
    }

    #[test]
    fn affichage_conserve_la_precision() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            parse_event_date("15-03-1942 08h30")
                .ok_or("date invalide")?
                .display,
            "15-03-1942 08h30"
        );
        assert_eq!(
            parse_event_date("15-03-1942")
                .ok_or("date invalide")?
                .display,
            "15-03-1942"
        );
        assert_eq!(
            parse_event_date("3-1942").ok_or("date invalide")?.display,
            "03-1942"
        );
        assert_eq!(
            parse_event_date("1942").ok_or("date invalide")?.display,
            "1942"
        );
        Ok(())
    }

    #[test]
    fn dates_invalides_rejetees() {
        assert!(parse_event_date("").is_none());
        assert!(parse_event_date("abc").is_none());
        assert!(parse_event_date("32-03-1942").is_none()); // jour > 31
        assert!(parse_event_date("15-13-1942").is_none()); // mois > 12
        assert!(parse_event_date("15-03-1942 25h00").is_none()); // heure > 23
        assert!(parse_event_date("1942 08h30").is_none()); // heure sans jour
        assert!(parse_event_date("15-03-1942 0830").is_none()); // heure sans 'h'
    }

    #[test]
    fn scene_extraite_du_yaml() -> Result<(), Box<dyn std::error::Error>> {
        let t =
            extract("date: 15-03-1942 08h30\nlieu: Berlin\nordre: 12\npersonnages: [Anna, Karl]");
        let s = t.scene.ok_or("scène attendue dans le YAML")?;
        assert_eq!(s.date.sort, "194203150830");
        assert_eq!(s.lieu.as_deref(), Some("Berlin"));
        assert_eq!(s.ordre, Some(12));
        assert_eq!(s.personnages, vec!["Anna", "Karl"]);
        assert!(t.chrono.is_empty());
        Ok(())
    }

    #[test]
    fn chronologie_perso_extraite() {
        let t = extract("chronologie:\n  - 15-03-1942 - doute, veut fuir\n  - 1943 - rupture");
        assert!(t.scene.is_none());
        assert_eq!(t.chrono.len(), 2);
        assert_eq!(t.chrono[0].date.sort, "194203150000");
        assert_eq!(t.chrono[0].note, "doute, veut fuir");
        assert_eq!(t.chrono[1].date.sort, "194300000000");
        assert_eq!(t.chrono[1].note, "rupture");
    }

    #[test]
    fn yaml_existant_sans_champs_temporels_ne_casse_pas() {
        let t = extract("goal: 1500\nstatut: en cours");
        assert!(t.scene.is_none());
        assert!(t.chrono.is_empty());
    }

    #[test]
    fn yaml_vide_ou_invalide_degrade_sans_paniquer() {
        assert!(extract("").scene.is_none());
        assert!(extract("   ").scene.is_none());
        assert!(extract("ceci: [n'est pas: du yaml valide").scene.is_none());
    }

    #[test]
    fn ligne_chrono_sans_separateur_ignoree() {
        let t = extract("chronologie:\n  - sans tiret separateur\n  - 1943 - ok");
        assert_eq!(t.chrono.len(), 1);
        assert_eq!(t.chrono[0].note, "ok");
    }
}
