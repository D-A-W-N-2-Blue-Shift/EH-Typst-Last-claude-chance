// ============================================================================
// core/src/lib.rs — Point d'entrée du noyau Engram_Hive
//
// Ce fichier expose l'API que les modules utilisent pour parler au core :
//   - le trait Module (le contrat que chaque module implémente)
//   - ModuleResponse (la SEULE façon pour un module d'agir sur le core)
//   - CoreEvent (la SEULE façon pour le core de pousser de l'info aux modules)
//   - CoreContext (chemins de config/data, rien d'autre)
//   - ModuleRegistry + ModulesConfig (chargement depuis modules.ron)
//
// Le core ne connaît AUCUN module par son nom. Le binaire (app/) remplit le
// registre, et ~/.config/engram_hive/modules.ron décide de ce qui est activé.
// ============================================================================

pub mod backup;
mod config;
pub mod fs;
pub mod ipc;
pub mod licorne;
mod module_api;
pub mod theme;

pub use backup::{BackupConfig, BackupHandle};
pub use config::{ModuleRegistry, ModulesConfig};
pub use fs::atomic_write;
pub use ipc::{IpcCommand, IpcServer};
pub use licorne::Licorne;
pub use module_api::{
    notes_module_name, notes_open_command, notes_toggle_command, CoreContext, CoreEvent, Module,
    ModuleResponse, RenderMode, StickyNoteMarkerRef,
};
pub use theme::Palette;
