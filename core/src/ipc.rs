// ============================================================================
// core/src/ipc.rs — Socket Unix IPC d'Engram_Hive
//
// Un seul socket pour toute l'app : ~/.local/share/engram_hive/engram.sock
//
// Côté serveur (la GUI) :
//   - IpcServer::bind() écoute dans un thread dédié, parse chaque ligne JSON
//     en IpcCommand et la pousse dans un channel mpsc.
//   - Le binaire draine ce channel à chaque frame et rediffuse les commandes
//     aux modules via CoreEvent::IpcCommand. Le core ne sait PAS ce que
//     "zoom" veut dire — c'est l'affaire du module destinataire.
//   - "ping" reçoit "pong" : c'est le test de vie utilisé au démarrage.
//
// Côté client (le CLI) :
//   - send_command() se connecte, envoie une ligne JSON, lit la réponse.
//   - instance_alive() = ping. Socket présent mais mort (crash) → false,
//     et le serveur suivant écrase le fichier socket orphelin.
//
// Format des commandes (une ligne JSON par commande) :
//   {"module": "editor", "action": "zoom", "value": 150}
//   {"module": "editor", "action": "open", "path": "/chemin/fichier.typ"}
// ============================================================================

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

/// Une commande IPC adressée à un module. `value` reste un JSON brut :
/// nombre pour "zoom", chaîne pour "toggle", absent pour "save" — chaque
/// module interprète ce qui le concerne.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IpcCommand {
    pub module: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// Chemin du socket : ~/.local/share/engram_hive/engram.sock.
pub fn socket_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("engram.sock")
}

/// Le serveur IPC : vit aussi longtemps que la GUI. Drop = socket supprimé.
pub struct IpcServer {
    pub rx: Receiver<IpcCommand>,
    path: PathBuf,
}

impl IpcServer {
    /// Écoute sur le socket. Si un fichier socket orphelin traîne (crash
    /// précédent) et ne répond pas au ping, il est écrasé.
    pub fn bind(data_dir: &std::path::Path) -> Result<Self, String> {
        let path = socket_path(data_dir);
        if path.exists() {
            if instance_alive(data_dir) {
                return Err(format!(
                    "Une instance d'Engram_Hive tourne déjà (socket {} actif). \
                     Deux ruches sur le même socket, ça finit mal.",
                    path.display()
                ));
            }
            // Socket orphelin d'un crash : on nettoie sans état d'âme.
            let _ = std::fs::remove_file(&path);
        }
        let listener = UnixListener::bind(&path)
            .map_err(|e| format!("Impossible d'écouter sur {} : {e}", path.display()))?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("ipc-server".into())
            .spawn(move || accept_loop(listener, tx))
            .map_err(|e| format!("Impossible de lancer le thread IPC : {e}"))?;
        Ok(Self { rx, path })
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Boucle d'acceptation : une connexion = une ou plusieurs lignes JSON.
/// Tourne jusqu'à la mort du process (le listener n'est jamais fermé à la
/// main : Drop du serveur supprime le fichier socket et le process sort).
fn accept_loop(listener: UnixListener, tx: Sender<IpcCommand>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let tx = tx.clone();
        // Un thread par connexion : les clients CLI vivent < 1ms, le coût
        // est négligeable et un client lent ne bloque pas les autres.
        std::thread::spawn(move || handle_client(stream, tx));
    }
}

fn handle_client(stream: UnixStream, tx: Sender<IpcCommand>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "ping" {
            let _ = writeln!(writer, "pong");
            continue;
        }
        match serde_json::from_str::<IpcCommand>(line) {
            Ok(cmd) => {
                tracing::info!(target: "ipc", "Commande reçue : {cmd:?}");
                let ok = tx.send(cmd).is_ok();
                let _ = writeln!(writer, "{}", if ok { "ok" } else { "err: core parti" });
            }
            Err(e) => {
                let _ = writeln!(writer, "err: JSON invalide ({e})");
            }
        }
    }
}

/// Une instance GUI répond-elle sur le socket ?
pub fn instance_alive(data_dir: &std::path::Path) -> bool {
    let path = socket_path(data_dir);
    let Ok(stream) = UnixStream::connect(&path) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return false,
    };
    if writeln!(writer, "ping").is_err() {
        return false;
    }
    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line).is_ok() && line.trim() == "pong"
}

/// Envoie une commande à l'instance qui tourne. Retourne la réponse brute.
pub fn send_command(data_dir: &std::path::Path, cmd: &IpcCommand) -> Result<String, String> {
    let path = socket_path(data_dir);
    let stream = UnixStream::connect(&path).map_err(|e| {
        format!(
            "Aucune instance d'Engram_Hive ne répond sur {} ({e}). \
             Lance la GUI d'abord — je ne peux pas zoomer dans le vide.",
            path.display()
        )
    })?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
    writeln!(writer, "{json}").map_err(|e| format!("Envoi IPC raté : {e}"))?;
    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut line)
        .map_err(|e| format!("Pas de réponse de l'instance : {e}"))?;
    let resp = line.trim().to_string();
    if resp.starts_with("err") {
        Err(resp)
    } else {
        Ok(resp)
    }
}
