// ============================================================================
// modules/journal/src/entry.rs — Logique pure d'une entrée de journal
//
// Markdown-first (brief « fallback total Typst → Markdown », 15/07/2026) :
// toute NOUVELLE entrée est créée en 01_journal/YYYY/YYYY-MM-DD.md avec un
// gabarit Markdown. Une entrée .typst héritée (projet d'avant le fallback)
// reste ouverte et éditée TELLE QUELLE — jamais convertie, renommée ni
// écrasée ; si les deux extensions existent pour la même date, le .md (la
// copie de travail Markdown) est prioritaire. Frontmatter YAML (bloc `---`),
// parsing manuel — identique dans les deux formats.
// ============================================================================

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

fn dir_for(root: &Path, date: NaiveDate) -> PathBuf {
    root.join("01_journal").join(date.format("%Y").to_string())
}

/// Chemin de CRÉATION d'une entrée (Markdown, format principal).
pub fn path_for(root: &Path, date: NaiveDate) -> PathBuf {
    dir_for(root, date).join(format!("{}.md", date.format("%Y-%m-%d")))
}

/// Chemin hérité (.typst) d'une date — n'est plus jamais créé, seulement
/// détecté pour ouvrir les projets d'avant le fallback sans y toucher.
pub fn legacy_path_for(root: &Path, date: NaiveDate) -> PathBuf {
    dir_for(root, date).join(format!("{}.typst", date.format("%Y-%m-%d")))
}

/// Résout le fichier à OUVRIR pour une date : `.md` s'il existe (priorité
/// Markdown-first), sinon `.typst` hérité s'il existe (ouvert tel quel),
/// sinon le chemin `.md` à créer.
pub fn resolve_path_for(root: &Path, date: NaiveDate) -> PathBuf {
    let md = path_for(root, date);
    if md.exists() {
        return md;
    }
    let legacy = legacy_path_for(root, date);
    if legacy.exists() {
        return legacy;
    }
    md
}

/// Gabarit par défaut d'une nouvelle entrée (Markdown) — utilisé par
/// `config::JournalConfig::default()` quand aucune section "journal" n'est
/// présente dans Hive_RBMK.ron. `{date}` est le seul placeholder reconnu.
pub const DEFAULT_TEMPLATE: &str = "---\ntags: []\n---\n\n# Journal — {date}\n\n";

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
    fn path_for_cree_en_markdown() {
        let root = Path::new("/tmp/projet");
        assert_eq!(
            path_for(root, date("2026-07-12")),
            PathBuf::from("/tmp/projet/01_journal/2026/2026-07-12.md")
        );
    }

    #[test]
    fn resolve_prefere_md_puis_typst_herite_puis_creation_md(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        let d = date("2026-07-12");
        // Rien n'existe → création .md.
        assert_eq!(resolve_path_for(root, d), path_for(root, d));
        // Seul le .typst hérité existe → on l'ouvre TEL QUEL.
        std::fs::create_dir_all(dir_for(root, d))?;
        std::fs::write(legacy_path_for(root, d), "ancien contenu typst")?;
        assert_eq!(resolve_path_for(root, d), legacy_path_for(root, d));
        // Les deux existent → priorité Markdown.
        std::fs::write(path_for(root, d), "copie markdown")?;
        assert_eq!(resolve_path_for(root, d), path_for(root, d));
        Ok(())
    }

    #[test]
    fn default_template_est_markdown_pas_typst() {
        assert!(DEFAULT_TEMPLATE.contains("# Journal"));
        assert!(!DEFAULT_TEMPLATE.contains("= "));
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
