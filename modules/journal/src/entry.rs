// ============================================================================
// modules/journal/src/entry.rs — Logique pure d'une entrée de journal
//
// Chemin canonique 01_journal/YYYY/YYYY-MM-DD.typst (doc §5.2), template
// configurable via la section "journal" de Hive_RBMK.ron (config.rs) — `
// DEFAULT_TEMPLATE` ici est la valeur par défaut de cette section, pas un
// gabarit figé. Frontmatter YAML minimal (même convention que le reste de
// l'écosystème Typst du projet : bloc `---`), parsing manuel — pas de
// dépendance serde_yaml pour un besoin aussi restreint (§7.4).
// ============================================================================

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

/// Chemin du fichier d'une date donnée (utilisé pour aujourd'hui ET pour la
/// navigation depuis la sidebar).
pub fn path_for(root: &Path, date: NaiveDate) -> PathBuf {
    root.join("01_journal")
        .join(date.format("%Y").to_string())
        .join(format!("{}.typst", date.format("%Y-%m-%d")))
}

/// Gabarit par défaut d'une nouvelle entrée — utilisé par
/// `config::JournalConfig::default()` quand aucune section "journal" n'est
/// présente dans Hive_RBMK.ron. `{date}` est le seul placeholder reconnu.
pub const DEFAULT_TEMPLATE: &str = "---\ntags: []\n---\n\n= Journal — {date}\n\n";

/// Substitue `{date}` (YYYY-MM-DD) dans `template`. Un gabarit sans le
/// marqueur est retourné tel quel — jamais d'erreur (§7.5 : la
/// configurabilité ne doit pas pouvoir planter la création d'entrée).
pub fn render_template(template: &str, date: NaiveDate) -> String {
    template.replace("{date}", &date.format("%Y-%m-%d").to_string())
}

/// Sépare le frontmatter YAML (délimité par `---`) du corps. Sans bloc
/// valide : frontmatter vide, tout est corps.
pub fn split_frontmatter(content: &str) -> (&str, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return ("", content);
    };
    let Some(close) = rest.find("\n---\n") else {
        return ("", content);
    };
    (&rest[..close], &rest[close + 5..])
}

/// Extrait la liste `tags:` du frontmatter, forme inline (`tags: [a, b]`)
/// ou liste (`tags:\n  - a\n  - b`). Vide si absent ou illisible — jamais
/// d'erreur, jamais de donnée inventée.
pub fn parse_tags(frontmatter: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut in_tags_block = false;
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(inline) = trimmed.strip_prefix("tags:") {
            let inline = inline.trim();
            if let Some(bracketed) = inline.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                tags.extend(
                    bracketed
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty()),
                );
                in_tags_block = false;
            } else if inline.is_empty() {
                in_tags_block = true;
            } else {
                in_tags_block = false;
            }
            continue;
        }
        if in_tags_block {
            if let Some(item) = trimmed.strip_prefix("- ") {
                tags.push(item.trim().trim_matches('"').to_string());
                continue;
            }
            in_tags_block = false;
        }
    }
    tags
}

/// Compte les mots du corps (hors frontmatter) — les métadonnées ne
/// polluent jamais les stats, même règle que le reste du projet.
pub fn word_count(body: &str) -> i64 {
    body.split_whitespace().count() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date de test valide")
    }

    #[test]
    fn path_for_forme_attendue() {
        let root = Path::new("/tmp/projet");
        assert_eq!(
            path_for(root, date("2026-07-12")),
            PathBuf::from("/tmp/projet/01_journal/2026/2026-07-12.typst")
        );
    }

    #[test]
    fn split_frontmatter_bloc_present() {
        let content = "---\ntags: [a, b]\n---\n\nCorps ici.";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm, "tags: [a, b]");
        assert_eq!(body, "\nCorps ici.");
    }

    #[test]
    fn split_frontmatter_absent() {
        let content = "Pas de frontmatter.";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm, "");
        assert_eq!(body, content);
    }

    #[test]
    fn parse_tags_forme_inline() {
        assert_eq!(
            parse_tags("tags: [burnout, sommeil]"),
            vec!["burnout".to_string(), "sommeil".to_string()]
        );
    }

    #[test]
    fn parse_tags_forme_liste() {
        let fm = "tags:\n  - burnout\n  - sommeil\nautre: valeur";
        assert_eq!(
            parse_tags(fm),
            vec!["burnout".to_string(), "sommeil".to_string()]
        );
    }

    #[test]
    fn parse_tags_absent() {
        assert_eq!(parse_tags("autre: valeur"), Vec::<String>::new());
    }

    #[test]
    fn parse_tags_liste_vide_inline() {
        assert_eq!(parse_tags("tags: []"), Vec::<String>::new());
    }

    #[test]
    fn word_count_ignore_espaces() {
        assert_eq!(word_count("  un   deux\ntrois  "), 3);
    }

    #[test]
    fn render_template_substitue_la_date() {
        let out = render_template(DEFAULT_TEMPLATE, date("2026-07-12"));
        assert!(out.contains("2026-07-12"));
        assert!(!out.contains("{date}"));
    }

    #[test]
    fn render_template_gabarit_personnalise_sans_marqueur() {
        let out = render_template("Pas de placeholder ici.", date("2026-07-12"));
        assert_eq!(out, "Pas de placeholder ici.");
    }

    #[test]
    fn word_count_frontmatter_exclu() {
        let content = "---\ntags: [a, b, c, d]\n---\n\nDeux mots.";
        let (_, body) = split_frontmatter(content);
        assert_eq!(word_count(body), 2);
    }
}
