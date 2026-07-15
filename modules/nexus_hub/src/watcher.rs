// ============================================================================
// modules/nexus_hub/src/watcher.rs — Watcher notify (doc §5.1)
//
// Détecte les modifications de fichiers externes au projet Nexus ouvert.
// Contrairement à file_tree (côté écrivain), pas de thread d'indexation
// séparé ni de DB d'index dédiée à maintenir : le hub relit déjà le
// filesystem et nexus.db à chaque frame (aucune donnée mise en cache côté
// UI — même philosophie que les moyennes glissantes de health/dashboard),
// donc le SEUL rôle du watcher est de réveiller egui (repaint) et de
// signaler qu'un changement a eu lieu — sinon une modification externe
// resterait invisible tant que l'utilisateur n'interagit pas avec la
// fenêtre (egui ne redessine pas en continu).
//
// `notify::recommended_watcher` gère déjà son propre thread de surveillance
// interne (inotify sur Linux) : pas besoin d'un `std::thread::spawn`
// supplémentaire comme le fait l'indexeur de file_tree, qui lui a du VRAI
// travail (scan, écriture DB) à sérialiser sur un thread dédié.
// ============================================================================

use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};

use notify::Watcher;

pub struct ProjectWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<()>,
}

impl ProjectWatcher {
    pub fn start(root: &Path, egui_ctx: egui::Context) -> Result<Self, String> {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok() {
                let _ = tx.send(());
                egui_ctx.request_repaint();
            }
        })
        .map_err(|e| format!("Impossible de démarrer le watcher notify : {e}"))?;
        watcher
            .watch(root, notify::RecursiveMode::Recursive)
            .map_err(|e| format!("Impossible de surveiller {} : {e}", root.display()))?;
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Draine les événements en attente ; renvoie `true` si au moins un
    /// changement externe a été détecté depuis le dernier appel.
    pub fn poll_changed(&self) -> bool {
        let mut changed = false;
        loop {
            match self.rx.try_recv() {
                Ok(()) => changed = true,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecte_une_ecriture_de_fichier_externe() {
        let tmp = tempfile::tempdir().expect("tmp");
        let w = ProjectWatcher::start(tmp.path(), egui::Context::default()).expect("watcher");
        // Pas de changement avant toute écriture.
        assert!(!w.poll_changed());
        std::fs::write(tmp.path().join("externe.typst"), "contenu").expect("write");
        // notify est asynchrone (thread OS) : on laisse une marge courte.
        let mut seen = false;
        for _ in 0..50 {
            if w.poll_changed() {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen, "le watcher n'a pas détecté l'écriture externe");
    }

    #[test]
    fn poll_changed_se_vide_apres_lecture() {
        let tmp = tempfile::tempdir().expect("tmp");
        let w = ProjectWatcher::start(tmp.path(), egui::Context::default()).expect("watcher");
        std::fs::write(tmp.path().join("a.typst"), "x").expect("write");
        let mut seen = false;
        for _ in 0..50 {
            if w.poll_changed() {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen);
        // Rien de nouveau depuis : le prochain poll doit être vide.
        assert!(!w.poll_changed());
    }
}
