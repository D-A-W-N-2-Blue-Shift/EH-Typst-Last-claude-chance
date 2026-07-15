// ============================================================================
// modules/articles/src/lib.rs — Point d'entrée du module Articles
//
// Ce que je fais : éditeur de prose pour les articles (doc §5.5). Contrairement
// au journal, pas de concept "aujourd'hui" — une liste d'articles existants
// (04_articles/*.md — les .typst hérités restent ouvrables tels quels,
// indexés dans la table `articles`) + création d'un
// nouvel article depuis un titre (slug généré, désambiguïsé si collision,
// fichier + frontmatter créés, doc §3/§5.5). Frontmatter à 5 champs édité par
// formulaire (titre/statut/tags/date_cible/destination), corps en zone de
// texte libre. Stats simples (doc §5.5, « pas de stats orientées roman ») :
// nombre de mots + temps de lecture estimé, jamais stockés (dérivés à
// l'affichage depuis le corps). Le corps est aussi indexé dans fts_content
// à chaque sauvegarde (doc §5.1/§8 : recherche plein-texte depuis le hub).
//
// Comment je marche : OwnViewport, fenêtre à la demande. J'apprends la
// racine du projet actif via CoreEvent::ProjectRootUpdated (même mécanisme
// que journal/health/todo/dashboard) et j'ouvre ma PROPRE connexion à
// nexus.db.
//
// Écart honnête au doc (§5.5 « réutilise le moteur éditeur de Hive quasi
// intégralement ») : IDENTIQUE à l'écart déjà investigué et documenté pour
// journal (modules/journal/README_MODULE.md « Écart documenté ») —
// editor::EditorWindow a un constructeur privé et des champs privés,
// structurellement inconstructible depuis un autre crate. Fait déjà établi
// par lecture directe du source à l'incrément 4, pas re-vérifié ici (même
// moteur, même mur). Zone de texte = egui::TextEdit::multiline.
//
// Comment me virer : supprimer modules/articles/ + retirer "articles" du
// registre du binaire nexus (app_nexus/src/main.rs) + la dépendance de
// app_nexus/Cargo.toml + le membre du Cargo.toml du workspace.
//
// Mes dépendances : engram_core (trait Module + atomic_write), nexus_db
// (couche de données), chrono (horodatage `updated_at`).
// ============================================================================

mod entry;

use std::path::PathBuf;

use engram_core::{CoreContext, CoreEvent, Module, ModuleResponse};

struct UserError {
    user_message: String,
    #[allow(dead_code)]
    technical: String,
}

pub struct ArticlesModule {
    closed: bool,
    project_root: Option<PathBuf>,
    db: Option<nexus_db::Connection>,
    current_path: Option<PathBuf>,
    content: String,
    meta: entry::ArticleMeta,
    tags_input: String,
    loaded: bool,
    new_title_input: String,
    glados: Vec<UserError>,
    pending: Vec<ModuleResponse>,
}

impl Default for ArticlesModule {
    fn default() -> Self {
        Self {
            closed: true,
            project_root: None,
            db: None,
            current_path: None,
            content: String::new(),
            meta: entry::ArticleMeta::default(),
            tags_input: String::new(),
            loaded: false,
            new_title_input: String::new(),
            glados: Vec::new(),
            pending: Vec::new(),
        }
    }
}

impl ArticlesModule {
    fn glados(&mut self, user_message: impl Into<String>, technical: impl Into<String>) {
        let user_message = user_message.into();
        let technical = technical.into();
        tracing::error!(target: "articles", "{user_message} ({technical})");
        self.pending.push(ModuleResponse::Error {
            module: "articles".into(),
            message: user_message.clone(),
        });
        self.glados.push(UserError {
            user_message,
            technical,
        });
        if self.glados.len() > 4 {
            self.glados.remove(0);
        }
    }

    fn set_project_root(&mut self, root: Option<PathBuf>) {
        self.db = None;
        self.loaded = false;
        self.current_path = None;
        self.project_root = root.clone();
        let Some(root) = root else { return };
        let db_path = root.join(".engram").join("nexus.db");
        match nexus_db::open_db(&db_path) {
            Ok(c) => self.db = Some(c),
            Err(e) => self.glados(
                format!(
                    "Impossible d'ouvrir la base de données ({}).",
                    db_path.display()
                ),
                e.to_string(),
            ),
        }
    }

    /// Sauve le fichier courant (atomique) et synchronise `articles`
    /// (titre/statut/tags/date_cible/destination/word_count/updated_at).
    /// No-op silencieux si rien n'est chargé (rien à sauver).
    fn save_current(&mut self, db: &nexus_db::Connection) {
        let Some(path) = self.current_path.clone() else {
            return;
        };
        self.meta.tags = split_tags_input(&self.tags_input);
        let full = entry::with_meta(&self.content, &self.meta);
        if let Err(e) = engram_core::atomic_write(&path, full.as_bytes()) {
            self.glados(format!("Impossible d'enregistrer {}.", path.display()), e);
            return;
        }
        self.content = full;
        let (_, body) = entry::split_frontmatter(&self.content);
        let word_count = entry::word_count(body);
        let file_path = self
            .project_root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        let record = nexus_db::Article {
            id: nexus_db::new_id(),
            file_path,
            titre: self.meta.titre.clone(),
            statut: self.meta.statut.clone(),
            tags: self.meta.tags.clone(),
            date_cible: self.meta.date_cible.clone(),
            destination: self.meta.destination.clone(),
            word_count,
            updated_at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        };
        // Les deux appels DB sont faits AVANT toute mutation de `self`
        // (glados) : `body` emprunte `self.content` et est utilisé par
        // `fts_upsert`, donc `&mut self` ne peut intervenir qu'après sa
        // dernière utilisation (NLL) — même contrainte que journal.
        let upsert_result = nexus_db::upsert_article(db, &record);
        let fts_result = nexus_db::fts_upsert(db, &record.file_path, body);
        if let Err(e) = upsert_result {
            self.glados("Impossible de synchroniser l'article.", e.to_string());
        }
        if let Err(e) = fts_result {
            self.glados(
                "Impossible d'indexer l'article pour la recherche.",
                e.to_string(),
            );
        }
    }

    /// Charge un article existant depuis son `file_path` (relatif à la
    /// racine, tel que stocké dans `articles.file_path`). Sauve d'abord
    /// l'article en cours s'il diffère (jamais de perte silencieuse).
    fn open_existing(
        &mut self,
        db: &nexus_db::Connection,
        root: &std::path::Path,
        file_path: &str,
    ) {
        if self.loaded {
            self.save_current(db);
        }
        let path = root.join(file_path);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.meta = entry::parse_meta(&content);
                self.tags_input = self.meta.tags.join(", ");
                self.content = content;
                self.current_path = Some(path);
                self.loaded = true;
            }
            Err(e) => self.glados(
                format!("Impossible de lire {}.", path.display()),
                e.to_string(),
            ),
        }
    }

    /// Crée un nouvel article depuis `new_title_input` : slug dérivé du
    /// titre, désambiguïsé par suffixe numérique si le fichier existe déjà
    /// (jamais d'écrasement silencieux d'un article existant).
    fn create_new(&mut self, db: &nexus_db::Connection, root: &std::path::Path) {
        let titre = self.new_title_input.trim().to_string();
        if titre.is_empty() {
            self.glados("Donne un titre à l'article.", "new_title_input vide");
            return;
        }
        // Même garde que open_existing : ne jamais perdre silencieusement
        // les modifications non sauvées de l'article en cours en basculant
        // vers un nouvel article (défense en profondeur — inatteignable
        // depuis l'UI actuelle, qui sauve déjà via « ← Liste », mais un
        // futur appelant ne doit pas pouvoir perdre des données).
        if self.loaded {
            self.save_current(db);
        }
        let base_slug = entry::slugify(&titre);
        let mut slug = base_slug.clone();
        let mut n = 2;
        while entry::path_for(root, &slug).exists() || entry::legacy_path_for(root, &slug).exists()
        {
            slug = format!("{base_slug}_{n}");
            n += 1;
        }
        let path = entry::path_for(root, &slug);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.glados(
                    format!("Impossible de créer {}.", parent.display()),
                    e.to_string(),
                );
                return;
            }
        }
        let template = entry::default_template(&titre);
        if let Err(e) = engram_core::atomic_write(&path, template.as_bytes()) {
            self.glados(format!("Impossible de créer {}.", path.display()), e);
            return;
        }
        self.meta = entry::parse_meta(&template);
        self.tags_input.clear();
        self.content = template;
        self.current_path = Some(path);
        self.loaded = true;
        self.new_title_input.clear();
        self.save_current(db);
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        if !self.glados.is_empty() {
            let mut dismiss = None;
            for (i, err) in self.glados.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(255, 46, 136), "⚠");
                    ui.label(err.user_message.as_str());
                    if ui.small_button("✕").clicked() {
                        dismiss = Some(i);
                    }
                });
            }
            if let Some(i) = dismiss {
                self.glados.remove(i);
            }
            ui.separator();
        }

        if self.project_root.is_none() {
            ui.heading("Articles");
            ui.add_space(6.0);
            ui.weak("Aucun projet ouvert. Ouvre un projet depuis le hub Nexus d'abord.");
            return;
        }
        let (Some(db), Some(root)) = (self.db.take(), self.project_root.clone()) else {
            ui.heading("Articles");
            ui.add_space(6.0);
            ui.weak("Base de données indisponible.");
            return;
        };

        if !self.loaded {
            self.draw_list(ui, &db, &root);
        } else {
            self.draw_editor(ui, &db);
        }

        self.db = Some(db);
    }

    fn draw_list(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection, root: &std::path::Path) {
        ui.heading("Articles");
        ui.horizontal(|ui| {
            ui.label("Nouvel article — titre :");
            ui.text_edit_singleline(&mut self.new_title_input);
            if ui.button("+ Créer").clicked() {
                self.create_new(db, root);
            }
        });
        ui.separator();
        match nexus_db::list_articles(db) {
            Ok(articles) => {
                if articles.is_empty() {
                    ui.weak("Aucun article pour l'instant.");
                }
                let mut to_open = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for a in &articles {
                        ui.horizontal(|ui| {
                            if ui.button("Ouvrir").clicked() {
                                to_open = Some(a.file_path.clone());
                            }
                            ui.strong(if a.titre.is_empty() {
                                "(sans titre)"
                            } else {
                                a.titre.as_str()
                            });
                            ui.weak(format!("· {} · {} mot(s)", a.statut, a.word_count));
                        });
                    }
                });
                if let Some(fp) = to_open {
                    self.open_existing(db, root, &fp);
                }
            }
            Err(e) => {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 46, 136),
                    format!("Lecture impossible : {e}"),
                );
            }
        }
    }

    fn current_is_legacy_typst(&self) -> bool {
        self.current_path
            .as_deref()
            .and_then(|p| p.extension())
            .is_some_and(|e| e == "typst")
    }

    /// Brief Phase 6 — copie Markdown explicite d'un article Typst hérité.
    /// Convertit le buffer courant, écrit `<slug>.md` + le rapport, bascule
    /// l'édition sur la copie. L'original `.typst` n'est jamais réécrit.
    fn create_md_copy(&mut self, db: &nexus_db::Connection) {
        let Some(src) = self.current_path.clone() else {
            return;
        };
        let dst = src.with_extension("md");
        if dst.exists() {
            self.glados(
                format!("{} existe déjà — copie refusée.", dst.display()),
                "pas d'écrasement de copie existante",
            );
            return;
        }
        let out = engram_core::typst_fallback::typst_to_md_safe(&self.content);
        if let Err(e) = engram_core::atomic_write(&dst, out.markdown.as_bytes()) {
            self.glados(format!("Impossible de créer {}.", dst.display()), e);
            return;
        }
        let report_path = src.with_extension("typ-to-md-report.md");
        let report = engram_core::typst_fallback::conversion_report(
            &src.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            &dst.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            &out,
        );
        if let Err(e) = engram_core::atomic_write(&report_path, report.as_bytes()) {
            self.glados(
                format!(
                    "Copie créée mais rapport impossible ({}).",
                    report_path.display()
                ),
                e,
            );
        }
        self.content = out.markdown;
        self.current_path = Some(dst);
        self.save_current(db);
    }

    fn draw_editor(&mut self, ui: &mut egui::Ui, db: &nexus_db::Connection) {
        let mut back_to_list = false;
        ui.horizontal(|ui| {
            if ui.button("← Liste").clicked() {
                back_to_list = true;
            }
            ui.weak(
                self.current_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            );
        });
        if self.current_is_legacy_typst() {
            ui.horizontal_wrapped(|ui| {
                ui.weak("Typst — secondaire.");
                if ui
                    .button("Créer une copie Markdown")
                    .on_hover_text(
                        "Crée une copie .md convertie (structures sûres uniquement) + \
                         un rapport. L'original .typst n'est JAMAIS modifié.",
                    )
                    .clicked()
                {
                    self.create_md_copy(db);
                }
            });
        }
        if back_to_list {
            self.save_current(db);
            self.loaded = false;
            self.current_path = None;
            return;
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Titre :");
            ui.text_edit_singleline(&mut self.meta.titre);
            ui.label("Statut :");
            egui::ComboBox::from_id_salt("articles_statut")
                .selected_text(self.meta.statut.as_str())
                .show_ui(ui, |ui| {
                    for s in entry::STATUTS {
                        ui.selectable_value(&mut self.meta.statut, (*s).to_string(), *s);
                    }
                });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Tags (séparés par virgule) :");
            ui.text_edit_singleline(&mut self.tags_input);
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Date cible (YYYY-MM-DD) :");
            ui.text_edit_singleline(&mut self.meta.date_cible);
            ui.label("Destination :");
            ui.text_edit_singleline(&mut self.meta.destination);
        });
        let (_, body) = entry::split_frontmatter(&self.content);
        let word_count = entry::word_count(body);
        ui.weak(format!(
            "{} mot(s) · ~{} min de lecture",
            word_count,
            entry::reading_time_minutes(word_count)
        ));
        ui.add_space(4.0);
        let response = ui.add(
            egui::TextEdit::multiline(&mut self.content)
                .desired_width(f32::INFINITY)
                .desired_rows(24),
        );
        ui.horizontal(|ui| {
            if response.lost_focus() || ui.button("💾 Enregistrer").clicked() {
                self.save_current(db);
            }
        });
    }
}

fn split_tags_input(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl Module for ArticlesModule {
    fn name(&self) -> &'static str {
        "articles"
    }

    fn init(&mut self, _ctx: &CoreContext) -> Result<(), String> {
        Ok(())
    }

    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>) {
        if self.closed {
            out.append(&mut self.pending);
            return;
        }
        let viewport_id = egui::ViewportId::from_hash_of("articles");
        let builder = egui::ViewportBuilder::default()
            .with_title("Hive-RBMK-mod-Tcherenkov — Articles")
            .with_inner_size([780.0, 640.0]);
        let mut open_palette = false;
        let mut close_requested = false;
        egui_ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                close_requested = true;
                return;
            }
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                    egui::Key::P,
                )
            }) {
                open_palette = true;
            }
            egui::CentralPanel::default().show(ctx, |ui| self.draw(ui));
        });
        if close_requested {
            if let Some(db) = self.db.take() {
                self.save_current(&db);
                self.db = Some(db);
            }
            self.closed = true;
        }
        if open_palette {
            out.push(ModuleResponse::OpenPaletteRequested);
        }
        out.append(&mut self.pending);
    }

    fn active_viewport_count(&self) -> usize {
        if self.closed {
            0
        } else {
            1
        }
    }

    fn handle_event(&mut self, event: &CoreEvent) {
        match event {
            CoreEvent::ProjectRootUpdated(root) => {
                self.set_project_root(root.clone());
            }
            CoreEvent::OpenModuleWindowRequested(name) if name == self.name() => {
                self.closed = false;
            }
            CoreEvent::ToggleModuleWindowRequested(name) if name == self.name() => {
                self.closed = !self.closed;
            }
            _ => {}
        }
    }

    fn shutdown(&mut self) {
        if let Some(db) = self.db.take() {
            self.save_current(&db);
        }
        self.db = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_new_puis_liste_le_montre() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("04_articles")).expect("mkdir");
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = ArticlesModule::default();
        m.new_title_input = "Le Burnout Autistique".to_string();
        m.create_new(&db, root);
        assert!(m.loaded);
        assert_eq!(m.meta.titre, "Le Burnout Autistique");
        let articles = nexus_db::list_articles(&db).expect("list");
        assert_eq!(articles.len(), 1);
        assert!(articles[0].file_path.ends_with("le_burnout_autistique.md"));
    }

    #[test]
    fn create_new_desambiguise_les_slugs_identiques() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("04_articles")).expect("mkdir");
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = ArticlesModule::default();
        m.new_title_input = "Repos".to_string();
        m.create_new(&db, root);
        let first_path = m.current_path.clone();
        m.loaded = false;
        m.current_path = None;
        m.new_title_input = "Repos".to_string();
        m.create_new(&db, root);
        assert_ne!(first_path, m.current_path);
        assert_eq!(nexus_db::list_articles(&db).expect("list").len(), 2);
    }

    #[test]
    fn copie_markdown_explicite_preserve_larticle_typst_herite() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("04_articles")).expect("mkdir");
        let legacy = root.join("04_articles").join("ancien.typst");
        let original =
            "---\ntitre: \"Ancien\"\nstatut: brouillon\ntags: []\ndate_cible: \"\"\ndestination: \"\"\n---\n\n= Ancien\n\nProse héritée.\n";
        std::fs::write(&legacy, original).expect("write");
        let db = nexus_db::open_in_memory().expect("db");
        // L'article hérité a sa ligne d'index (comme tout article réellement
        // sauvé avant le fallback).
        nexus_db::upsert_article(
            &db,
            &nexus_db::Article {
                id: nexus_db::new_id(),
                file_path: "04_articles/ancien.typst".into(),
                titre: "Ancien".into(),
                statut: "brouillon".into(),
                tags: Vec::new(),
                date_cible: String::new(),
                destination: String::new(),
                word_count: 2,
                updated_at: "2026-07-01T00:00:00".into(),
            },
        )
        .expect("seed");
        let mut m = ArticlesModule {
            project_root: Some(root.to_path_buf()),
            ..ArticlesModule::default()
        };
        m.open_existing(&db, root, "04_articles/ancien.typst");
        m.create_md_copy(&db);
        // Original intact à l'octet près.
        assert_eq!(std::fs::read_to_string(&legacy).expect("read"), original);
        // Copie convertie + rapport + bascule d'édition.
        let md = std::fs::read_to_string(root.join("04_articles/ancien.md")).expect("copie");
        assert!(md.contains("# Ancien"));
        assert!(legacy.with_extension("typ-to-md-report.md").is_file());
        assert!(m
            .current_path
            .as_ref()
            .expect("chemin")
            .ends_with("ancien.md"));
        // Les DEUX articles coexistent dans l'index (l'utilisateur décide).
        assert_eq!(nexus_db::list_articles(&db).expect("list").len(), 2);
    }

    #[test]
    fn create_new_ne_prend_jamais_le_nom_dun_typst_herite() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("04_articles")).expect("mkdir");
        // Article Typst hérité d'avant le fallback.
        let legacy = root.join("04_articles").join("repos.typst");
        std::fs::write(&legacy, "= Repos\n\nContenu hérité.").expect("write");
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = ArticlesModule::default();
        m.new_title_input = "Repos".to_string();
        m.create_new(&db, root);
        // Le nouveau .md est désambiguïsé, l'hérité est INTACT.
        assert!(m
            .current_path
            .as_ref()
            .expect("créé")
            .ends_with("repos_2.md"));
        assert_eq!(
            std::fs::read_to_string(&legacy).expect("read"),
            "= Repos\n\nContenu hérité."
        );
    }

    #[test]
    fn save_current_synchronise_word_count() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("04_articles")).expect("mkdir");
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = ArticlesModule::default();
        m.new_title_input = "Test".to_string();
        m.create_new(&db, root);
        // Corps remplacé explicitement (plutôt qu'ajouté au template par
        // défaut, dont la ligne de titre "# Test" contribue elle-même 2
        // tokens au comptage — même comportement que journal, non testé
        // comme un cas séparé là-bas ; ici le corps est contrôlé pour que
        // l'assertion soit sans ambiguïté).
        m.content = "---\ntitre: \"Test\"\nstatut: brouillon\ntags: []\ndate_cible: \"\"\ndestination: \"\"\n---\n\nun deux trois quatre"
            .to_string();
        m.save_current(&db);
        let articles = nexus_db::list_articles(&db).expect("list");
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].word_count, 4);
    }

    #[test]
    fn save_current_indexe_le_corps_pour_la_recherche_plein_texte() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("04_articles")).expect("mkdir");
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = ArticlesModule::default();
        m.new_title_input = "Brouillard cognitif".to_string();
        m.create_new(&db, root);
        m.content = "---\ntitre: \"Brouillard cognitif\"\nstatut: brouillon\ntags: []\ndate_cible: \"\"\ndestination: \"\"\n---\n\nUn paragraphe qui parle de brouillard cognitif persistant."
            .to_string();
        m.save_current(&db);
        let hits = nexus_db::fts_search(&db, "brouillard", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].file_path.ends_with("brouillard_cognitif.md"));
        // Le frontmatter n'est PAS indexé, seul le corps l'est : "statut"
        // n'apparaît que dans le frontmatter de cet article.
        assert!(nexus_db::fts_search(&db, "brouillon", 10)
            .expect("search")
            .is_empty());
    }

    #[test]
    fn open_existing_sauve_l_article_precedent_avant_de_changer() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("04_articles")).expect("mkdir");
        let db = nexus_db::open_in_memory().expect("db");
        let mut m = ArticlesModule::default();
        m.new_title_input = "Premier".to_string();
        m.create_new(&db, root);
        m.content.push_str(" contenu modifié");
        let first_file_path = nexus_db::list_articles(&db).expect("list")[0]
            .file_path
            .clone();

        m.new_title_input = "Second".to_string();
        m.create_new(&db, root);

        m.open_existing(&db, root, &first_file_path);
        assert!(m.content.contains("contenu modifié"));
    }
}
