// ============================================================================
// modules/file_tree/src/tree_view.rs — Rendu egui de l'arborescence
//
// L'arbre n'est pas une liste de fichiers : c'est un tableau de bord
// sémantique. Comptages de mots à côté des dossiers, progression vers les
// goals, icônes de statut (⚪🔵🟡🟢🔴), 🔗 pour les symlinks de en_cours/.
//
// Ce fichier ne MODIFIE rien : il dessine le modèle (stats::TreeNode) et
// remonte les intentions de l'utilisateur (double-clic, action de menu).
// ============================================================================

use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::Config;
use crate::context_menu::{self, MenuAction};
use crate::stats::{self, ProjectTotals, TreeNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeFilter {
    All,
    Typst,
    Text,
    Images,
    Data,
    Other,
}

impl Default for TreeFilter {
    fn default() -> Self {
        Self::All
    }
}

impl TreeFilter {
    pub const ALL: &'static [Self] = &[
        Self::All,
        Self::Typst,
        Self::Text,
        Self::Images,
        Self::Data,
        Self::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Tous",
            Self::Typst => "Typst",
            Self::Text => "Texte",
            Self::Images => "Images",
            Self::Data => "Données",
            Self::Other => "Autres",
        }
    }

    fn matches_path(self, path: &std::path::Path, is_dir: bool) -> bool {
        if is_dir {
            return true;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match self {
            Self::All => true,
            Self::Typst => ext == "typ",
            Self::Text => matches!(
                ext.as_str(),
                "typ" | "md" | "txt" | "ron" | "toml" | "sql" | "csv" | "tsv" | "json"
            ),
            Self::Images => matches!(
                ext.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg"
            ),
            Self::Data => matches!(
                ext.as_str(),
                "ron" | "toml" | "sql" | "csv" | "tsv" | "json"
            ),
            Self::Other => !matches!(
                ext.as_str(),
                "typ"
                    | "md"
                    | "txt"
                    | "ron"
                    | "toml"
                    | "sql"
                    | "csv"
                    | "tsv"
                    | "json"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "svg"
            ),
        }
    }
}

/// Intentions remontées par le rendu d'une frame.
#[derive(Default)]
pub struct TreeOutput {
    /// Double-clic sur un fichier → ouvrir.
    pub open: Option<PathBuf>,
    /// Action choisie dans un menu contextuel.
    pub menu_action: Option<MenuAction>,
    /// Clic simple → sélection.
    pub selected: Option<PathBuf>,
}

/// État UI persistant du tree (plis/déplis, sélection).
#[derive(Default)]
pub struct TreeUiState {
    pub expanded: HashSet<PathBuf>,
    pub selected: Option<PathBuf>,
    pub filter: TreeFilter,
}

/// Couleurs résolues du thème, passées au rendu de l'arbre (le file_tree n'a
/// pas de palette à lui : elle vient du core via CoreContext).
#[derive(Clone, Copy)]
pub struct TreeColors {
    /// Dossiers : accent_secondary (violet néon).
    pub folder: egui::Color32,
    /// Fichiers .typ : foreground (magenta).
    pub file: egui::Color32,
    /// Symlinks de en_cours/ : accent_secondary atténué.
    pub symlink: egui::Color32,
    /// Lignes verticales d'indentation : accent_secondary à 30%.
    pub guide: egui::Color32,
}

/// Dessine l'arbre complet (racine incluse) et retourne les intentions.
pub fn show(
    ui: &mut egui::Ui,
    root: &TreeNode,
    totals: &ProjectTotals,
    goal_words: u64,
    state: &mut TreeUiState,
    cfg: &Config,
    colors: &TreeColors,
) -> TreeOutput {
    let mut out = TreeOutput::default();

    ui.horizontal(|ui| {
        ui.weak("Filtre");
        egui::ComboBox::from_id_salt("file_tree_filter")
            .selected_text(state.filter.label())
            .show_ui(ui, |ui| {
                for filter in TreeFilter::ALL {
                    ui.selectable_value(&mut state.filter, *filter, filter.label());
                }
            });
    });
    ui.add_space(4.0);

    // Racine : nom du projet + total global (+ goal projet si défini).
    let header = if cfg.simple.show_word_counts {
        if goal_words > 0 {
            format!(
                "📁 {}   [{} mots / goal: {}]",
                root.name,
                stats::format_number(totals.total_words, cfg),
                stats::format_number(goal_words, cfg)
            )
        } else {
            format!(
                "📁 {}   [{} mots]",
                root.name,
                stats::format_number(totals.total_words, cfg)
            )
        }
    } else {
        format!("📁 {}", root.name)
    };
    ui.label(egui::RichText::new(header).strong().color(colors.folder));
    ui.add_space(2.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for child in &root.children {
                render_node(ui, child, 0, false, state, cfg, colors, &mut out);
            }
        });
    out
}

#[allow(clippy::too_many_arguments)] // historique : refactor en struct contexte = scope dédié
fn render_node(
    ui: &mut egui::Ui,
    node: &TreeNode,
    depth: usize,
    inside_en_cours: bool,
    state: &mut TreeUiState,
    cfg: &Config,
    colors: &TreeColors,
    out: &mut TreeOutput,
) {
    if !subtree_visible(node, state.filter) {
        return;
    }
    let step = cfg.vomi.indent_size as f32;
    let indent = depth as f32 * step;
    let row_height = cfg.vomi.row_height as f32;
    let expanded = state.expanded.contains(&node.path);
    let label = row_label(node, expanded, inside_en_cours, cfg);
    let is_selected = state.selected.as_ref() == Some(&node.path);

    // Couleur de la ligne selon le type : symlink (atténué) > dossier > fichier.
    let color = if node.is_symlink {
        colors.symlink
    } else if node.is_dir {
        colors.folder
    } else {
        colors.file
    };

    let row = ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add_space(indent);
            let resp = ui.selectable_label(is_selected, egui::RichText::new(label).color(color));

            if resp.clicked() {
                state.selected = Some(node.path.clone());
                out.selected = Some(node.path.clone());
                if node.is_dir {
                    if expanded {
                        state.expanded.remove(&node.path);
                    } else {
                        state.expanded.insert(node.path.clone());
                    }
                }
            }
            if resp.double_clicked() && !node.is_dir {
                out.open = Some(node.path.clone());
            }
            resp.context_menu(|ui| {
                if let Some(a) = context_menu::build(ui, node, inside_en_cours) {
                    out.menu_action = Some(a);
                }
            });
        },
    );

    // Guides d'indentation : une ligne verticale par niveau ancêtre, centrée
    // dans sa colonne d'indentation. Les segments des rangées contiguës se
    // raccordent en lignes continues le long de l'arbre.
    if depth > 0 && step > 1.0 {
        let rect = row.response.rect;
        let painter = ui.painter();
        let stroke = egui::Stroke::new(1.0_f32, colors.guide);
        for i in 0..depth {
            let x = rect.left() + step * i as f32 + step * 0.5;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                stroke,
            );
        }
    }

    if node.is_dir && expanded {
        for child in &node.children {
            render_node(
                ui,
                child,
                depth + 1,
                inside_en_cours || node.is_en_cours,
                state,
                cfg,
                colors,
                out,
            );
        }
    }
}

fn subtree_visible(node: &TreeNode, filter: TreeFilter) -> bool {
    if node.is_dir {
        node.children
            .iter()
            .any(|child| subtree_visible(child, filter))
    } else {
        filter.matches_path(&node.path, false)
    }
}

/// Construit le texte d'une ligne : icône, nom, stats, statut.
fn row_label(node: &TreeNode, expanded: bool, inside_en_cours: bool, cfg: &Config) -> String {
    let mut s = String::new();

    // Symlink (06_en_cours/) : icône 🔗 dédiée si activée. Sinon dossier/fichier.
    if node.is_symlink && cfg.simple.show_symlink_icons {
        s.push_str("🔗 ");
    } else if node.is_dir {
        s.push_str(if expanded { "📂 " } else { "📁 " });
    } else {
        s.push_str("📄 ");
    }
    s.push_str(&node.name);

    if node.is_en_cours {
        s.push_str("   [contexte actif]");
        return s;
    }

    if cfg.simple.show_word_counts && !inside_en_cours {
        if node.is_dir {
            if node.words > 0 {
                s.push_str(&format!(
                    "   [{}]",
                    stats::format_count(node.words, node.chars, cfg)
                ));
            }
        } else if node.goal > 0 {
            s.push_str(&format!(
                "   [{} / goal: {}]",
                stats::format_count(node.words, node.chars, cfg),
                stats::format_number(node.goal, cfg)
            ));
        } else if node.words > 0 {
            s.push_str(&format!(
                "   [{}]",
                stats::format_count(node.words, node.chars, cfg)
            ));
        }
    }

    if cfg.simple.show_status_indicators && !inside_en_cours {
        if let Some(st) = node.status {
            s.push(' ');
            s.push_str(st.icon());
        }
    }

    s
}
