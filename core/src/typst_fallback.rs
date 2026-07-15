// ============================================================================
// core/src/typst_fallback.rs — Conversion SÛRE Typst → Markdown (texte pur)
//
// Brief « fallback total Typst → Markdown » (15/07/2026), Phases 5-6 :
// utilitaire de la commande explicite « Créer une copie Markdown ». Aucune
// conversion automatique nulle part — cette fonction n'est appelée QUE sur
// action utilisateur, produit un NOUVEAU texte (l'appelant écrit une copie,
// jamais l'original) et n'invente JAMAIS de traduction :
//
//   - titres Typst (`= Titre`, `== Sous-titre`, …) → titres Markdown
//     (`# Titre`, `## Sous-titre`, …) — seule structure convertie, la seule
//     que les gabarits de ce projet aient jamais émise ;
//   - frontmatter YAML (bloc `---`) : identique dans les deux formats,
//     conservé verbatim ;
//   - toute ligne de code Typst (directive `#import`, `#let`, `#show`,
//     appel de fonction…) : NON traduite — conservée telle quelle dans un
//     bloc balisé au format exact du brief, à traiter manuellement ;
//   - tout le reste (prose, listes, lignes vides) : verbatim.
//
// Place dans le core : utilitaire TEXTE pur, sans nom de module, sans I/O —
// partagé par les modules qui éditent des fichiers narratifs (même statut
// d'infrastructure que `fs::atomic_write`). Le core reste aveugle aux
// modules (§7.2).
// ============================================================================

/// Marqueurs du brief pour la source Typst non convertie.
pub const UNCONVERTED_BEGIN: &str = "<!-- ENGRAM_TYPST_UNCONVERTED_BEGIN -->";
pub const UNCONVERTED_END: &str = "<!-- ENGRAM_TYPST_UNCONVERTED_END -->";

/// Résultat d'une conversion sûre.
pub struct TypstToMd {
    pub markdown: String,
    pub headings_converted: usize,
    pub unconverted_blocks: usize,
}

/// Une ligne est-elle du code Typst non traduisible (directive/appel) ?
/// En Typst, `#` en tête de ligne introduit du code (`#import`, `#let`,
/// `#show`, `#figure(...)`) — précisément ce qu'il ne faut PAS copier nu
/// dans un `.md` où `#` signifie « titre ».
fn is_typst_code_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#')
        && t.chars()
            .nth(1)
            .is_some_and(|c| c.is_alphabetic() || c == '{' || c == '(')
}

/// Ligne de titre Typst (`=`+ espace) → niveau et texte.
fn typst_heading(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let level = t.chars().take_while(|c| *c == '=').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &t[level..];
    rest.strip_prefix(' ').map(|text| (level, text))
}

/// Convertit les structures sûres, préserve le reste. Voir l'en-tête.
pub fn typst_to_md_safe(source: &str) -> TypstToMd {
    // Frontmatter YAML : conservé verbatim (identique dans les deux formats).
    let (frontmatter, body) = match source.strip_prefix("---\n") {
        Some(rest) => match rest.find("\n---\n") {
            Some(close) => (&source[..close + 4 + 4 + 1], &source[close + 4 + 4 + 1..]),
            None => ("", source),
        },
        None => ("", source),
    };

    let mut out = String::with_capacity(source.len() + 64);
    out.push_str(frontmatter);
    let mut headings = 0usize;
    let mut blocks = 0usize;
    let mut pending_code: Vec<&str> = Vec::new();

    let flush_code = |out: &mut String, pending: &mut Vec<&str>, blocks: &mut usize| {
        if pending.is_empty() {
            return;
        }
        *blocks += 1;
        out.push_str(UNCONVERTED_BEGIN);
        out.push_str("\n```typst\n");
        for l in pending.iter() {
            out.push_str(l);
            out.push('\n');
        }
        out.push_str("```\n");
        out.push_str(UNCONVERTED_END);
        out.push('\n');
        pending.clear();
    };

    for line in body.lines() {
        if is_typst_code_line(line) {
            pending_code.push(line);
            continue;
        }
        flush_code(&mut out, &mut pending_code, &mut blocks);
        if let Some((level, text)) = typst_heading(line) {
            headings += 1;
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(text);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    flush_code(&mut out, &mut pending_code, &mut blocks);

    TypstToMd {
        markdown: out,
        headings_converted: headings,
        unconverted_blocks: blocks,
    }
}

/// Rapport de conversion (brief Phase 6 : `<nom>.typ-to-md-report.md`).
pub fn conversion_report(source_name: &str, dest_name: &str, r: &TypstToMd) -> String {
    format!(
        "# Rapport de conversion Typst → Markdown\n\n\
         - Source (INTACTE, non modifiée) : `{source_name}`\n\
         - Copie Markdown créée : `{dest_name}`\n\
         - Titres convertis (`=` → `#`) : {}\n\
         - Blocs Typst NON convertis (conservés dans des blocs balisés \
         `ENGRAM_TYPST_UNCONVERTED`) : {}\n\n\
         Seules les structures sûres sont converties ; le code Typst est \
         conservé tel quel dans la copie, à traiter manuellement. \
         L'original n'a pas été touché : les deux fichiers coexistent, \
         c'est à toi de décider lequel fait foi.\n",
        r.headings_converted, r.unconverted_blocks
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titres_typst_convertis_en_markdown() {
        let r = typst_to_md_safe("= Titre\n\nProse.\n\n== Sous-titre\n");
        assert_eq!(r.markdown, "# Titre\n\nProse.\n\n## Sous-titre\n");
        assert_eq!(r.headings_converted, 2);
        assert_eq!(r.unconverted_blocks, 0);
    }

    #[test]
    fn frontmatter_conserve_verbatim() {
        let src = "---\ntags: [a, b]\n---\n\n= Titre\n";
        let r = typst_to_md_safe(src);
        assert!(r.markdown.starts_with("---\ntags: [a, b]\n---\n"));
        assert!(r.markdown.contains("# Titre"));
    }

    #[test]
    fn code_typst_conserve_dans_bloc_balise_jamais_traduit() {
        let src = "Prose avant.\n#import \"module.typ\"\n#let x = 1\nProse après.\n";
        let r = typst_to_md_safe(src);
        assert_eq!(r.unconverted_blocks, 1, "lignes consécutives = un bloc");
        assert!(r.markdown.contains(UNCONVERTED_BEGIN));
        assert!(r
            .markdown
            .contains("```typst\n#import \"module.typ\"\n#let x = 1\n```"));
        assert!(r.markdown.contains(UNCONVERTED_END));
        assert!(r.markdown.contains("Prose avant.\n"));
        assert!(r.markdown.contains("Prose après.\n"));
    }

    #[test]
    fn prose_et_listes_verbatim() {
        let src = "Paragraphe *simple*.\n\n- item un\n- item deux\n";
        let r = typst_to_md_safe(src);
        assert_eq!(r.markdown, src, "aucune invention sur la prose");
        assert_eq!(r.headings_converted, 0);
        assert_eq!(r.unconverted_blocks, 0);
    }

    #[test]
    fn deux_groupes_de_code_deux_blocs() {
        let src = "#let a = 1\nProse.\n#show heading\n";
        let r = typst_to_md_safe(src);
        assert_eq!(r.unconverted_blocks, 2);
    }

    #[test]
    fn ligne_diese_espace_nest_pas_du_code_typst() {
        // `# Titre` (dièse + espace) n'est PAS une directive typst — en
        // typst le code est `#identifiant`. On la laisse verbatim (c'est
        // déjà du Markdown valide).
        let r = typst_to_md_safe("# Déjà markdown\n");
        assert_eq!(r.markdown, "# Déjà markdown\n");
        assert_eq!(r.unconverted_blocks, 0);
    }

    #[test]
    fn rapport_contient_les_comptes_et_les_noms() {
        let r = typst_to_md_safe("= T\n#let x = 1\n");
        let rep = conversion_report("a.typst", "a.md", &r);
        assert!(rep.contains("`a.typst`"));
        assert!(rep.contains("`a.md`"));
        assert!(rep.contains(": 1"));
    }
}
