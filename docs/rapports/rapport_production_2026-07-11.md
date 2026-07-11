# Rapport de production — 2026-07-11

## Résultat global
Le fork `EH-Typst-Labs` est converti en Typst comme format documentaire principal. La compilation et les tests automatisés sont validés, mais les preuves fonctionnelles d’usage réel restent partielles ou absentes pour plusieurs points demandés.

## Correction de formulation
La mention précédente "validé localement" est remplacée par "compilation et tests automatisés validés".

## Preuves fonctionnelles

### 1. `typst --version`
Commande exécutée:

```bash
typst --version
```

Résultat:

```text
typst 0.15.0 (unknown commit)
```

### 2. Méthode de rendu utilisée
Méthode réellement vérifiée dans cette session:
- CLI externe `typst`
- pas de crate Typst embarquée observée dans le code de ce fork

### 3. Fichier exact qui lance le rendu Typst
Aucun fichier du commit `220ae17` ne lance encore `typst compile` ou `typst watch` dans l'application.

Constat de code:
- [`app/src/main.rs`](/home/azot/Sharashkas-B/EH-Typst-Labs/app/src/main.rs) contient le chemin CLI de l'éditeur via IPC, mais pas de moteur Typst.
- [`modules/cockpit/src/md_mirror.rs`](/home/azot/Sharashkas-B/EH-Typst-Labs/modules/cockpit/src/md_mirror.rs) génère un miroir `.typ`, mais ne compile pas le document.

Conclusion: le rendu Typst est aujourd'hui validé comme outil externe, pas encore intégré comme pipeline applicatif.

### 4. Test d'ouverture, édition et sauvegarde d'un `.typ`
Preuve de code, pas de test GUI interactif réalisé dans cette session:
- [`app/src/main.rs:664`](/home/azot/Sharashkas-B/EH-Typst-Labs/app/src/main.rs#L664) expose `cli_editor("open" ...)` et `cli_editor("save" ...)` via IPC.
- [`modules/editor/src/lib.rs:446`](/home/azot/Sharashkas-B/EH-Typst-Labs/modules/editor/src/lib.rs#L446) traite les actions IPC `open` et `save`.

Statut:
- ouverture: attestée par le code
- édition: attestée par le code de l'éditeur, pas par un test utilisateur GUI dans cette session
- sauvegarde: attestée par le code, pas par un test GUI dans cette session

### 5. Test de rendu d'un `.typ` valide
Commande exécutée:

```bash
typst compile /tmp/typst_lab_probe/valid.typ /tmp/typst_lab_probe/valid.pdf
```

Résultat:
- exit status `0`
- PDF produit localement
- temps mesuré:
  - user `0.06 s`
  - system `0.02 s`
  - elapsed `0.08 s`
- CPU mesuré: `101%`
- mémoire max: `31,216 KB`

### 6. Test d'un `.typ` invalide
Commande exécutée:

```bash
typst compile /tmp/typst_lab_probe/invalid.typ /tmp/typst_lab_probe/invalid.pdf
```

Résultat:

```text
error: unclosed delimiter
  ┌─ ../../../tmp/typst_lab_probe/invalid.typ:3:6
  │
3 │ #table(
  │       ^
```

Constats:
- l'erreur pointe ligne `3`, colonne `6`
- l'éditeur n'a pas été bloqué dans cette session, mais ce comportement n'a pas été démontré en interaction GUI
- le dernier rendu valide n'a pas été observé dans l'UI, donc ce point reste non attesté

### 7. Test d'un chapitre réel
Non exécuté dans cette session.

La preuve disponible est seulement un document de test synthétique:
- fichier de travail `/tmp/typst_lab_probe/valid.typ`

### 8. Test d'une fiche massive avec tableaux
Non exécuté dans cette session.

### 9. Test d'un gros fichier
Non exécuté dans cette session.

### 10. Mesure CPU au repos et pendant rendu
Mesure pendant rendu:
- `101%` sur `typst compile` du fichier valide

Mesure au repos:
- `typst watch /tmp/typst_lab_probe/valid.typ /tmp/typst_lab_probe/valid.pdf`
- snapshot `ps` pendant veille: `4.0% CPU`

### 11. Mesure mémoire
Mesure observée pendant rendu:
- `31,216 KB` de RSS max sur `typst compile` du fichier valide

### 12. Vérification des dossiers séparés
Constat de code:
- [`core/src/module_api.rs`](/home/azot/Sharashkas-B/EH-Typst-Labs/core/src/module_api.rs#L154) crée encore:
  - `~/.config/engram_hive/`
  - `~/.local/share/engram_hive/`

Conclusion:
- les dossiers séparés `~/.config/engram_hive_typst/` et `~/.local/share/engram_hive_typst/` ne sont pas implémentés dans le code actuel
- la vérification demandée échoue donc pour l’instant

### 13. Vérification qu'aucun fichier du projet Markdown original n'a été modifié
Dans ce dépôt et dans le commit `220ae17`, aucun chemin du projet Markdown original n'apparaît.

Preuve disponible:
- la liste complète des fichiers du commit ne contient que le fork `EH-Typst-Labs`

Limite:
- le projet Markdown original n'est pas monté dans ce workspace, donc la vérification est limitée au contenu du commit et du checkout courant

### 14. Liste complète des fichiers du commit `220ae17`
```text
.gitignore
ARCHITECTURE.md
Architecture_des_fichiers.md
BRIEF CODEX — EH5 v0.5.0 CORRECT.md
Cargo.lock
Cargo.toml
Doc/EH5_CLEANUP.md
LICENSE
Template_Fiches perso /template_concept.md
Template_Fiches perso /template_document_interne.md
Template_Fiches perso /template_entite.md
Template_Fiches perso /template_fiche_st.md
Template_Fiches perso /template_fragment_lore.md
Template_Fiches perso /template_lieu.md
Template_Fiches perso /template_objet.md
Template_Fiches perso /template_organisation.md
Template_Fiches perso /template_personnage.md
app/Cargo.toml
app/src/assets.rs
app/src/main.rs
app/src/module_cli.rs
app/src/palette.rs
assets/README.md
assets/core_bg.png
assets/icon.png
config/engram.ron
config/licorne-a-gerber.ron
core/Cargo.toml
core/src/backup.rs
core/src/config.rs
core/src/ipc.rs
core/src/lib.rs
core/src/licorne.rs
core/src/module_api.rs
core/src/theme.rs
core/tests/ipc.rs
docs/rapports/rapport_session_2026-07-09.md
install/engram_hive.desktop
install/install.sh
modules/claude_terminal/Cargo.toml
modules/claude_terminal/README_MODULE.md
modules/claude_terminal/src/config.rs
modules/claude_terminal/src/lib.rs
modules/claude_terminal/src/process.rs
modules/claude_terminal/src/render.rs
modules/claude_terminal/src/token_log.rs
modules/cockpit/Cargo.toml
modules/cockpit/src/claude_terminal_form.rs
modules/cockpit/src/lib.rs
modules/cockpit/src/md_mirror.rs
modules/cockpit/src/theme_form.rs
modules/editor/Cargo.toml
modules/editor/README_MODULE.md
modules/editor/config/editor.ron
modules/editor/src/buffer.rs
modules/editor/src/config.rs
modules/editor/src/focus_mode.rs
modules/editor/src/highlight.rs
modules/editor/src/input.rs
modules/editor/src/lib.rs
modules/editor/src/search.rs
modules/editor/src/snippets.rs
modules/editor/src/stats.rs
modules/editor/src/table_dialog.rs
modules/editor/src/table_edit_assist.rs
modules/editor/src/toc.rs
modules/editor/src/typography.rs
modules/editor/src/viewport.rs
modules/editor/src/wikilinks.rs
modules/editor/tests/focus_surrender.rs
modules/file_tree/Cargo.toml
modules/file_tree/README_MODULE.md
modules/file_tree/src/config.rs
modules/file_tree/src/context_menu.rs
modules/file_tree/src/folder_picker.rs
modules/file_tree/src/indexer.rs
modules/file_tree/src/layout.rs
modules/file_tree/src/lib.rs
modules/file_tree/src/palette.rs
modules/file_tree/src/project.rs
modules/file_tree/src/stats.rs
modules/file_tree/src/symlinks.rs
modules/file_tree/src/temporal.rs
modules/file_tree/src/tree_view.rs
modules/file_tree/tests/focus_input.rs
modules/timeline/Cargo.toml
modules/timeline/src/lib.rs
```

### 15. Explication de `modules/cockpit/src/md_mirror.rs`
Rôle actuel:
- générer un miroir documentaire de `theme.ron` au format Typst
- écrire `theme.typ` à côté de la configuration

Pourquoi le nom Markdown subsiste:
- le nom du fichier source `md_mirror.rs` est resté historique
- le code a été renommé en partie, mais le nom du module n'a pas encore été harmonisé partout

Produit-il encore du Markdown ?
- non
- le fichier génère actuellement du Typst dans `theme.typ`

### 16. Test des boutons ou actions
Dans le code actuel, les actions présentes et vérifiables sont:
- `Ouvrir fichier`
- `Ouvrir dossier`
- `Recharger depuis disque`
- `Relancer app`

Preuve:
- [`modules/cockpit/src/lib.rs:523`](/home/azot/Sharashkas-B/EH-Typst-Labs/modules/cockpit/src/lib.rs#L523)
- [`modules/cockpit/src/lib.rs:609`](/home/azot/Sharashkas-B/EH-Typst-Labs/modules/cockpit/src/lib.rs#L609)

Actions demandées mais non présentes dans le code actuel:
- ouvrir dans Kate
- ouvrir dans Okular
- relancer le rendu Typst

Conclusion:
- ces actions ne sont pas encore implémentées comme boutons dédiés
- l'ouverture système générale existe, mais Kate/Okular ne sont pas câblés spécifiquement

### 17. Confirmer que DB2 n'a pas été touchée
Vérification par liste de commit:
- `DBv2-EH4.md` n'apparaît pas dans le commit `220ae17`
- aucun fichier DB2 n’a été modifié dans ce commit

## Validations automatisées
- `cargo fmt --all`
- `CC=/usr/bin/gcc cargo check -q`
- `CC=/usr/bin/gcc cargo test --all-targets --all-features -q`

## Résultat des tests
- compilation validée
- tests automatisés validés
- un premier passage des tests a échoué dans le sandbox sur un socket local, puis un rerun hors sandbox a validé la suite complète

## Fichiers modifiés dans cette phase
- [`docs/rapports/rapport_production_2026-07-11.md`](/home/azot/Sharashkas-B/EH-Typst-Labs/docs/rapports/rapport_production_2026-07-11.md)

## Commit local
- `220ae17` — `Convert EH-Typst-Labs to Typst`

## Conclusion opérationnelle
Le fork est bien converti et la partie CLI Typst est démontrée. En revanche, plusieurs exigences fonctionnelles restent non attestées par des tests d’usage réel dans cette session:
- rendu intégré dans l’application,
- ouverture/édition/sauvegarde validées en interaction GUI,
- conservation du dernier rendu valide dans l’UI,
- dossiers `_typst` séparés,
- boutons dédiés Kate/Okular,
- tests sur chapitres réels et fichiers massifs.

La bonne formulation actuelle est donc:
- `compilation et tests automatisés validés`
- pas `validé localement` au sens fonctionnel complet
