// ============================================================================
// core/tests/ipc.rs — Test d'intégration du socket Unix IPC
//
// Vérifie le cycle complet : bind → ping (détection d'instance) → envoi
// d'une commande JSON → réception côté serveur → refus d'une seconde
// instance → nettoyage du socket au Drop.
// ============================================================================

use engram_core::ipc::{instance_alive, send_command, socket_path};
use engram_core::{IpcCommand, IpcServer};

#[test]
fn full_ipc_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let data_dir = dir.path();

    // Pas d'instance : pas de réponse au ping.
    assert!(!instance_alive(data_dir));

    let server = IpcServer::bind(data_dir)?;
    assert!(instance_alive(data_dir), "le serveur doit répondre pong");

    // Une seconde instance est refusée tant que la première vit.
    assert!(IpcServer::bind(data_dir).is_err());

    // Une commande traverse le socket et arrive typée côté serveur.
    let cmd = IpcCommand {
        module: "editor".into(),
        action: "zoom".into(),
        value: Some(serde_json::json!(150)),
        path: None,
    };
    let resp = send_command(data_dir, &cmd)?;
    assert_eq!(resp, "ok");
    let received = server.rx.recv_timeout(std::time::Duration::from_secs(2))?;
    assert_eq!(received.module, "editor");
    assert_eq!(received.action, "zoom");
    assert_eq!(received.value, Some(serde_json::json!(150)));

    // Drop du serveur = socket supprimé → plus d'instance.
    drop(server);
    assert!(!socket_path(data_dir).exists());
    assert!(!instance_alive(data_dir));
    Ok(())
}
