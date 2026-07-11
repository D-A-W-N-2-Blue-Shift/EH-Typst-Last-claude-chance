// ============================================================================
// core/src/module_api.rs — Le contrat Core ↔ Modules
//
// Étanchéité stricte :
//   - Un module ne touche JAMAIS au core directement. Il pousse des
//     ModuleResponse dans le Vec fourni à chaque frame.
//   - Le core ne touche JAMAIS à l'intérieur d'un module. Il lui envoie des
//     CoreEvent (ex: relais d'un changement de focus).
//   - Les modules ne se parlent JAMAIS entre eux. Le core est l'arbitre.
// ============================================================================

use std::path::PathBuf;
use std::sync::Arc;

/// La seule voie de communication module → core.
#[derive(Debug, Clone)]
pub enum ModuleResponse {
    /// Demande au core d'ouvrir un fichier dans une fenêtre éditeur.
    /// Le chemin est TOUJOURS canonique (résolu via std::fs::canonicalize)
    /// pour que le core puisse dédupliquer les buffers (même inode = même buffer).
    OpenFile(PathBuf),

    /// Demande au core de créer un fichier (s'il n'existe pas) puis de
    /// l'ouvrir. Utilisé par les wikilinks orphelins : clic sur un
    /// [[lien rouge]] → création dans <projet>/orphelins/<nom>.typ.
    /// Le core crée les dossiers parents, le fichier vide, puis rediffuse
    /// un CoreEvent::OpenFileRequested avec le chemin canonique.
    CreateAndOpenFile { path: PathBuf },

    /// Un module qui indexe le projet (file_tree) publie la liste des
    /// fichiers .typ connus. Le core la rediffuse à tous les modules via
    /// CoreEvent::FileIndexUpdated — c'est ainsi que l'éditeur alimente
    /// l'auto-complétion des wikilinks SANS scanner lui-même le filesystem
    /// et SANS parler directement au file_tree (les modules ne se parlent
    /// jamais entre eux, le core est l'arbitre).
    PublishFileIndex {
        project_root: PathBuf,
        files: Arc<Vec<PathBuf>>,
    },

    /// La fenêtre du module a bougé. Le core ne fait qu'enregistrer/loguer
    /// l'information.
    WindowMoved { id: String, new_pos: (f32, f32) },

    /// L'éditeur entre/sort du mode focus plein écran. Le core rediffuse
    /// en CoreEvent::FocusModeChanged : le file_tree se masque (état
    /// sauvegardé, pas fermé) puis réapparaît.
    FocusModeChanged(bool),

    /// Une erreur à remonter. Le core la logue ; le module l'affiche AUSSI
    /// dans sa propre fenêtre (règle GLaDOS : jamais d'erreur silencieuse).
    Error { module: String, message: String },

    /// Demande un backup immédiat du projet actif (commande palette
    /// « backup: sauvegarder maintenant »). Le core déclenche WrapDrive.
    BackupNow,

    /// Ouvre le dossier de destination des backups dans le gestionnaire de
    /// fichiers (commande palette « backup: afficher le dossier »).
    OpenBackupDir,

    /// Demande au core de relayer l'ouverture de la fenêtre d'un module
    /// `OwnViewport` (cockpit, timeline, claude_terminal…). Émis depuis la
    /// palette ou un keybind. Les modules concernés écoutent
    /// CoreEvent::OpenModuleWindowRequested et n'ouvrent leur fenêtre que
    /// si le nom matche — par défaut elles démarrent FERMÉES (doctrine
    /// "fenêtre à la demande", on n'impose rien au tiling utilisateur).
    OpenModuleWindow(String),

    /// Demande d'ouverture de la palette globale. Le core/app l'ouvre
    /// directement ; aucun module ne l'héberge.
    OpenPaletteRequested,

    /// Demande de basculer la visibilité d'une fenêtre de module propre.
    ToggleModuleWindow(String),

    /// §3.4 — La palette a demandé la vue chronologique de la timeline
    /// (« timeline: chronologie ») : liste triée par date_sortable des
    /// événements des deux fichiers globaux. Le core relaie au module
    /// timeline via CoreEvent::TimelineChronologyRequested.
    OpenTimelineChronology,

    /// Redémarrage complet du programme demandé (bouton « reboot » du cockpit).
    /// Le core relance le binaire (avec un léger délai pour laisser l'instance
    /// courante libérer le socket IPC) puis ferme proprement la fenêtre racine.
    RestartApp,
}

/// La seule voie de communication core → module.
#[derive(Debug, Clone)]
pub enum CoreEvent {
    /// Le core ordonne à une fenêtre de se déplacer.
    WindowRepositioned { id: String, pos: (f32, f32) },

    /// Un fichier doit être ouvert (relais d'un ModuleResponse::OpenFile ou
    /// CreateAndOpenFile). Le chemin est canonique. Le module éditeur le
    /// consomme ; les autres modules l'ignorent.
    OpenFileRequested(PathBuf),

    /// L'index des fichiers du projet a changé (relais d'un
    /// ModuleResponse::PublishFileIndex). Alimente l'auto-complétion
    /// des wikilinks de l'éditeur.
    FileIndexUpdated {
        project_root: PathBuf,
        files: Arc<Vec<PathBuf>>,
    },

    /// Le mode focus a changé (relais d'un ModuleResponse::FocusModeChanged).
    /// Le file_tree se masque/réapparaît, les autres modules font ce qu'ils
    /// veulent de l'info.
    FocusModeChanged(bool),

    /// Une commande est arrivée par le socket IPC (CLI, Streamdeck).
    /// Chaque module filtre sur `module` et ignore ce qui ne le concerne pas.
    IpcCommand(crate::ipc::IpcCommand),

    /// Relais d'un ModuleResponse::OpenModuleWindow. Chaque module à
    /// viewport propre filtre sur son nom et ouvre sa fenêtre si match.
    OpenModuleWindowRequested(String),

    /// Relais d'un ModuleResponse::ToggleModuleWindow. Chaque module à
    /// viewport propre filtre sur son nom et bascule son état visible.
    ToggleModuleWindowRequested(String),

    /// §3.4 — Relais de la demande "vue chronologique" de la timeline.
    /// Le module timeline s'ouvre et bascule sur la vue chronologie.
    TimelineChronologyRequested,
}

/// Contexte fourni aux modules à l'init : uniquement des chemins.
/// Zéro chemin absolu hardcodé : tout vient de la crate `dirs`.
#[derive(Debug, Clone)]
pub struct CoreContext {
    /// ~/.config/engram_hive_typst
    pub config_dir: PathBuf,
    /// ~/.local/share/engram_hive_typst
    pub data_dir: PathBuf,
    /// Thème global résolu (couleurs + polices), source unique d'apparence.
    /// Chargé depuis theme.ron + section "theme" de engram.ron (legacy
    /// licorne-a-gerber.ron accepté en repli).
    pub theme: crate::theme::Palette,
    /// Configuration experte unifiée (engram.ron). Chaque module y
    /// lit sa section via `licorne.section::<SonTypeVomi>("son_nom", &mut errs)`.
    pub licorne: crate::licorne::Licorne,
}

impl CoreContext {
    /// Résout les dossiers de config et data depuis l'environnement, et charge
    /// le thème global. Retourne aussi les erreurs de thème à loguer/afficher.
    pub fn from_env() -> Result<Self, String> {
        let (ctx, errors) = Self::from_env_with_errors()?;
        for e in errors {
            tracing::warn!(target: "core", "{e}");
        }
        Ok(ctx)
    }

    /// Variante exposant les erreurs de chargement du thème (pour les
    /// remonter dans la fenêtre core en plus des logs).
    pub fn from_env_with_errors() -> Result<(Self, Vec<String>), String> {
        let config_dir = dirs::config_dir()
            .ok_or(
                "Impossible de trouver le dossier de configuration de l'OS. \
                    Ton environnement est plus cassé que prévu.",
            )?
            .join("engram_hive_typst");
        let data_dir = dirs::data_dir()
            .ok_or("Impossible de trouver le dossier de données de l'OS.")?
            .join("engram_hive_typst");
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Impossible de créer {} : {e}", config_dir.display()))?;
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Impossible de créer {} : {e}", data_dir.display()))?;
        // La config experte unifiée d'abord : le thème y lit sa section.
        let (licorne, mut errors) = crate::licorne::Licorne::load(&config_dir);
        let (theme, theme_errors) = crate::theme::Palette::load(&config_dir, &licorne);
        errors.extend(theme_errors);
        Ok((
            Self {
                config_dir,
                data_dir,
                theme,
                licorne,
            },
            errors,
        ))
    }
}

/// Où un module se dessine. La plupart ouvrent leur propre fenêtre OS
/// (viewport egui) ; certains s'intègrent DANS la fenêtre core (le file_tree
/// forme un bloc visuel unifié avec le core — décision architecturale du hub).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Le module ouvre et dessine son propre viewport dans `update`.
    #[default]
    OwnViewport,
    /// Le module ne crée pas de viewport : le core l'appelle via
    /// `draw_embedded` dans un panel de la fenêtre core.
    EmbeddedInCore,
}

/// Le contrat que chaque module implémente. Un module = un dossier isolé
/// dans modules/, une fenêtre OS (ou un panel embarqué), et ce trait.
pub trait Module {
    /// Identifiant stable du module (= nom du dossier dans modules/).
    fn name(&self) -> &'static str;

    /// Appelé une fois au lancement, avant la première frame.
    fn init(&mut self, ctx: &CoreContext) -> Result<(), String>;

    /// Appelé à chaque frame. Un module `OwnViewport` y dessine sa fenêtre ;
    /// un module `EmbeddedInCore` n'y fait QUE sa logique (pas de rendu) et
    /// sera dessiné ensuite via `draw_embedded`. Pousse ses ModuleResponse.
    fn update(&mut self, egui_ctx: &egui::Context, out: &mut Vec<ModuleResponse>);

    /// Mode de rendu. Défaut : fenêtre OS propre.
    fn render_mode(&self) -> RenderMode {
        RenderMode::OwnViewport
    }

    /// Rendu d'un module `EmbeddedInCore` dans un panel fourni par le core.
    /// No-op pour les modules à viewport propre.
    fn draw_embedded(&mut self, _ui: &mut egui::Ui, _out: &mut Vec<ModuleResponse>) {}

    /// Nombre de viewports OS actuellement ouverts par le module (hors le
    /// viewport core). Le core s'en sert pour maintenir un heartbeat de
    /// frame quand sa fenêtre est minimisée : les viewports enfants créés
    /// via `show_viewport_immediate` dépendent de la boucle de frame du
    /// parent. Sans heartbeat, minimiser le core fait chuter la cadence des
    /// viewports enfants (régression documentée §5 du brief 2/7/2026).
    /// Défaut 0 pour les modules qui n'ouvrent pas de viewport.
    fn active_viewport_count(&self) -> usize {
        0
    }

    /// Le core pousse un événement au module (relais de focus, IPC, etc.).
    fn handle_event(&mut self, _event: &CoreEvent) {}

    /// Appelé à la fermeture de l'app. Le module sauvegarde et nettoie.
    fn shutdown(&mut self) {}
}
