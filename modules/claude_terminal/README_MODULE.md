# claude_terminal

## Quoi ?

Fenêtre chat Q&A invoquant le CLI `claude` en mode one-shot (`--print`). Remplace COH2B. Lecture seule du projet — Claude Code peut lire et analyser mais jamais écrire dans les fichiers du projet.

## Comment ?

Invocation via `std::process::Command` sur un thread dédié (`std::thread::spawn` + `mpsc::channel`). La sortie JSON est parsée pour extraire le texte et les tokens consommés. L'UI est un chat egui dans un viewport propre (fenêtre OS dédiée). Chaque échange est logué dans un fichier `.md` horodaté. Les tokens sont comptabilisés en JSONL.

## Comment virer ?

1. Supprimer `modules/claude_terminal/` du repo.
2. Retirer `"modules/claude_terminal"` de `[workspace] members` dans `Cargo.toml`.
3. Retirer `claude_terminal` de `[dependencies]` dans `app/Cargo.toml`.
4. Retirer `registry.register("claude_terminal", ...)` dans `app/src/main.rs`.
5. Retirer la catégorie `ClaudeTerminal` du cockpit (`modules/cockpit/src/lib.rs` + `claude_terminal_form.rs`).

## Dépendances

- `engram_core` (Module trait, CoreContext, CoreEvent, ModuleResponse)
- `egui` (UI)
- `serde`, `serde_json`, `ron` (sérialisation config et réponses CLI)
- `tracing` (logging)
- `chrono` (horodatage)
- `dirs` (chemins data/config)
