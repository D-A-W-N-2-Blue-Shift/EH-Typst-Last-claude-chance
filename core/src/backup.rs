// ============================================================================
// core/src/backup.rs — WrapDrive : backup automatique invisible
//
// Un thread de fond archive périodiquement le projet ACTIF en .tar.gz.
// Invisible : aucune UI, aucune notification, SAUF en cas d'erreur (remontée
// en GLaDOS dans la status bar du core). Configuré par la section "backup" de
// engram.ron (ou legacy licorne-a-gerber.ron) (cf. BackupConfig).
//
// Projet actif : lu depuis ~/.config/engram_hive/modules/file_tree/
// last_project.ron (relu à chaque backup ; suivre le module file_tree sans en
// dépendre — le core ne connaît pas ses types).
//
// Déclenchement manuel : commande palette « backup: sauvegarder maintenant »
// → ModuleResponse::BackupNow → BackupHandle::trigger_now().
// ============================================================================

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Section "backup" de engram.ron. Tous les champs ont un défaut : repli
/// legacy sur licorne-a-gerber.ron si besoin.
/// commenter une ligne = revenir au défaut.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    /// Backup automatique périodique actif (le manuel marche même si false).
    pub enabled: bool,
    /// Intervalle entre deux backups automatiques, en minutes.
    pub interval_minutes: u64,
    /// Purge des archives plus vieilles que ce nombre de jours.
    pub keep_days: u64,
    /// Format d'archive. Seul "tar.gz" est supporté pour l'instant.
    pub format: String,
    /// Dossier de destination. Vide = ~/.local/share/engram_hive/backups/.
    pub destination: String,
    /// Copie supplémentaire de la dernière archive vers ce chemin. Vide = rien.
    pub auto_export_path: String,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 30,
            keep_days: 30,
            format: "tar.gz".to_string(),
            destination: String::new(),
            auto_export_path: String::new(),
        }
    }
}

/// Poignée vers le thread de backup. Tient le canal de déclenchement manuel et
/// la file d'erreurs (drainée chaque frame par le core pour GLaDOS).
pub struct BackupHandle {
    trigger_tx: mpsc::Sender<()>,
    errors: Arc<Mutex<Vec<String>>>,
    dest_dir: PathBuf,
}

impl BackupHandle {
    /// Déclenche un backup immédiat (commande palette manuelle).
    pub fn trigger_now(&self) {
        // Si le thread est mort, on tombe en erreur silencieuse côté envoi :
        // ce n'est pas critique (l'app tourne sans backup).
        let _ = self.trigger_tx.send(());
    }

    /// Dossier de destination des archives (résolu au démarrage). Sert à la
    /// commande palette « backup: afficher le dossier de sauvegarde ».
    pub fn dest_dir(&self) -> &Path {
        &self.dest_dir
    }

    /// Récupère et vide les erreurs accumulées (à afficher en status bar).
    pub fn drain_errors(&self) -> Vec<String> {
        match self.errors.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => Vec::new(),
        }
    }
}

/// Démarre le thread de backup. Ne bloque jamais le démarrage de l'app : si le
/// thread ne peut pas être lancé, on renvoie quand même une poignée inerte.
pub fn spawn(config_dir: PathBuf, data_dir: PathBuf, cfg: BackupConfig) -> BackupHandle {
    let (tx, rx) = mpsc::channel();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let errs = errors.clone();
    let dest_dir = resolved_dest(&data_dir, &cfg);
    if let Err(e) = std::thread::Builder::new()
        .name("engram_backup".into())
        .spawn(move || run(config_dir, data_dir, cfg, rx, errs))
    {
        errors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!(
                "WrapDrive n'a pas pu démarrer ({e}). Pas de backup auto."
            ));
    }
    BackupHandle {
        trigger_tx: tx,
        errors,
        dest_dir,
    }
}

/// Dossier de destination des archives : `destination` si non vide, sinon
/// ~/.local/share/engram_hive/backups/.
fn resolved_dest(data_dir: &Path, cfg: &BackupConfig) -> PathBuf {
    if cfg.destination.trim().is_empty() {
        data_dir.join("backups")
    } else {
        PathBuf::from(cfg.destination.trim())
    }
}

fn run(
    config_dir: PathBuf,
    data_dir: PathBuf,
    cfg: BackupConfig,
    rx: mpsc::Receiver<()>,
    errors: Arc<Mutex<Vec<String>>>,
) {
    let interval = Duration::from_secs(cfg.interval_minutes.max(1) * 60);
    loop {
        // On attend SOIT l'intervalle (backup auto), SOIT un déclenchement
        // manuel (qui marche même si l'auto est désactivé).
        let manual = match rx.recv_timeout(interval) {
            Ok(()) => true,
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => break, // app fermée
        };
        if !manual && !cfg.enabled {
            continue;
        }
        match do_backup(&config_dir, &data_dir, &cfg) {
            Ok(archive) => {
                log_line(&data_dir, &format!("OK {}", archive.display()));
            }
            Err(e) => {
                log_line(&data_dir, &format!("ERREUR {e}"));
                if let Ok(mut g) = errors.lock() {
                    g.push(format!("WrapDrive : backup raté — {e}"));
                }
            }
        }
    }
}

/// Lit le projet actif depuis last_project.ron du module file_tree. Le core ne
/// dépend pas de file_tree : on parse le RON minimal localement.
fn active_project(config_dir: &Path) -> Option<PathBuf> {
    #[derive(serde::Deserialize)]
    struct LastProject {
        path: String,
    }
    let file = config_dir
        .join("modules")
        .join("file_tree")
        .join("last_project.ron");
    let raw = std::fs::read_to_string(file).ok()?;
    let lp: LastProject = ron::from_str(&raw).ok()?;
    let p = PathBuf::from(lp.path);
    p.is_dir().then_some(p)
}

fn do_backup(config_dir: &Path, data_dir: &Path, cfg: &BackupConfig) -> Result<PathBuf, String> {
    if cfg.format != "tar.gz" {
        return Err(format!(
            "format '{}' non supporté (seul 'tar.gz' l'est).",
            cfg.format
        ));
    }
    let project = active_project(config_dir)
        .ok_or("aucun projet actif (last_project.ron absent ou invalide).")?;
    let name = project
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("projet");

    let dest_dir = resolved_dest(data_dir, cfg);
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("impossible de créer {} : {e}", dest_dir.display()))?;

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M");
    let archive = dest_dir.join(format!("{name}_{stamp}.tar.gz"));
    write_targz(&project, name, &archive, &dest_dir)?;

    purge_old(&dest_dir, cfg.keep_days);

    if !cfg.auto_export_path.trim().is_empty() {
        let export = PathBuf::from(cfg.auto_export_path.trim());
        if let Some(parent) = export.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&archive, &export) {
            return Err(format!(
                "archive créée mais export vers {} raté : {e}",
                export.display()
            ));
        }
    }
    Ok(archive)
}

/// Archive `project` (sous le préfixe `name/`) en tar.gz vers `archive`. Évite
/// d'aspirer le dossier de destination s'il est imbriqué dans le projet
/// (sinon : archive qui se mange la queue).
fn write_targz(project: &Path, name: &str, archive: &Path, dest_dir: &Path) -> Result<(), String> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let file = std::fs::File::create(archive)
        .map_err(|e| format!("création de {} ratée : {e}", archive.display()))?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(enc);

    let dest_canon = dest_dir.canonicalize().ok();
    for entry in walk(project) {
        // Ne pas archiver le dossier de backups s'il vit dans le projet.
        if let Some(dc) = &dest_canon {
            if entry
                .canonicalize()
                .ok()
                .as_deref()
                .map(|p| p.starts_with(dc))
                == Some(true)
            {
                continue;
            }
        }
        let rel = match entry.strip_prefix(project) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let in_archive = Path::new(name).join(rel);
        if entry.is_dir() {
            builder
                .append_dir(&in_archive, &entry)
                .map_err(|e| format!("tar dir {} : {e}", entry.display()))?;
        } else if entry.is_file() {
            let mut f = std::fs::File::open(&entry)
                .map_err(|e| format!("ouverture {} : {e}", entry.display()))?;
            builder
                .append_file(&in_archive, &mut f)
                .map_err(|e| format!("tar file {} : {e}", entry.display()))?;
        }
    }
    builder
        .into_inner()
        .map_err(|e| format!("finalisation tar : {e}"))?
        .finish()
        .map_err(|e| format!("finalisation gzip : {e}"))?;
    Ok(())
}

/// Parcours récursif simple (sans dépendance externe). Symlinks non suivis.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            out.push(p.clone());
            if is_dir {
                stack.push(p);
            }
        }
    }
    out
}

/// Supprime les .tar.gz du dossier plus vieux que `keep_days` jours. Best
/// effort : toute erreur d'IO est ignorée (la rétention n'est pas critique).
fn purge_old(dest_dir: &Path, keep_days: u64) {
    if keep_days == 0 {
        return;
    }
    let max_age = Duration::from_secs(keep_days * 24 * 60 * 60);
    let Ok(rd) = std::fs::read_dir(dest_dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("gz") {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if let Ok(age) = modified.elapsed() {
            if age > max_age {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

fn log_line(data_dir: &Path, msg: &str) {
    use std::io::Write;
    let dir = data_dir.join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("backup.log"))
    {
        let _ = writeln!(f, "[{stamp}] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_brief() {
        let c = BackupConfig::default();
        assert!(c.enabled);
        assert_eq!(c.interval_minutes, 30);
        assert_eq!(c.keep_days, 30);
        assert_eq!(c.format, "tar.gz");
    }

    #[test]
    fn backup_creates_archive_of_active_project() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let config_dir = tmp.path().join("config");
        let data_dir = tmp.path().join("data");
        let project = tmp.path().join("mon_projet");
        std::fs::create_dir_all(project.join(".engram"))?;
        std::fs::write(project.join("scene.typ"), "= Titre\ncorps")?;

        // last_project.ron pointant vers le projet.
        let ft_dir = config_dir.join("modules").join("file_tree");
        std::fs::create_dir_all(&ft_dir)?;
        std::fs::write(
            ft_dir.join("last_project.ron"),
            format!("LastProject(path: {:?})", project.display().to_string()),
        )?;

        let cfg = BackupConfig::default();
        let archive = do_backup(&config_dir, &data_dir, &cfg)?;
        assert!(archive.exists());
        assert!(archive
            .file_name()
            .ok_or("archive sans nom de fichier")?
            .to_str()
            .ok_or("nom d'archive non UTF-8")?
            .starts_with("mon_projet_"));
        Ok(())
    }

    #[test]
    fn no_active_project_is_an_error() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let cfg = BackupConfig::default();
        let err = do_backup(&tmp.path().join("config"), &tmp.path().join("data"), &cfg);
        assert!(err.is_err());
        Ok(())
    }
}
