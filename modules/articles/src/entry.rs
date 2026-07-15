// ============================================================================
// modules/articles/src/entry.rs — Logique pure d'un article
//
// Fichier canonique 04_articles/<slug>.md (Markdown-first, brief 15/07/2026 ;
// les .typst hérités restent lus tels quels). Frontmatter à 5 champs
// (doc §5.5) : titre, statut, tags, date_cible, destination. Même patron de
// parsing manuel que journal/src/entry.rs (§7.4 : pas de serde_yaml pour un
// format interne restreint et contrôlé — journal a établi ce choix pour 1
// champ, ici étendu à 5 champs plats, toujours pas de listes imbriquées ni de
// types complexes qui justifieraient un vrai parseur YAML). Dupliqué
// volontairement plutôt qu'importé depuis journal : deux modules pairs du
// même registre (§7.1), pas de dépendance croisée entre crates de modules.
// ============================================================================

use std::path::{Path, PathBuf};

/// Un article n'a pas de concept "aujourd'hui" (contrairement au journal) :
/// les 5 champs frontmatter du doc §5.5, plus le corps tenu séparément.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArticleMeta {
    pub titre: String,
    pub statut: String,
    pub tags: Vec<String>,
    pub date_cible: String,
    pub destination: String,
}

/// Valeurs valides de `statut` (doc §5.5 : « brouillon | revue | publié |
/// archivé »).
pub const STATUTS: &[&str] = &["brouillon", "revue", "publié", "archivé"];

/// Vitesse de lecture silencieuse moyenne — valeur couramment citée pour un
/// texte en français (200-250 mots/min) ; on retient la borne basse pour ne
/// jamais SOUS-estimer le temps affiché (doc §5.5 : « temps de lecture
/// estimé », aucune mesure personnelle disponible dans ce projet).
pub const WORDS_PER_MINUTE: i64 = 200;

/// Réduit un titre à un slug de nom de fichier : minuscules, accents
/// français retirés, tout ce qui n'est pas alphanumérique devient `_`,
/// séparateurs consécutifs fusionnés, bords élagués. Jamais vide si `titre`
/// contient au moins un caractère exploitable — sinon "article".
pub fn slugify(titre: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for c in titre.chars() {
        let base = strip_accent(c).to_ascii_lowercase();
        if base.is_ascii_alphanumeric() {
            out.push(base);
            last_was_sep = false;
        } else if !last_was_sep && !out.is_empty() {
            out.push('_');
            last_was_sep = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "article".to_string()
    } else {
        out
    }
}

fn strip_accent(c: char) -> char {
    match c {
        // Formes majuscules incluses : `to_ascii_lowercase()` (appelé par
        // l'appelant après cette fonction) ne touche PAS les caractères hors
        // ASCII, donc 'É' non traité ici resterait accentué et serait
        // classé non-alphanumérique — perdu silencieusement en tête de
        // chaîne (bug réel attrapé par slugify_retire_les_accents).
        'à' | 'â' | 'ä' | 'á' | 'À' | 'Â' | 'Ä' | 'Á' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'î' | 'ï' | 'í' | 'ì' | 'Î' | 'Ï' | 'Í' | 'Ì' => 'i',
        'ô' | 'ö' | 'ó' | 'ò' | 'Ô' | 'Ö' | 'Ó' | 'Ò' => 'o',
        'û' | 'ü' | 'ù' | 'ú' | 'Û' | 'Ü' | 'Ù' | 'Ú' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        other => other,
    }
}

/// Chemin de CRÉATION d'un article pour un `slug` donné (Markdown-first,
/// brief 15/07/2026 : `04_articles/slug_titre.md`). Les `.typst` hérités
/// s'ouvrent inchangés via leur chemin stocké en DB (agnostique). L'appelant
/// est responsable de désambiguïser `slug` si le fichier existe déjà —
/// cette fonction reste pure, sans accès disque.
pub fn path_for(root: &Path, slug: &str) -> PathBuf {
    root.join("04_articles").join(format!("{slug}.md"))
}

/// Chemin hérité (.typst) d'un slug — plus jamais créé ; sert uniquement à
/// la désambiguïsation (un nouvel article ne doit pas prendre le nom d'un
/// article Typst hérité, même si l'extension diffère).
pub fn legacy_path_for(root: &Path, slug: &str) -> PathBuf {
    root.join("04_articles").join(format!("{slug}.typst"))
}

/// Contenu initial d'un nouvel article (doc §5.5, frontmatter exact ;
/// corps Markdown).
pub fn default_template(titre: &str) -> String {
    format!(
        "---\ntitre: \"{titre}\"\nstatut: brouillon\ntags: []\ndate_cible: \"\"\ndestination: \"\"\n---\n\n# {titre}\n\n"
    )
}

/// Sépare le frontmatter YAML (délimité par `---`) du corps. Sans bloc
/// valide : frontmatter vide, tout est corps (même contrat que
/// journal::entry::split_frontmatter).
pub fn split_frontmatter(content: &str) -> (&str, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return ("", content);
    };
    let Some(close) = rest.find("\n---\n") else {
        return ("", content);
    };
    (&rest[..close], &rest[close + 5..])
}

/// Valeur d'un champ scalaire `cle: valeur` (guillemets optionnels). Chaîne
/// vide si absent — jamais d'erreur, jamais de donnée inventée.
fn scalar_field(frontmatter: &str, key: &str) -> String {
    let prefix = format!("{key}:");
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(&prefix) {
            return value.trim().trim_matches('"').to_string();
        }
    }
    String::new()
}

/// Extrait la liste `tags:` — forme inline (`[a, b]`) ou liste (`- a`).
fn parse_tags(frontmatter: &str) -> Vec<String> {
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

/// Parse les 5 champs frontmatter du doc §5.5 depuis le contenu complet du
/// fichier. `statut` retombe sur "brouillon" si absent ou vide (doc : valeur
/// par défaut d'un nouvel article).
pub fn parse_meta(content: &str) -> ArticleMeta {
    let (fm, _) = split_frontmatter(content);
    let statut = scalar_field(fm, "statut");
    ArticleMeta {
        titre: scalar_field(fm, "titre"),
        statut: if statut.is_empty() {
            "brouillon".to_string()
        } else {
            statut
        },
        tags: parse_tags(fm),
        date_cible: scalar_field(fm, "date_cible"),
        destination: scalar_field(fm, "destination"),
    }
}

/// Réécrit le frontmatter d'un contenu existant avec `meta`, corps conservé
/// tel quel. Utilisé quand la métadonnée est éditée depuis le formulaire.
pub fn with_meta(content: &str, meta: &ArticleMeta) -> String {
    let (_, body) = split_frontmatter(content);
    let tags = meta
        .tags
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "---\ntitre: \"{}\"\nstatut: {}\ntags: [{}]\ndate_cible: \"{}\"\ndestination: \"{}\"\n---\n{}",
        meta.titre, meta.statut, tags, meta.date_cible, meta.destination, body
    )
}

/// Compte les mots du corps (hors frontmatter) — les métadonnées ne
/// polluent jamais les stats, même règle que journal.
pub fn word_count(body: &str) -> i64 {
    body.split_whitespace().count() as i64
}

/// Temps de lecture estimé en minutes, arrondi au supérieur (doc §5.5 :
/// « stats simples : nombre de mots, temps de lecture estimé » — jamais 0
/// minute affichée pour un texte non vide).
pub fn reading_time_minutes(word_count: i64) -> i64 {
    if word_count <= 0 {
        return 0;
    }
    // i64::div_ceil est instable sur cette chaîne d'outils (int_roundings,
    // #88581) : division entière manuelle, valable pour des opérandes > 0.
    (word_count + WORDS_PER_MINUTE - 1) / WORDS_PER_MINUTE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_minuscule_et_underscore() {
        assert_eq!(slugify("Le Burnout Autistique"), "le_burnout_autistique");
    }

    #[test]
    fn slugify_retire_les_accents() {
        assert_eq!(slugify("Épuisement révélé"), "epuisement_revele");
    }

    #[test]
    fn slugify_fusionne_separateurs_consecutifs() {
        assert_eq!(slugify("un   -- test !!"), "un_test");
    }

    #[test]
    fn slugify_vide_retombe_sur_article() {
        assert_eq!(slugify("!!!"), "article");
    }

    #[test]
    fn path_for_forme_attendue() {
        let root = Path::new("/tmp/projet");
        assert_eq!(
            path_for(root, "burnout_autistique"),
            PathBuf::from("/tmp/projet/04_articles/burnout_autistique.md")
        );
    }

    #[test]
    fn parse_meta_tous_les_champs() {
        let content = "---\ntitre: \"Le burnout\"\nstatut: revue\ntags: [neurodivergence, sant\u{e9}]\ndate_cible: 2026-08-01\ndestination: blog\n---\n\nCorps.";
        let meta = parse_meta(content);
        assert_eq!(meta.titre, "Le burnout");
        assert_eq!(meta.statut, "revue");
        assert_eq!(
            meta.tags,
            vec!["neurodivergence".to_string(), "sant\u{e9}".to_string()]
        );
        assert_eq!(meta.date_cible, "2026-08-01");
        assert_eq!(meta.destination, "blog");
    }

    #[test]
    fn parse_meta_statut_absent_retombe_sur_brouillon() {
        let content = "---\ntitre: \"X\"\n---\n\nCorps.";
        assert_eq!(parse_meta(content).statut, "brouillon");
    }

    #[test]
    fn with_meta_conserve_le_corps_et_round_trip() {
        let content =
            "---\ntitre: \"A\"\nstatut: brouillon\ntags: []\ndate_cible: \"\"\ndestination: \"\"\n---\n\nCorps original.";
        let meta = ArticleMeta {
            titre: "A".into(),
            statut: "publié".into(),
            tags: vec!["x".into()],
            date_cible: "2026-08-01".into(),
            destination: "blog".into(),
        };
        let rewritten = with_meta(content, &meta);
        assert!(rewritten.contains("statut: publié"));
        assert!(rewritten.contains("Corps original."));
        assert_eq!(parse_meta(&rewritten), meta);
    }

    #[test]
    fn word_count_ignore_espaces() {
        assert_eq!(word_count("  un   deux\ntrois  "), 3);
    }

    #[test]
    fn word_count_frontmatter_exclu() {
        let content = "---\ntitre: \"X\"\n---\n\nDeux mots.";
        let (_, body) = split_frontmatter(content);
        assert_eq!(word_count(body), 2);
    }

    #[test]
    fn reading_time_arrondi_superieur() {
        assert_eq!(reading_time_minutes(0), 0);
        assert_eq!(reading_time_minutes(1), 1);
        assert_eq!(reading_time_minutes(200), 1);
        assert_eq!(reading_time_minutes(201), 2);
        assert_eq!(reading_time_minutes(450), 3);
    }
}
