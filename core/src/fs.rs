use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path_for(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("atomic");
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{stem}.{pid}.{nanos}.{counter}.tmp"))
}

fn cleanup_temp(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Écrit `contents` atomiquement dans `path`.
///
/// Stratégie :
/// - temporaire unique dans le même répertoire ;
/// - écriture complète ;
/// - `flush` puis `sync_all` ;
/// - `rename` vers la cible ;
/// - nettoyage du temporaire en cas d'échec.
pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|e| format!("Impossible de créer {} : {e}", parent.display()))?;
    let data = contents.as_ref();
    let target_perms = fs::metadata(path).ok().map(|m| m.permissions());

    for _ in 0..16 {
        let tmp_path = temp_path_for(path);
        let file_res = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path);
        let mut file = match file_res {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "Impossible de préparer l'écriture atomique de {} : {e}",
                    path.display()
                ))
            }
        };

        if let Some(perms) = target_perms.clone() {
            if let Err(e) = fs::set_permissions(&tmp_path, perms) {
                cleanup_temp(&tmp_path);
                return Err(format!(
                    "Impossible de préserver les permissions de {} : {e}",
                    path.display()
                ));
            }
        }

        if let Err(e) = file.write_all(data) {
            cleanup_temp(&tmp_path);
            return Err(format!("Impossible d'écrire {} : {e}", path.display()));
        }
        if let Err(e) = file.flush() {
            cleanup_temp(&tmp_path);
            return Err(format!(
                "Impossible de vider le tampon vers {} : {e}",
                path.display()
            ));
        }
        if let Err(e) = file.sync_all() {
            cleanup_temp(&tmp_path);
            return Err(format!(
                "Impossible de synchroniser {} : {e}",
                path.display()
            ));
        }
        drop(file);

        match fs::rename(&tmp_path, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                cleanup_temp(&tmp_path);
                return Err(format!(
                    "Impossible de remplacer {} par atomique : {e}",
                    path.display()
                ));
            }
        }
    }

    Err(format!(
        "Impossible de créer un temporaire unique pour {}.",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_and_replaces_content() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("note.typ");

        atomic_write(&path, "un\ntexte\n")?;
        assert_eq!(fs::read_to_string(&path)?, "un\ntexte\n");

        atomic_write(&path, "deux")?;
        assert_eq!(fs::read_to_string(&path)?, "deux");

        let leftovers: Vec<_> = fs::read_dir(dir.path())?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(".note.typ.") && name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_surfaces_errors_without_deleting_target(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir()?;
        let path = dir.path().join("locked.typ");
        fs::write(&path, "gardé")?;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&path, perms)?;
        let mut dir_perms = fs::metadata(dir.path())?.permissions();
        dir_perms.set_mode(0o555);
        fs::set_permissions(dir.path(), dir_perms)?;

        let err = atomic_write(&path, "nouveau").unwrap_err();
        assert!(err.contains("Impossible"));
        assert_eq!(fs::read_to_string(&path)?, "gardé");
        Ok(())
    }
}
