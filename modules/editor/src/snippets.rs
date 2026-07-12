// ============================================================================
// modules/editor/src/snippets.rs — Snippets déclenchés par préfixe
//
// Un fichier .toml par snippet dans ~/.config/engram_hive/snippets/.
// Au premier lancement, le dossier est créé avec les snippets par défaut :
// ;sce (scène), ;per (personnage), ;cha (chapitre), ;tab (table), ;date, ;dt.
//
// Expansion : dès que le token tapé (du dernier blanc au curseur) égale un
// trigger ET qu'aucun autre trigger ne le prolonge, le token est remplacé
// par le contenu — dans la MÊME transaction que la frappe (un seul Ctrl+Z).
//
// Placeholders dynamiques dans content : {{date}} et {{datetime}}.
// ============================================================================

use std::path::Path;

use engram_core::atomic_write;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Snippet {
    pub trigger: String,
    #[serde(default)]
    pub description: String,
    pub content: String,
}

#[derive(Default)]
pub struct SnippetSet {
    snippets: Vec<Snippet>,
}

impl SnippetSet {
    /// Charge ~/.config/engram_hive/snippets/*.toml (créés au premier
    /// lancement). Retourne (set, erreurs GLaDOS).
    pub fn load(config_dir: &Path) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let dir = config_dir.join("snippets");
        if !dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                errors.push(format!("Impossible de créer {} : {e}.", dir.display()));
                return (Self::default(), errors);
            }
            for (name, content) in DEFAULT_SNIPPETS {
                let p = dir.join(name);
                if let Err(e) = atomic_write(&p, content.as_bytes()) {
                    errors.push(format!("Impossible d'écrire {} : {e}.", p.display()));
                }
            }
        }
        let mut snippets = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Snippets illisibles ({}) : {e}.", dir.display()));
                return (Self::default(), errors);
            }
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_none_or(|e| e != "toml") {
                continue;
            }
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|raw| toml::from_str::<Snippet>(&raw).map_err(|e| e.to_string()))
            {
                Ok(s) => snippets.push(s),
                Err(e) => errors.push(format!(
                    "Snippet {} illisible : {e}. Il attendra que tu le répares.",
                    p.display()
                )),
            }
        }
        snippets.sort_by(|a, b| a.trigger.cmp(&b.trigger));
        (Self { snippets }, errors)
    }

    #[cfg(test)]
    pub fn from_snippets(snippets: Vec<Snippet>) -> Self {
        Self { snippets }
    }

    /// Le token se terminant au curseur correspond-il exactement à un
    /// trigger qu'aucun autre trigger ne prolonge ? Retourne
    /// (début du token en chars, contenu expansé).
    pub fn expansion_at(
        &self,
        rope: &ropey::Rope,
        cursor: usize,
        prefix: &str,
    ) -> Option<(usize, String)> {
        if self.snippets.is_empty() || prefix.is_empty() {
            return None;
        }
        let line_idx = rope.char_to_line(cursor.min(rope.len_chars()));
        let line_start = rope.line_to_char(line_idx);
        let before: String = rope.slice(line_start..cursor).to_string();
        // Token : du dernier blanc (ou début de ligne) au curseur.
        let token_start_byte = before
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let token = &before[token_start_byte..];
        if !token.starts_with(prefix) {
            return None;
        }
        let exact = self.snippets.iter().find(|s| s.trigger == token)?;
        let extendable = self
            .snippets
            .iter()
            .any(|s| s.trigger != token && s.trigger.starts_with(token));
        if extendable {
            return None; // ;s pourrait devenir ;sce : on attend la suite.
        }
        let token_chars = before[token_start_byte..].chars().count();
        Some((cursor - token_chars, render(&exact.content)))
    }

    pub fn len(&self) -> usize {
        self.snippets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snippets.is_empty()
    }
}

/// Placeholders dynamiques.
fn render(content: &str) -> String {
    let now = chrono::Local::now();
    content
        .replace("{{date}}", &now.format("%Y-%m-%d").to_string())
        .replace("{{datetime}}", &now.format("%Y-%m-%d %H:%M").to_string())
}

/// Les snippets livrés par défaut (un .toml par snippet, commentés).
const DEFAULT_SNIPPETS: &[(&str, &str)] = &[
    (
        "scene.toml",
        r#"# Snippet : template de scène Typst. Tape ;sce dans l'éditeur.
trigger = ";sce"
description = "Template de scène Typst"
content = """
/*
---
tags:
  - scene
chapitre:
personnages_presents:
  -
lieu:
moment:
pov:
---
*/

= [Titre de la scène]

"""
"#,
    ),
    (
        "personnage.toml",
        r#"# Snippet : fiche personnage Typst. Tape ;per dans l'éditeur.
trigger = ";per"
description = "Fiche personnage Typst"
content = """
/*
---
tags:
  - personnage
nom:
surnom:
age:
apparence:
motivation_principale:
faille:
liens:
  -
---
*/

= [Nom du personnage]

"""
"#,
    ),
    (
        "chapitre.toml",
        r#"# Snippet : en-tête de chapitre Typst. Tape ;cha dans l'éditeur.
trigger = ";cha"
description = "En-tête de chapitre Typst"
content = """
/*
---
tags:
  - chapitre
numero:
goal: 2000
---
*/

= Chapitre

"""
"#,
    ),
    (
        "table.toml",
        r#"# Snippet : tableau Typst. Tape ;tab dans l'éditeur.
trigger = ";tab"
description = "Tableau Typst simple"
content = """
#table(
  columns: 3,
  [Col 1], [Col 2], [Col 3],
  [ ], [ ], [ ],
)
"""
"#,
    ),
    (
        "date.toml",
        r#"# Snippet : la date du jour. Tape ;date dans l'éditeur.
trigger = ";date"
description = "Date du jour (AAAA-MM-JJ)"
content = "{{date}}"
"#,
    ),
    (
        "datetime.toml",
        r#"# Snippet : date et heure. Tape ;dt dans l'éditeur.
trigger = ";dt"
description = "Date et heure"
content = "{{datetime}}"
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> SnippetSet {
        SnippetSet::from_snippets(vec![
            Snippet {
                trigger: ";sce".into(),
                description: String::new(),
                content: "SCENE".into(),
            },
            Snippet {
                trigger: ";s".into(),
                description: String::new(),
                content: "COURT".into(),
            },
            Snippet {
                trigger: ";date".into(),
                description: String::new(),
                content: "{{date}}".into(),
            },
        ])
    }

    #[test]
    fn expands_exact_non_ambiguous() -> Result<(), Box<dyn std::error::Error>> {
        let rope = ropey::Rope::from_str("texte ;sce");
        let (start, content) = set()
            .expansion_at(&rope, 10, ";")
            .ok_or(";sce doit s'étendre")?;
        assert_eq!(start, 6);
        assert_eq!(content, "SCENE");
        Ok(())
    }

    #[test]
    fn waits_when_prefix_of_longer_trigger() {
        // ;s est trigger, mais ;sce le prolonge → on attend.
        let rope = ropey::Rope::from_str(";s");
        assert!(set().expansion_at(&rope, 2, ";").is_none());
    }

    #[test]
    fn date_placeholder_renders() -> Result<(), Box<dyn std::error::Error>> {
        let rope = ropey::Rope::from_str(";date");
        let (_, content) = set()
            .expansion_at(&rope, 5, ";")
            .ok_or(";date doit s'étendre")?;
        assert_eq!(content.len(), 10); // AAAA-MM-JJ
        assert!(content.contains('-'));
        Ok(())
    }

    #[test]
    fn default_snippets_parse() {
        for (name, raw) in DEFAULT_SNIPPETS {
            let s: Result<Snippet, _> = toml::from_str(raw);
            assert!(s.is_ok(), "snippet par défaut {name} cassé : {:?}", s.err());
        }
    }
}
