# Rapport corrections post-audit

## Commit de départ
`68f3159`

## Branche
`depsweep/no-xml-touch`

## Fichiers modifiés
- `core/src/fs.rs`
- `core/src/lib.rs`
- `core/src/module_api.rs`
- `core/src/licorne.rs`
- `core/src/config.rs`
- `app/src/main.rs`
- `config/engram.ron`
- `docs/MANUEL_UTILISATEUR.md`
- `modules/editor/src/buffer.rs`
- `modules/editor/src/input.rs`
- `modules/editor/src/lib.rs`
- `modules/editor/src/snippets.rs`
- `modules/editor/src/typst_render.rs`
- `modules/file_tree/src/indexer.rs`
- `modules/file_tree/src/layout.rs`
- `modules/file_tree/src/project.rs`
- `modules/file_tree/src/symlinks.rs`
- `modules/sticky_notes/Cargo.toml`
- `modules/sticky_notes/src/config.rs`
- `modules/sticky_notes/src/db.rs`
- `modules/sticky_notes/src/lib.rs`
- `docs/reports/POST_AUDIT_FIX_REPORT.md`

## Correctifs réalisés
- Ajout d’un helper d’écriture atomique partagé dans `core/src/fs.rs`, réutilisé pour les sauvegardes critiques.
- Passage de la sauvegarde du buffer éditeur, de la session éditeur, du fichier de statut Typst, des snippets, de `engram.ron`, des projets et des symlinks vers l’écriture atomique.
- Stabilisation SQLite sur `file_tree` et `sticky_notes` avec `WAL` et `busy_timeout` cohérents.
- Remontée des erreurs SQLite critiques au lieu de les avaler silencieusement.
- Activation réelle de `sticky_notes` dans `config/engram.ron` et dans le défaut de `ModulesConfig`.
- UUID `sticky_notes` rendu robuste avec `getrandom` et échec explicite.
- Préservation LF/CRLF et du newline final pour les marqueurs Sticky Notes.
- Heartbeat de repaint du cockpit limité aux cas où la fenêtre principale est minimisée et qu’un viewport enfant est actif.
- Mutation Sticky Notes branchée sur le buffer éditeur déjà ouvert, avec fallback disque atomique uniquement quand le fichier n’est pas ouvert.
- Documentation utilisateur mise à jour pour `sticky_notes`, l’écriture atomique et la limitation du heartbeat cockpit.

## Correctifs non réalisés
- Validation interactive GUI longue du cockpit et de `sticky_notes`.
- Nettoyage des lints préexistants hors périmètre dans `cockpit`, `claude_terminal`, `editor` et `file_tree`.

## Raisons
- Les lints `clippy` bloquants observés hors lot modifié sont déjà présents dans l’arbre et ne sont pas dus à cette passe.

## Tests ajoutés
- `core/src/fs.rs`
  - écriture atomique normale et remplacement d’un fichier existant
  - erreur sans suppression du fichier cible
- `modules/editor/src/lib.rs`
  - buffer ouvert muté via l’API normale avec transaction undo unique
  - buffer ouvert laisse le fichier disque intact
  - fallback disque préserve CRLF et newline final
- `modules/sticky_notes/src/db.rs`
  - round-trip insertion/suppression de marqueur
  - préservation CRLF et newline final
  - génération UUID v4

## Commandes exécutées
- `cargo fmt --all -- --check`
- `CC=/usr/bin/gcc cargo check --workspace`
- `CC=/usr/bin/gcc cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `CC=/usr/bin/gcc cargo test --workspace --all-targets --all-features`
- `CC=/usr/bin/gcc cargo test -p sticky_notes --lib`

## Résultats exacts
- `cargo fmt --all -- --check` : PASS
- `cargo check --workspace` avec `CC=/usr/bin/gcc` : PASS
- `cargo test --workspace --all-targets --all-features` avec `CC=/usr/bin/gcc` : PASS après relance escaladée; un premier essai a échoué sur le test IPC avec `Operation not permitted` dans le sandbox
- `cargo test -p sticky_notes --lib` avec `CC=/usr/bin/gcc` : PASS
- `cargo test -p engram_core --lib` avec `CC=/usr/bin/gcc` : PASS
- `cargo test -p editor --lib` avec `CC=/usr/bin/gcc` : PASS
- `cargo clippy -p engram_core -p editor -p sticky_notes -p engram_hive --all-targets --all-features -- -D warnings` : FAIL, bloqué par des lints préexistants hors périmètre dans `cockpit`, `claude_terminal`, `editor`, `file_tree` et `sticky_notes/src/render.rs`

## Test fonctionnel
- Non exécuté en session GUI interactive.
- La validation a couvert les chemins de code et les tests unitaires/intégration Rust.

## État du cockpit
- Le repaint permanent est supprimé hors cas nécessaire.
- Le heartbeat ne se maintient que lorsque la fenêtre principale est minimisée et qu’au moins un viewport enfant est actif.
- Vérification interactive GUI non réalisée dans cette session.

## État de sticky_notes
- Module chargé par défaut via `engram.ron`.
- Si le fichier source est déjà ouvert dans l’éditeur, le marqueur est muté dans le buffer partagé avec une seule transaction undo.
- Si le fichier source n’est pas ouvert, le marqueur est écrit atomiquement en préservant LF/CRLF et le newline final.
- Les fins de ligne LF/CRLF sont préservées.
- Les UUID ne dépendent plus d’une lecture silencieusement ignorée de `/dev/urandom`.

## État SQLite
- `file_tree` et `sticky_notes` appliquent `WAL` et un `busy_timeout` cohérent.
- Les erreurs transactionnelles critiques remontent au lieu d’être avalées.
- La base partagée reste `index.db` pour cette mission.

## Risques restants
- Les lints `clippy` du workspace restent non verts sur des fichiers hors lot.
- L’état du cockpit n’a pas été retesté en interaction GUI réelle.

## Recommandation avant Dockmaster
- Faire une courte session GUI de validation sur `sticky_notes` et le cockpit.
- Décider si les lints hors périmètre doivent être purgés avant tout nouveau chantier.
- Ne pas démarrer Dockmaster tant que la validation interactive n’a pas confirmé le comportement attendu.
