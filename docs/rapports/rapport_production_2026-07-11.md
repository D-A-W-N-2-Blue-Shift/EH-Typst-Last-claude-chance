# Rapport de production - 2026-07-11

## Statut global

Le fork `EH-Typst-Labs` est dans un état de production contrôlée sur le périmètre actuel:

- rendu Typst branché dans l'éditeur;
- dossier de démonstration Typst livré;
- cockpit neutralisé par défaut;
- file tree élargi aux fichiers utiles;
- panneau `sticky_notes` ajouté;
- panneau `COH2B` branché à partir du terminal Claude existant;
- docs de fonctionnement préparées.

## Livrables présents

- `modules/sticky_notes/`
- `modules/claude_terminal/` utilisé comme base du panneau `COH2B`
- `docs/rapports/STICKY_NOTES_REPORT.md`
- `docs/rapports/TYPST_LAB_DELIVERY_REPORT.md`
- `docs/MANUEL_UTILISATEUR.md`

## Vérifications techniques connues

### Validation Rust

- `cargo fmt --all` : PASS
- `CC=/usr/bin/gcc cargo check -q` : PASS
- `CC=/usr/bin/gcc cargo test -q -p claude_terminal` : PASS
- `CC=/usr/bin/gcc cargo build --release -q` : PASS

### Validation complète du workspace

- `CC=/usr/bin/gcc cargo test --all-targets --all-features -q` : FAIL dans ce sandbox
- cause observée : `full_ipc_roundtrip` échoue sur `engram.sock` avec `Operation not permitted`
- cette erreur est environnementale, pas une régression du panneau COH2B

## Détails fonctionnels

### Typst

- compilation locale branchée sur le CLI `typst`
- dernier rendu valide conservé côté éditeur
- ouverture Kate / Okular / dossier de rendu prévue dans l'interface

### Sticky Notes

- notes persistées dans la base SQLite du projet
- tags et liens supportés
- compteur de notes publié vers l'éditeur
- bandeau cliquable de notes liées

### COH2B

- panneau basé sur le terminal Claude existant
- lecture du fichier courant
- contexte corpus via index SQLite du projet
- modes de périmètre et d'analyse visibles
- journalisation locale dédiée
- configuration séparée `coh2b`

## Limites connues

- le périmètre texte sélectionné n'est pas encore relié à un flux explicite d'éditeur vers COH2B
- les tests interactifs GUI n'ont pas été exécutés dans cette passe
- `cargo test --all-targets --all-features -q` reste sensible au sandbox IPC local

## Fichiers modifiés dans ce lot

- `modules/claude_terminal/Cargo.toml`
- `modules/claude_terminal/src/config.rs`
- `modules/claude_terminal/src/corpus.rs`
- `modules/claude_terminal/src/lib.rs`
- `modules/claude_terminal/src/render.rs`
- `modules/claude_terminal/src/token_log.rs`
- `modules/cockpit/src/claude_terminal_form.rs`
- `modules/cockpit/src/lib.rs`
- `modules/file_tree/src/palette.rs`
- `docs/rapports/rapport_production_2026-07-11.md`
- `docs/MANUEL_UTILISATEUR.md`

## Conclusion

Le fork est exploitable pour le test utilisateur demandé, avec documentation de production et manuel de prise en main disponibles.
