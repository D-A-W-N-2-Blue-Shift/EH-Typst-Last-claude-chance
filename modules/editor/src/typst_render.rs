use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use engram_core::atomic_write;
use regex::Regex;

use crate::buffer::{SharedBuffer, SharedBufferExt};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum RenderState {
    Idle,
    Running,
    Success,
    Error,
    Unavailable,
}

impl Default for RenderState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ErrorLocation {
    pub file: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TypstSnapshot {
    pub engine_available: bool,
    pub engine_version: Option<String>,
    pub source: Option<PathBuf>,
    pub pdf: Option<PathBuf>,
    pub state: RenderState,
    pub last_revision: u64,
    pub last_success_revision: u64,
    pub last_duration_ms: Option<u128>,
    pub last_stdout: String,
    pub last_stderr: String,
    pub last_error: Option<ErrorLocation>,
    pub needs_initial_render: bool,
}

#[derive(Debug)]
struct ActiveJob {
    path: PathBuf,
    revision: u64,
    root: PathBuf,
    source: PathBuf,
    pdf_tmp: PathBuf,
    pdf_final: PathBuf,
}

#[derive(Debug)]
struct JobResult {
    path: PathBuf,
    revision: u64,
    pdf_tmp: PathBuf,
    pdf_final: PathBuf,
    duration_ms: u128,
    stdout: String,
    stderr: String,
    success: bool,
}

pub struct TypstRenderService {
    cache_root: PathBuf,
    status_file: PathBuf,
    engine_available: bool,
    engine_version: Option<String>,
    queued: BTreeMap<PathBuf, u64>,
    active: Option<ActiveJob>,
    tx: Sender<JobResult>,
    rx: Receiver<JobResult>,
    snapshots: HashMap<PathBuf, TypstSnapshot>,
}

impl TypstRenderService {
    pub fn new(data_dir: PathBuf) -> Self {
        let cache_root = data_dir.join("typst-render");
        let _ = std::fs::create_dir_all(&cache_root);
        let status_file = cache_root.join("status.ron");
        let (tx, rx) = mpsc::channel();
        let (engine_available, engine_version) = probe_engine();
        Self {
            cache_root,
            status_file,
            engine_available,
            engine_version,
            queued: BTreeMap::new(),
            active: None,
            tx,
            rx,
            snapshots: HashMap::new(),
        }
    }

    pub fn engine_available(&self) -> bool {
        self.engine_available
    }

    pub fn engine_version(&self) -> Option<&str> {
        self.engine_version.as_deref()
    }

    pub fn snapshot_for(&self, path: &Path) -> TypstSnapshot {
        self.snapshots
            .get(path)
            .cloned()
            .unwrap_or_else(|| self.default_snapshot(path))
    }

    pub fn request_render(&mut self, shared: &SharedBuffer) {
        let buf = shared.read_buf();
        if !is_typst_file(&buf.path) {
            return;
        }
        if !self.engine_available {
            let mut snapshot = self.default_snapshot(&buf.path);
            snapshot.engine_available = false;
            snapshot.engine_version = self.engine_version.clone();
            snapshot.state = RenderState::Unavailable;
            snapshot.last_stderr =
                "Le moteur Typst est introuvable. Installe ou configure le binaire typst."
                    .to_string();
            snapshot.needs_initial_render = false;
            self.snapshots.insert(buf.path.clone(), snapshot.clone());
            self.write_status_file(&snapshot);
            return;
        }
        let revision = buf.version;
        let path = buf.path.clone();
        drop(buf);
        let default_snapshot = self.default_snapshot(&path);
        let snapshot = self
            .snapshots
            .entry(path.clone())
            .or_insert(default_snapshot);
        snapshot.engine_available = true;
        snapshot.engine_version = self.engine_version.clone();
        snapshot.source = Some(path.clone());
        snapshot.needs_initial_render = false;
        snapshot.last_revision = revision;
        self.queued.insert(path, revision);
        self.maybe_start_next();
    }

    pub fn tick(&mut self) {
        while let Ok(result) = self.rx.try_recv() {
            self.finish_job(result);
        }
        self.maybe_start_next();
    }

    pub fn cache_dir_for(&self, path: &Path) -> PathBuf {
        self.cache_root.join(hash_path(path))
    }

    pub fn final_pdf_for(&self, path: &Path) -> PathBuf {
        self.cache_dir_for(path).join("document.pdf")
    }

    pub fn open_kate(&self, path: &Path) -> Result<(), String> {
        spawn_viewer("kate", path)
    }

    pub fn open_okular(&self, path: &Path) -> Result<(), String> {
        spawn_viewer("okular", path)
    }

    pub fn open_render_dir(&self, path: &Path) -> Result<(), String> {
        spawn_viewer_dir(self.cache_dir_for(path))
    }

    pub fn open_last_valid_pdf(&self, path: &Path) -> Result<(), String> {
        let pdf = self
            .snapshots
            .get(path)
            .and_then(|s| s.pdf.clone())
            .ok_or_else(|| {
                format!(
                    "Aucun PDF Typst valide pour {}. Lance d'abord le rendu.",
                    path.display()
                )
            })?;
        if !pdf.exists() {
            return Err(format!(
                "Aucun PDF Typst valide disponible pour {} pour le moment.",
                path.display()
            ));
        }
        spawn_viewer("okular", &pdf)
    }

    fn default_snapshot(&self, path: &Path) -> TypstSnapshot {
        TypstSnapshot {
            engine_available: self.engine_available,
            engine_version: self.engine_version.clone(),
            source: Some(path.to_path_buf()),
            pdf: Some(self.final_pdf_for(path)),
            state: if self.engine_available {
                RenderState::Idle
            } else {
                RenderState::Unavailable
            },
            last_revision: 0,
            last_success_revision: 0,
            last_duration_ms: None,
            last_stdout: String::new(),
            last_stderr: String::new(),
            last_error: None,
            needs_initial_render: true,
        }
    }

    fn maybe_start_next(&mut self) {
        if self.active.is_some() || !self.engine_available {
            return;
        }
        let Some((path, revision)) = self
            .queued
            .iter()
            .max_by_key(|(_, revision)| *revision)
            .map(|(path, revision)| (path.clone(), *revision))
        else {
            return;
        };
        self.queued.remove(&path);
        self.start_job(path, revision);
    }

    fn start_job(&mut self, path: PathBuf, revision: u64) {
        let cache_dir = self.cache_dir_for(&path);
        let root = project_root_for(&path);
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            let mut snapshot = self
                .snapshots
                .get(&path)
                .cloned()
                .unwrap_or_else(|| self.default_snapshot(&path));
            snapshot.state = RenderState::Error;
            snapshot.last_stderr = format!("Impossible de créer {} : {e}", cache_dir.display());
            snapshot.last_error = Some(ErrorLocation {
                file: path.clone(),
                line: None,
                column: None,
                message: snapshot.last_stderr.clone(),
            });
            self.snapshots.insert(path.clone(), snapshot.clone());
            self.write_status_file(&snapshot);
            return;
        }
        let pdf_tmp = cache_dir.join(format!("document.rev{revision}.pdf"));
        let pdf_final = cache_dir.join("document.pdf");
        let job = ActiveJob {
            path: path.clone(),
            revision,
            root: root.clone(),
            source: path.clone(),
            pdf_tmp: pdf_tmp.clone(),
            pdf_final: pdf_final.clone(),
        };
        let job_path = job.path.clone();
        let job_revision = job.revision;
        let job_root = job.root.clone();
        let job_source = job.source.clone();
        let job_pdf_tmp = job.pdf_tmp.clone();
        let job_pdf_final = job.pdf_final.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            let output = Command::new("typst")
                .arg("compile")
                .arg("--root")
                .arg(&job_root)
                .arg(&job_source)
                .arg(&job_pdf_tmp)
                .output();
            let elapsed = start.elapsed().as_millis();
            let result = match output {
                Ok(out) => JobResult {
                    path: job_path.clone(),
                    revision: job_revision,
                    pdf_tmp: job_pdf_tmp.clone(),
                    pdf_final: job_pdf_final.clone(),
                    duration_ms: elapsed,
                    stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                    success: out.status.success(),
                },
                Err(e) => JobResult {
                    path: job_path.clone(),
                    revision: job_revision,
                    pdf_tmp: job_pdf_tmp.clone(),
                    pdf_final: job_pdf_final.clone(),
                    duration_ms: elapsed,
                    stdout: String::new(),
                    stderr: format!("Erreur de lancement de typst : {e}"),
                    success: false,
                },
            };
            let _ = tx.send(result);
        });
        let default_snapshot = self.default_snapshot(&path);
        let snapshot = self
            .snapshots
            .entry(path.clone())
            .or_insert(default_snapshot);
        snapshot.state = RenderState::Running;
        snapshot.last_revision = revision;
        snapshot.engine_available = true;
        snapshot.engine_version = self.engine_version.clone();
        snapshot.source = Some(path.clone());
        snapshot.pdf = Some(pdf_final);
        self.active = Some(job);
        let snapshot_clone = snapshot.clone();
        self.write_status_file(&snapshot_clone);
    }

    fn finish_job(&mut self, result: JobResult) {
        if let Some(active) = &self.active {
            if active.path != result.path || active.revision != result.revision {
                let _ = std::fs::remove_file(&result.pdf_tmp);
                return;
            }
        } else {
            let _ = std::fs::remove_file(&result.pdf_tmp);
            return;
        }
        let default_snapshot = self.default_snapshot(&result.path);
        let snapshot = self
            .snapshots
            .entry(result.path.clone())
            .or_insert(default_snapshot);
        snapshot.engine_available = self.engine_available;
        snapshot.engine_version = self.engine_version.clone();
        snapshot.last_revision = result.revision;
        snapshot.last_duration_ms = Some(result.duration_ms);
        snapshot.last_stdout = result.stdout.clone();
        snapshot.last_stderr = result.stderr.clone();
        snapshot.last_error = parse_error(&result.stderr, &result.path);
        snapshot.source = Some(result.path.clone());
        snapshot.pdf = Some(result.pdf_final.clone());
        if result.success {
            if let Some(parent) = result.pdf_final.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let rename_res = std::fs::rename(&result.pdf_tmp, &result.pdf_final)
                .or_else(|_| std::fs::copy(&result.pdf_tmp, &result.pdf_final).map(|_| ()));
            if rename_res.is_err() {
                snapshot.state = RenderState::Error;
                snapshot.last_stderr = format!(
                    "Compilation Typst réussie mais impossible d'installer le PDF final {}.",
                    result.pdf_final.display()
                );
            } else {
                snapshot.state = RenderState::Success;
                snapshot.last_success_revision = result.revision;
                snapshot.needs_initial_render = false;
            }
            let _ = std::fs::remove_file(&result.pdf_tmp);
        } else {
            snapshot.state = RenderState::Error;
            snapshot.needs_initial_render = false;
            let _ = std::fs::remove_file(&result.pdf_tmp);
        }
        let snapshot_clone = snapshot.clone();
        self.write_status_file(&snapshot_clone);
        self.active = None;
    }

    fn write_status_file(&self, snapshot: &TypstSnapshot) {
        if let Some(parent) = self.status_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = ron::ser::to_string_pretty(snapshot, ron::ser::PrettyConfig::default()) {
            if let Err(e) = atomic_write(&self.status_file, raw.as_bytes()) {
                tracing::warn!(target: "editor", "Statut Typst non écrit : {e}");
            }
        }
    }
}

fn probe_engine() -> (bool, Option<String>) {
    let output = Command::new("typst").arg("--version").output();
    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let version = if version.is_empty() {
                None
            } else {
                Some(version)
            };
            (true, version)
        }
        _ => (false, None),
    }
}

fn parse_error(stderr: &str, source: &Path) -> Option<ErrorLocation> {
    let loc_re = Regex::new(r"(?m)(?P<file>[^\s:]+):(?P<line>\d+):(?P<col>\d+)").ok()?;
    let caps = loc_re.captures(stderr)?;
    let file = caps
        .name("file")
        .map(|m| PathBuf::from(m.as_str()))
        .unwrap_or_else(|| source.to_path_buf());
    let line = caps
        .name("line")
        .and_then(|m| m.as_str().parse::<usize>().ok());
    let column = caps
        .name("col")
        .and_then(|m| m.as_str().parse::<usize>().ok());
    let message = stderr
        .lines()
        .find(|l| l.starts_with("error:"))
        .unwrap_or(stderr.lines().next().unwrap_or("Erreur Typst"))
        .to_string();
    Some(ErrorLocation {
        file,
        line,
        column,
        message,
    })
}

fn is_typst_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("typ"))
        .unwrap_or(false)
}

fn project_root_for(path: &Path) -> PathBuf {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir.join("templates").join("engram.typ").exists() {
            return dir.to_path_buf();
        }
        current = dir.parent();
    }
    path.parent().unwrap_or(path).to_path_buf()
}

fn hash_path(path: &Path) -> String {
    let hash = xxhash_rust::xxh3::xxh3_64(path.to_string_lossy().as_bytes());
    format!("{hash:016x}")
}

fn spawn_viewer(cmd: &str, target: &Path) -> Result<(), String> {
    Command::new(cmd)
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            format!(
                "Ouverture de {} avec {cmd} impossible : {e}",
                target.display()
            )
        })
}

fn spawn_viewer_dir(target: PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(not(target_os = "macos"))]
    let cmd = "xdg-open";
    Command::new(cmd)
        .arg(&target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Ouverture de {} impossible : {e}", target.display()))
}
