// ============================================================================
// modules/file_tree/src/stats.rs — Statistiques et modèle d'arborescence
//
// Construit le modèle d'arbre affiché par tree_view.rs :
//   - parcours du filesystem (en sautant .engram/ et les exclusions)
//   - agrégation des comptages de mots par dossier (depuis le snapshot indexeur)
//   - mapping statut YAML → icône (⚪ 🔵 🟡 🟢 🔴)
//   - formatage des nombres (287432 → "287k")
//
// Règle critique : 05_texte/ est le seul "poids littéraire". 06_en_cours/
// est un miroir (symlinks) et ne compte JAMAIS dans les agrégats.
// ============================================================================

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::indexer::FileStats;

/// Statut éditorial d'un fichier, lu dans le champ `statut:` du YAML.
/// Valeur inconnue ou absente = pas d'icône, pas d'erreur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Brouillon, // ⚪ premier jet
    EnCours,   // 🔵 en écriture
    ARelire,   // 🟡 à réviser
    Ready,     // 🟢 terminé
    Bloque,    // 🔴 problème
}

impl Status {
    /// Mapping case-insensitive validé par Steve.
    pub fn from_yaml(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "brouillon" | "draft" => Some(Self::Brouillon),
            "en_cours" | "wip" => Some(Self::EnCours),
            "a_relire" | "review" => Some(Self::ARelire),
            "ready" | "final" => Some(Self::Ready),
            "bloque" | "blocked" => Some(Self::Bloque),
            _ => None,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Brouillon => "⚪",
            Self::EnCours => "🔵",
            Self::ARelire => "🟡",
            Self::Ready => "🟢",
            Self::Bloque => "🔴",
        }
    }
}

/// Un nœud de l'arbre affiché. Le modèle est reconstruit quand l'indexeur
/// publie une nouvelle génération ou après une action utilisateur.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub children: Vec<TreeNode>,
    /// Mots (corps Typst) agrégés. Pour un fichier : son propre compte.
    pub words: u64,
    pub chars: u64,
    /// Valeur de goal: dans le YAML (fichiers) — 0 si absent.
    pub goal: u64,
    pub status: Option<Status>,
    /// Vrai pour 06_en_cours/ : exclu des agrégats, icônes 🔗.
    pub is_en_cours: bool,
}

/// Totaux projet affichés à la racine du tree.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectTotals {
    /// Total tous dossiers (hors 06_en_cours).
    pub total_words: u64,
    /// Mots du manuscrit réel (05_texte uniquement).
    pub manuscript_words: u64,
}

/// Construit l'arbre complet depuis le filesystem + le snapshot de stats.
pub fn build_tree(
    root: &Path,
    files: &HashMap<PathBuf, FileStats>,
    cfg: &Config,
) -> (TreeNode, ProjectTotals) {
    let mut node = build_node(root, files, cfg, false);
    node.name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.display().to_string());

    let mut totals = ProjectTotals::default();
    for child in &node.children {
        if child.is_en_cours {
            continue;
        }
        totals.total_words += child.words;
        // Manuscrit réel : 6_chapitres (structure Wingate) ou 05_texte
        // (projets historiques). Les scènes de travail (5_scenes) et l'atelier
        // ne sont pas comptés comme manuscrit final.
        if child.name.starts_with("6_chapitres") || child.name.starts_with("05_texte") {
            totals.manuscript_words += child.words;
        }
    }
    node.words = totals.total_words;
    (node, totals)
}

fn build_node(
    path: &Path,
    files: &HashMap<PathBuf, FileStats>,
    cfg: &Config,
    inside_en_cours: bool,
) -> TreeNode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let meta = std::fs::symlink_metadata(path).ok();
    let is_symlink = meta
        .as_ref()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    // §6 — reconnaît l'ancien "06_en_cours" et le nouveau "06_session".
    let is_en_cours = crate::symlinks::is_session_folder(&name);

    // Note : un symlink vers un dossier dans en_cours/ est traité comme une
    // feuille (on ne descend pas dedans, l'original est déjà dans l'arbre).
    let mut node = TreeNode {
        name: name.clone(),
        path: path.to_path_buf(),
        is_dir: path.is_dir() && !(inside_en_cours && is_symlink),
        is_symlink,
        children: Vec::new(),
        words: 0,
        chars: 0,
        goal: 0,
        status: None,
        is_en_cours,
    };

    if node.is_dir {
        let mut entries: Vec<PathBuf> = match std::fs::read_dir(path) {
            Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
            Err(_) => Vec::new(),
        };
        // Tri : dossiers d'abord, puis alphabétique (insensible à la casse).
        entries.sort_by(|a, b| {
            let (da, db) = (a.is_dir(), b.is_dir());
            db.cmp(&da).then_with(|| {
                a.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .cmp(&b.file_name().map(|n| n.to_string_lossy().to_lowercase()))
            })
        });
        for entry in entries {
            let fname = entry
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // .engram/ n'apparaît JAMAIS dans le tree. Les fichiers cachés
            // (.context inclus) non plus. Exclusions vomi respectées.
            if fname.starts_with('.') || cfg.vomi.exclude_dirs.contains(&fname) {
                continue;
            }
            let child = build_node(&entry, files, cfg, inside_en_cours || is_en_cours);
            // Agrégation : 06_en_cours est un miroir, jamais compté.
            if !(child.is_en_cours || inside_en_cours || is_en_cours) {
                node.words += child.words;
                node.chars += child.chars;
            }
            node.children.push(child);
        }
    } else {
        // Feuille : stats depuis le snapshot (clé = chemin canonique).
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(st) = files.get(&canonical) {
            node.words = st.words_body;
            node.chars = st.chars;
            node.goal = st.goal;
            node.status = st.status;
        }
    }
    node
}

/// Une entrée est indexable si extension .typ ou listée dans extra_extensions.
pub fn is_indexable(path: &Path, cfg: &Config) -> bool {
    let ext = match path.extension() {
        Some(e) => e.to_string_lossy().to_lowercase(),
        None => return false,
    };
    ext == "typ"
        || cfg
            .vomi
            .extra_extensions
            .iter()
            .any(|x| x.trim_start_matches('.') == ext)
}

/// Formate un comptage selon la config (abréviation "287k", unité words/chars).
pub fn format_count(node_words: u64, node_chars: u64, cfg: &Config) -> String {
    let n = if cfg.vomi.stats_unit == "chars" {
        node_chars
    } else {
        node_words
    };
    format_number(n, cfg)
}

pub fn format_number(n: u64, cfg: &Config) -> String {
    if cfg.vomi.abbreviate_counts && n >= cfg.vomi.abbreviate_threshold.max(1000) {
        format!("{}k", n / 1000)
    } else {
        // Séparateur de milliers à l'espace fine (lisibilité Steve).
        let s = n.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i).is_multiple_of(3) {
                out.push(' ');
            }
            out.push(c);
        }
        out
    }
}

/// Section sémantique d'un chemin relatif au projet (pour la colonne
/// `section` de la table files). 06_en_cours et .engram sont ignorés en amont.
pub fn section_of(rel: &Path) -> String {
    let first = rel
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_default();
    match first.as_str() {
        // Structure « Wingate » (2026-07-04).
        s if s.starts_with("1_atelier") => "atelier".into(),
        s if s.starts_with("2_todo") => "todo".into(),
        s if s.starts_with("3_plan") => "plan".into(),
        s if s.starts_with("4_fiches") => "fiches".into(),
        s if s.starts_with("5_scenes") => "scenes".into(),
        s if s.starts_with("6_chapitres") => "chapitres".into(),
        s if s.starts_with("7_notes") => "notes".into(),
        s if s.starts_with("8_archives") => "archives".into(),
        // Projets historiques (rétro-compatibilité, jamais de régression).
        s if s.starts_with("01_architecture") => "architecture".into(),
        s if s.starts_with("02_intendance_micro") => "intendance_micro".into(),
        s if s.starts_with("03_intendance_macro") => "intendance_macro".into(),
        s if s.starts_with("04_notes") => "notes".into(),
        s if s.starts_with("05_texte") => "texte".into(),
        _ => "autre".into(),
    }
}

// ---------------------------------------------------------------------------
// Tests unitaires : formatage et sections.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abreviation_des_grands_nombres() {
        let cfg = Config::default();
        assert_eq!(format_number(287_432, &cfg), "287k");
        assert_eq!(format_number(850, &cfg), "850");
    }

    #[test]
    fn separateur_de_milliers_sans_abreviation() {
        let mut cfg = Config::default();
        cfg.vomi.abbreviate_counts = false;
        assert_eq!(format_number(31_847, &cfg), "31 847");
    }

    #[test]
    fn sections_par_prefixe() {
        // Structure Wingate.
        assert_eq!(
            section_of(Path::new("1_atelier/scene_active.typ")),
            "atelier"
        );
        assert_eq!(
            section_of(Path::new("4_fiches/personnages/x.typ")),
            "fiches"
        );
        assert_eq!(
            section_of(Path::new("6_chapitres/manuscrit_full/c1.typ")),
            "chapitres"
        );
        assert_eq!(
            section_of(Path::new("8_archives/vrac_historique/x.typ")),
            "archives"
        );
        // Projets historiques (rétro-compatibilité).
        assert_eq!(
            section_of(Path::new("01_architecture/synopsis.typ")),
            "architecture"
        );
        assert_eq!(section_of(Path::new("05_texte/partie_1/s.typ")), "texte");
        assert_eq!(section_of(Path::new("divers/note.typ")), "autre");
    }

    #[test]
    fn liens_reconnu_comme_dossier_session() {
        // La table de liens Wingate (1_atelier/liens) et les anciens dossiers
        // de session sont tous reconnus (exclus des totaux, icône 🔗).
        assert!(crate::symlinks::is_session_folder("liens"));
        assert!(crate::symlinks::is_session_folder("06_session"));
        assert!(crate::symlinks::is_session_folder("06_en_cours"));
        assert!(!crate::symlinks::is_session_folder("4_fiches"));
    }
}
