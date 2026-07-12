/// Les actions exposées par la palette globale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    TreeNewFile,
    TreeNewFolder,
    TreeRename,
    ProjectOpen,
    ProjectNew,
    ContextAdd,
    ContextClear,
    ContextGoToOriginal,
    ProjectSearch,
    BackupNow,
    BackupShowDir,
    /// Ouvre la fenêtre Cockpit (édition UI des RON).
    CockpitOpen,
    /// Bascule l'affichage du cockpit.
    CockpitToggle,
    /// Ouvre la fenêtre Sticky Notes.
    StickyNotesOpen,
    /// Bascule l'affichage de Sticky Notes.
    StickyNotesToggle,
    /// Ouvre la fenêtre WrapDrive (Analyse rapide + Dialogue projet).
    WrapDriveOpen,
    /// Ouvre la fenêtre Timeline (vue chronologique).
    TimelineOpen,
    /// §3.4 — Vue chronologique triée par date des deux fichiers globaux
    /// chronologie/{biographies,evenements}.typ.
    TimelineChronology,
    /// §4 — Ouvre la fenêtre Claude Terminal.
    ClaudeTerminalOpen,
}

impl PaletteAction {
    pub const ALL: &'static [PaletteAction] = &[
        Self::TreeNewFile,
        Self::TreeNewFolder,
        Self::TreeRename,
        Self::ProjectOpen,
        Self::ProjectNew,
        Self::ContextAdd,
        Self::ContextClear,
        Self::ContextGoToOriginal,
        Self::ProjectSearch,
        Self::BackupNow,
        Self::BackupShowDir,
        Self::CockpitOpen,
        Self::CockpitToggle,
        Self::StickyNotesOpen,
        Self::StickyNotesToggle,
        Self::WrapDriveOpen,
        Self::TimelineOpen,
        Self::TimelineChronology,
        Self::ClaudeTerminalOpen,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::TreeNewFile => "tree: nouveau fichier",
            Self::TreeNewFolder => "tree: nouveau dossier",
            Self::TreeRename => "tree: renommer",
            Self::ProjectOpen => "project: ouvrir projet",
            Self::ProjectNew => "project: nouveau projet",
            Self::ContextAdd => "context: ajouter à en cours",
            Self::ContextClear => "context: vider en cours",
            Self::ContextGoToOriginal => "context: aller à l'original",
            Self::ProjectSearch => "project: chercher dans le projet",
            Self::BackupNow => "backup: sauvegarder maintenant",
            Self::BackupShowDir => "backup: afficher le dossier de sauvegarde",
            Self::CockpitOpen => "cockpit: ouvrir la fenêtre de configuration",
            Self::CockpitToggle => "cockpit: afficher / masquer",
            Self::StickyNotesOpen => "sticky notes: ouvrir la fenêtre",
            Self::StickyNotesToggle => "sticky notes: afficher / masquer",
            Self::WrapDriveOpen => "wrapdrive: ouvrir le panel (Analyse / Dialogue projet)",
            Self::TimelineOpen => "timeline: ouvrir la vue",
            Self::TimelineChronology => "timeline: chronologie",
            Self::ClaudeTerminalOpen => "coh2b: ouvrir",
        }
    }

    pub fn command(&self) -> &'static str {
        match self {
            Self::TreeNewFile => "tree_new_file",
            Self::TreeNewFolder => "tree_new_folder",
            Self::TreeRename => "tree_rename",
            Self::ProjectOpen => "project_open",
            Self::ProjectNew => "project_new",
            Self::ContextAdd => "context_add",
            Self::ContextClear => "context_clear",
            Self::ContextGoToOriginal => "context_go_to_original",
            Self::ProjectSearch => "project_search",
            Self::BackupNow => "backup_now",
            Self::BackupShowDir => "backup_show_dir",
            Self::CockpitOpen => "open_cockpit",
            Self::CockpitToggle => "toggle_cockpit",
            Self::StickyNotesOpen => engram_core::notes_open_command(),
            Self::StickyNotesToggle => engram_core::notes_toggle_command(),
            Self::WrapDriveOpen => "open_wrapdrive",
            Self::TimelineOpen => "open_timeline",
            Self::TimelineChronology => "open_timeline_chronology",
            Self::ClaudeTerminalOpen => "open_claude_terminal",
        }
    }

    pub fn from_command(cmd: &str) -> Option<Self> {
        Some(match cmd {
            "tree_new_file" => Self::TreeNewFile,
            "tree_new_folder" => Self::TreeNewFolder,
            "tree_rename" => Self::TreeRename,
            "project_open" => Self::ProjectOpen,
            "project_new" => Self::ProjectNew,
            "context_add" => Self::ContextAdd,
            "context_clear" => Self::ContextClear,
            "context_go_to_original" => Self::ContextGoToOriginal,
            "project_search" => Self::ProjectSearch,
            "backup_now" => Self::BackupNow,
            "backup_show_dir" => Self::BackupShowDir,
            "open_cockpit" => Self::CockpitOpen,
            "toggle_cockpit" => Self::CockpitToggle,
            cmd if cmd == engram_core::notes_open_command() => Self::StickyNotesOpen,
            cmd if cmd == engram_core::notes_toggle_command() => Self::StickyNotesToggle,
            "open_wrapdrive" => Self::WrapDriveOpen,
            "open_timeline" => Self::TimelineOpen,
            "open_timeline_chronology" => Self::TimelineChronology,
            "open_claude_terminal" => Self::ClaudeTerminalOpen,
            _ => return None,
        })
    }
}
