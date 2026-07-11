// ============================================================================
// app/src/module_cli.rs — `engram_hive module add|remove <nom>`
//
// Scaffolding BUILD-TIME (l'enregistrement des modules est compile-time) :
// la CLI écrit/retire les fichiers source et les entrées de config, puis un
// `cargo build` est requis. Pas de chargement à chaud — c'est assumé et
// documenté (ARCHITECTURE.md, README_MODULE.md).
//
// `module add <nom>` :
//   - modules/<nom>/{Cargo.toml, src/lib.rs, README_MODULE.md}
//   - racine Cargo.toml : ajoute "modules/<nom>" aux members du workspace
//   - app/Cargo.toml : ajoute la dépendance vers le module
//   - app/src/main.rs : ajoute registry.register("<nom>", …)
//   - config/licorne-a-gerber.ron : ajoute une section "<nom>"
// `module remove <nom>` : inverse exact.
// ============================================================================

use std::path::{Path, PathBuf};

/// Modules livrés avec le cœur du projet : on refuse de les retirer par CLI
/// (trop destructeur — passe par la main si tu y tiens vraiment).
const BUILTIN: &[&str] = &["editor", "file_tree"];

/// Point d'entrée CLI. `args` = ce qui suit `engram_hive module`.
pub fn run(args: &[&str]) -> Result<String, String> {
    let root = workspace_root()?;
    match args {
        ["add", name, ..] => add(&root, name),
        ["remove", name, ..] => remove(&root, name),
        _ => Err("Usage : engram_hive module add|remove <nom>".to_string()),
    }
}

/// Remonte depuis le dossier courant (puis depuis l'exécutable) jusqu'au
/// Cargo.toml qui contient `[workspace]`.
fn workspace_root() -> Result<PathBuf, String> {
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            starts.push(d.to_path_buf());
        }
    }
    for start in starts {
        let mut cur = Some(start.as_path());
        while let Some(dir) = cur {
            let manifest = dir.join("Cargo.toml");
            if let Ok(c) = std::fs::read_to_string(&manifest) {
                if c.contains("[workspace]") {
                    return Ok(dir.to_path_buf());
                }
            }
            cur = dir.parent();
        }
    }
    Err(
        "Racine du workspace introuvable (lance la commande dans le dépôt \
         engram_hive)."
            .to_string(),
    )
}

fn validate_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Nom de module invalide : '{name}'. Attendu : snake_case ASCII \
             (a-z, 0-9, _), commençant par une lettre."
        ))
    }
}

/// snake_case → CamelCase pour le nom du type (ex: file_tree → FileTree).
fn camel(name: &str) -> String {
    name.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut cs = s.chars();
            match cs.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

pub fn add(root: &Path, name: &str) -> Result<String, String> {
    validate_name(name)?;
    let mod_dir = root.join("modules").join(name);
    if mod_dir.exists() {
        return Err(format!("modules/{name}/ existe déjà. Rien fait."));
    }
    let type_name = format!("{}Module", camel(name));

    // 1. Fichiers du module.
    std::fs::create_dir_all(mod_dir.join("src"))
        .map_err(|e| format!("création de modules/{name}/src ratée : {e}"))?;
    write(&mod_dir.join("Cargo.toml"), &cargo_toml_template(name))?;
    write(
        &mod_dir.join("src").join("lib.rs"),
        &lib_rs_template(name, &type_name),
    )?;
    write(&mod_dir.join("README_MODULE.md"), &readme_template(name))?;

    // 2. Cargo.toml racine : members du workspace.
    let root_manifest = root.join("Cargo.toml");
    let c = read(&root_manifest)?;
    let c = members_add(&c, name)?;
    write(&root_manifest, &c)?;

    // 3. app/Cargo.toml : dépendance.
    let app_manifest = root.join("app").join("Cargo.toml");
    let c = read(&app_manifest)?;
    let c = dep_add(&c, name)?;
    write(&app_manifest, &c)?;

    // 4. app/src/main.rs : enregistrement.
    let main_rs = root.join("app").join("src").join("main.rs");
    let c = read(&main_rs)?;
    let c = register_add(&c, name, &type_name)?;
    write(&main_rs, &c)?;

    // 5. config/licorne-a-gerber.ron : section (best effort).
    let licorne = root.join("config").join("licorne-a-gerber.ron");
    if let Ok(c) = read(&licorne) {
        if let Ok(c) = licorne_add(&c, name) {
            let _ = write(&licorne, &c);
        }
    }

    Ok(format!(
        "Module '{name}' créé (modules/{name}/, members, dépendance, \
         registry.register, section licorne). Lance maintenant `cargo build`."
    ))
}

pub fn remove(root: &Path, name: &str) -> Result<String, String> {
    validate_name(name)?;
    if BUILTIN.contains(&name) {
        return Err(format!(
            "'{name}' est un module livré avec le cœur — retrait par CLI refusé. \
             Supprime-le à la main si tu y tiens."
        ));
    }
    let mod_dir = root.join("modules").join(name);
    if !mod_dir.exists() {
        return Err(format!("modules/{name}/ n'existe pas. Rien à retirer."));
    }

    std::fs::remove_dir_all(&mod_dir)
        .map_err(|e| format!("suppression de modules/{name}/ ratée : {e}"))?;

    let root_manifest = root.join("Cargo.toml");
    let c = members_remove(&read(&root_manifest)?, name);
    write(&root_manifest, &c)?;

    let app_manifest = root.join("app").join("Cargo.toml");
    let c = dep_remove(&read(&app_manifest)?, name);
    write(&app_manifest, &c)?;

    let main_rs = root.join("app").join("src").join("main.rs");
    let c = register_remove(&read(&main_rs)?, name);
    write(&main_rs, &c)?;

    let licorne = root.join("config").join("licorne-a-gerber.ron");
    if let Ok(c) = read(&licorne) {
        let c = licorne_remove(&c, name);
        let _ = write(&licorne, &c);
    }

    Ok(format!(
        "Module '{name}' retiré (dossier, members, dépendance, registry, \
         section licorne). Lance `cargo build` pour valider."
    ))
}

// ---------------------------------------------------------------------------
// Édition des fichiers (chirurgie de chaînes, formatage préservé).
// ---------------------------------------------------------------------------

/// Ajoute `"modules/<name>"` au tableau `members = [ … ]` du Cargo.toml racine.
fn members_add(content: &str, name: &str) -> Result<String, String> {
    let entry = format!("\"modules/{name}\"");
    if content.contains(&entry) {
        return Ok(content.to_string());
    }
    let start = content
        .find("members")
        .and_then(|i| content[i..].find('[').map(|j| i + j + 1))
        .ok_or("tableau `members` introuvable dans Cargo.toml racine.")?;
    let end = content[start..]
        .find(']')
        .map(|j| start + j)
        .ok_or("`members` mal formé (pas de `]`).")?;
    let mut out = String::with_capacity(content.len() + entry.len() + 4);
    out.push_str(&content[..end]);
    let inner = content[start..end].trim_end();
    // Insère une virgule si nécessaire.
    if inner.is_empty() {
        out.push_str(&entry);
    } else {
        out.push_str(", ");
        out.push_str(&entry);
    }
    out.push_str(&content[end..]);
    Ok(out)
}

fn members_remove(content: &str, name: &str) -> String {
    let entry = format!("\"modules/{name}\"");
    // Retire l'entrée + une virgule adjacente.
    content
        .replace(&format!(", {entry}"), "")
        .replace(&format!("{entry}, "), "")
        .replace(&entry, "")
}

/// Ajoute la dépendance du module dans la section [dependencies] d'app.
fn dep_add(content: &str, name: &str) -> Result<String, String> {
    let line = format!("{name} = {{ path = \"../modules/{name}\" }}");
    if content.contains(&line) {
        return Ok(content.to_string());
    }
    // Insère juste après la ligne de l'éditeur (un module pair existant).
    let anchor = "editor = { path = \"../modules/editor\" }";
    if let Some(pos) = content.find(anchor) {
        let insert_at = pos + anchor.len();
        let mut out = String::with_capacity(content.len() + line.len() + 2);
        out.push_str(&content[..insert_at]);
        out.push('\n');
        out.push_str(&line);
        out.push_str(&content[insert_at..]);
        return Ok(out);
    }
    Err("ancre [dependencies] introuvable dans app/Cargo.toml.".to_string())
}

fn dep_remove(content: &str, name: &str) -> String {
    let line = format!("{name} = {{ path = \"../modules/{name}\" }}");
    content
        .lines()
        .filter(|l| l.trim() != line)
        .collect::<Vec<_>>()
        .join("\n")
        + if content.ends_with('\n') { "\n" } else { "" }
}

/// Ajoute `registry.register("<name>", || Box::new(<name>::<Type>::default()));`
/// après la dernière ligne d'enregistrement existante.
fn register_add(content: &str, name: &str, type_name: &str) -> Result<String, String> {
    let line =
        format!("    registry.register(\"{name}\", || Box::new({name}::{type_name}::default()));");
    if content.contains(&format!("registry.register(\"{name}\"")) {
        return Ok(content.to_string());
    }
    let anchor = content
        .rfind("registry.register(")
        .ok_or("aucune ligne registry.register dans main.rs.")?;
    let eol = content[anchor..]
        .find('\n')
        .map(|j| anchor + j)
        .ok_or("registry.register sans fin de ligne.")?;
    let mut out = String::with_capacity(content.len() + line.len() + 1);
    out.push_str(&content[..eol]);
    out.push('\n');
    out.push_str(&line);
    out.push_str(&content[eol..]);
    Ok(out)
}

fn register_remove(content: &str, name: &str) -> String {
    let needle = format!("registry.register(\"{name}\"");
    content
        .lines()
        .filter(|l| !l.contains(&needle))
        .collect::<Vec<_>>()
        .join("\n")
        + if content.ends_with('\n') { "\n" } else { "" }
}

/// Ajoute une section `"<name>": ( … )` avant le `}` final du modèle licorne.
fn licorne_add(content: &str, name: &str) -> Result<String, String> {
    if content.contains(&format!("\"{name}\":")) {
        return Ok(content.to_string());
    }
    let close = content
        .rfind('}')
        .ok_or("modèle licorne-a-gerber.ron mal formé (pas de `}`).")?;
    let section = format!(
        "    // Section générée pour le module '{name}'. À compléter.\n    \
         \"{name}\": (\n    ),\n",
    );
    let mut out = String::with_capacity(content.len() + section.len());
    out.push_str(&content[..close]);
    out.push_str(&section);
    out.push_str(&content[close..]);
    Ok(out)
}

/// Retire la section `"<name>": ( … ),` (best effort : section plate générée).
fn licorne_remove(content: &str, name: &str) -> String {
    let key = format!("\"{name}\":");
    let Some(key_pos) = content.find(&key) else {
        return content.to_string();
    };
    // Recule jusqu'au début de ligne (pour emporter l'indentation + le commentaire).
    let line_start = content[..key_pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // Avance jusqu'au `),` de fermeture de la section.
    let after = &content[key_pos..];
    let Some(end_rel) = after.find("),") else {
        return content.to_string();
    };
    let mut end = key_pos + end_rel + 2;
    // Emporte le saut de ligne suivant.
    if content[end..].starts_with('\n') {
        end += 1;
    }
    // Retire aussi une ligne de commentaire générée juste au-dessus.
    let mut start = line_start;
    if let Some(prev_nl) = content[..line_start.saturating_sub(1)].rfind('\n') {
        let prev_line = &content[prev_nl + 1..line_start];
        if prev_line.contains(&format!("module '{name}'")) {
            start = prev_nl + 1;
        }
    }
    let mut out = String::with_capacity(content.len());
    out.push_str(&content[..start]);
    out.push_str(&content[end..]);
    out
}

// ---------------------------------------------------------------------------
// IO + templates.
// ---------------------------------------------------------------------------

fn read(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("lecture de {} ratée : {e}", p.display()))
}

fn write(p: &Path, content: &str) -> Result<(), String> {
    std::fs::write(p, content).map_err(|e| format!("écriture de {} ratée : {e}", p.display()))
}

fn cargo_toml_template(name: &str) -> String {
    format!(
        "# Module '{name}' — généré par `engram_hive module add {name}`.\n\
         # Supprimer ce dossier + `engram_hive module remove {name}` = disparition propre.\n\
         [package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2021\"\n\n\
         [dependencies]\n\
         engram_core = {{ path = \"../../core\" }}\n\
         egui = {{ workspace = true }}\n"
    )
}

fn lib_rs_template(name: &str, type_name: &str) -> String {
    format!(
        "// Module '{name}'. Voir README_MODULE.md pour le contrat (les 4 questions).\n\
         //\n\
         // Squelette minimal : implémente le trait Module. À toi de remplir\n\
         // init() (chargement de config via ctx.licorne.section::<...>(\"{name}\", …))\n\
         // et update() (dessine ton viewport egui, pousse des ModuleResponse).\n\n\
         use engram_core::{{CoreContext, Module, ModuleResponse}};\n\n\
         #[derive(Default)]\n\
         pub struct {type_name};\n\n\
         impl Module for {type_name} {{\n\
         \x20   fn name(&self) -> &'static str {{\n\
         \x20       \"{name}\"\n\
         \x20   }}\n\n\
         \x20   fn init(&mut self, _ctx: &CoreContext) -> Result<(), String> {{\n\
         \x20       Ok(())\n\
         \x20   }}\n\n\
         \x20   fn update(&mut self, _egui_ctx: &egui::Context, _out: &mut Vec<ModuleResponse>) {{\n\
         \x20   }}\n\
         }}\n"
    )
}

fn readme_template(name: &str) -> String {
    format!(
        "# Module `{name}`\n\n\
         Généré par `engram_hive module add {name}`.\n\n\
         ## Ce que je fais\n\n(À remplir.)\n\n\
         ## Comment je marche\n\n\
         J'implémente `trait Module` dans `src/lib.rs`. Ma config experte vit\n\
         dans la section `\"{name}\"` de `~/.config/engram_hive/licorne-a-gerber.ron`.\n\n\
         ## Comment me virer\n\n\
         `engram_hive module remove {name}` puis `cargo build`. (Ou : supprimer\n\
         ce dossier, retirer la ligne `registry.register(\"{name}\"…)` de\n\
         `app/src/main.rs`, la dépendance d'`app/Cargo.toml`, l'entrée des\n\
         `members` racine, et ma section licorne.)\n\n\
         ## Mes dépendances\n\n`engram_core`, `egui`.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_workspace() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        std::fs::create_dir_all(root.join("app").join("src"))?;
        std::fs::create_dir_all(root.join("config"))?;
        std::fs::create_dir_all(root.join("modules"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\nmembers = [\"core\", \"app\", \"modules/file_tree\", \"modules/editor\"]\n",
        )?;
        std::fs::write(
            root.join("app").join("Cargo.toml"),
            "[dependencies]\nengram_core = { path = \"../core\" }\nfile_tree = { path = \"../modules/file_tree\" }\neditor = { path = \"../modules/editor\" }\n",
        )?;
        std::fs::write(
            root.join("app").join("src").join("main.rs"),
            "    let mut registry = ModuleRegistry::new();\n    registry.register(\"file_tree\", || Box::new(file_tree::FileTreeModule::default()));\n    registry.register(\"editor\", || Box::new(editor::EditorModule::default()));\n",
        )?;
        std::fs::write(
            root.join("config").join("licorne-a-gerber.ron"),
            "{\n    \"editor\": EditorVomi(\n    ),\n}\n",
        )?;
        Ok(tmp)
    }

    #[test]
    fn add_then_remove_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = fake_workspace()?;
        let root = tmp.path();

        add(root, "wordcount")?;
        // Fichiers créés.
        assert!(root.join("modules/wordcount/Cargo.toml").exists());
        assert!(root.join("modules/wordcount/src/lib.rs").exists());
        assert!(root.join("modules/wordcount/README_MODULE.md").exists());
        // Entrées de config.
        let root_toml = std::fs::read_to_string(root.join("Cargo.toml"))?;
        assert!(root_toml.contains("\"modules/wordcount\""));
        let app_toml = std::fs::read_to_string(root.join("app/Cargo.toml"))?;
        assert!(app_toml.contains("wordcount = { path = \"../modules/wordcount\" }"));
        let main_rs = std::fs::read_to_string(root.join("app/src/main.rs"))?;
        assert!(main_rs.contains("registry.register(\"wordcount\""));
        assert!(main_rs.contains("wordcount::WordcountModule::default()"));
        let licorne = std::fs::read_to_string(root.join("config/licorne-a-gerber.ron"))?;
        assert!(licorne.contains("\"wordcount\":"));
        // La section licorne reste un map RON valide.
        let parsed: std::collections::HashMap<String, ron::Value> = ron::from_str(&licorne)?;
        assert!(parsed.contains_key("wordcount"));

        remove(root, "wordcount")?;
        assert!(!root.join("modules/wordcount").exists());
        let root_toml = std::fs::read_to_string(root.join("Cargo.toml"))?;
        assert!(!root_toml.contains("wordcount"));
        let app_toml = std::fs::read_to_string(root.join("app/Cargo.toml"))?;
        assert!(!app_toml.contains("wordcount"));
        let main_rs = std::fs::read_to_string(root.join("app/src/main.rs"))?;
        assert!(!main_rs.contains("wordcount"));
        let licorne = std::fs::read_to_string(root.join("config/licorne-a-gerber.ron"))?;
        assert!(!licorne.contains("wordcount"));
        // Toujours un RON valide après retrait.
        ron::from_str::<std::collections::HashMap<String, ron::Value>>(&licorne)?;
        Ok(())
    }

    #[test]
    fn refuses_builtin_and_bad_names() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = fake_workspace()?;
        assert!(remove(tmp.path(), "editor").is_err());
        assert!(add(tmp.path(), "Bad-Name").is_err());
        assert!(add(tmp.path(), "9oops").is_err());
        Ok(())
    }

    #[test]
    fn camel_case_conversion() {
        assert_eq!(camel("file_tree"), "FileTree");
        assert_eq!(camel("wordcount"), "Wordcount");
        assert_eq!(camel("a_b_c"), "ABC");
    }
}
