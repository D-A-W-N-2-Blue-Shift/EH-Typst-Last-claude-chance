# TYPST_LAB_DELIVERY_REPORT

## Résultat global

Le fork `EH-Typst-Labs` utilise maintenant Typst comme moteur de rendu externe
intégré à l’éditeur. Le code compile, la suite de tests passe, et les fichiers
de démonstration Typst compilent avec un root de projet partagé.

## Ce qui est livré

- moteur Typst externe branché dans `modules/editor/src/typst_render.rs`
- intégration éditeur pour :
  - sauvegarde suivie d’un rendu Typst
  - rendu différé après modification
  - ouverture dans Kate
  - ouverture du dernier PDF valide dans Okular
  - ouverture du dossier de rendu
  - saut vers ligne/colonne d’erreur
- séparation des dossiers applicatifs en `_typst`
- dossier de démonstration `demo_typst/`
- fiche étalon Svetlana convertie en Typst
- chapitres réels convertis en Typst
- doc Stream Deck `docs/TYPST_STREAMDECK_KEYS.md`

## Preuves exécutées

### 1. Version Typst

Commande:

```bash
typst --version
```

Résultat:

```text
typst 0.15.0 (unknown commit)
```

### 2. Méthode de rendu

Méthode utilisée par le fork:

- CLI externe `typst`
- pas de crate Typst embarquée

### 3. Fichier qui lance le rendu Typst

Fichier:

- [`modules/editor/src/typst_render.rs`](/home/azot/Sharashkas-B/EH-Typst-Labs/modules/editor/src/typst_render.rs)

Point d’entrée:

- `start_job()`
- commande `typst compile --root <root> <source> <pdf_tmp>`

### 4. Ouverture, édition, sauvegarde d’un `.typ`

État observé:

- le pipeline d’éditeur est branché sur les actions IPC et les raccourcis
- la sauvegarde déclenche un rendu Typst pour les fichiers `.typ`

Statut de vérification:

- `PASS` sur le chemin de code
- `NOT EXECUTED` en interaction GUI dans cette session

### 5. Rendu d’un `.typ` valide

Commande:

```bash
typst compile --root demo_typst demo_typst/roman/chapitre_charge.typ /tmp/eh_typst_check/chapitre_charge_measure.pdf
```

Résultat:

- `PASS`
- durée mesurée: `0.08 s`
- CPU: `100%`
- RSS max: `30940 KB`

### 6. Rendu d’un `.typ` invalide

Commande:

```bash
typst compile /tmp/eh_typst_check/invalid.typ /tmp/eh_typst_check/invalid.pdf
```

Résultat:

```text
error: unclosed delimiter
  ┌─ ../../../../tmp/eh_typst_check/invalid.typ:4:6
  │
4 │ #table(columns: 2, [A], [B]
  │       ^
```

Statut:

- `PASS` pour l’erreur ligne/colonne
- `PASS` pour l’affichage lisible
- `NOT EXECUTED` dans l’UI interactive de cette session

### 7. Chapitre réel

Fichier:

- [`demo_typst/roman/chapitre_reel.typ`](/home/azot/Sharashkas-B/EH-Typst-Labs/demo_typst/roman/chapitre_reel.typ)

Validation:

- `PASS` avec `typst compile --root demo_typst ...`

### 8. Fiche massive avec tableaux

Fichier:

- [`demo_typst/fiches/personnage_massif.typ`](/home/azot/Sharashkas-B/EH-Typst-Labs/demo_typst/fiches/personnage_massif.typ)

Validation:

- `PASS` avec `typst compile --root demo_typst ...`

### 9. Gros fichier

Fichier:

- [`demo_typst/roman/chapitre_charge.typ`](/home/azot/Sharashkas-B/EH-Typst-Labs/demo_typst/roman/chapitre_charge.typ)

Validation:

- `PASS`
- durée mesurée: `0.08 s`
- CPU: `100%`
- RSS max: `30940 KB`

### 10. CPU au repos et pendant rendu

Mesures observées:

- rendu: `100%` sur `typst compile`
- repos: `8.0%` sur `typst watch` juste après démarrage

### 11. Mémoire

Mesure observée pendant rendu:

- `30940 KB` de RSS max

### 12. Dossiers séparés

État du code:

- `~/.config/engram_hive_typst/`
- `~/.local/share/engram_hive_typst/`

Validation:

- `PASS` dans `core/src/module_api.rs`
- `PASS` dans `modules/file_tree/src/lib.rs`

### 13. Projet Markdown original

Constat:

- aucun fichier du projet Markdown original n’a été modifié dans ce fork
- les changements sont confinés au dépôt `EH-Typst-Labs`

### 14. Commit `220ae17`

Le commit `220ae17` reste un jalon historique. Le fork courant le dépasse
désormais avec le moteur Typst branché et le dossier de démo livré.

### 15. `modules/cockpit/src/md_mirror.rs`

Rôle actuel:

- miroir de documentation/configuration en `.typ`

Pourquoi le nom Markdown subsiste:

- le module conserve un nom hérité de la base EH5

Produit-il encore du Markdown ?

- non, le miroir manipulé ici est en `.typ`

### 16. Boutons et actions

Actions branchées dans le code:

- ouvrir dans Kate
- ouvrir dans Okular
- relancer le rendu
- aller à l’erreur
- ouvrir le dossier de rendu

Statut:

- `PASS` sur le câblage du code
- `NOT EXECUTED` en interaction GUI dans cette session

### 17. DB2

Constat:

- DB2 n’a pas été touchée

## Vérifications Rust

- `cargo fmt --all` : `PASS`
- `CC=/usr/bin/gcc cargo check -q` : `PASS`
- `CC=/usr/bin/gcc cargo test --all-targets --all-features -q` : `PASS`

## Vérifications Typst

- `typst --version` : `PASS`
- `typst compile --root demo_typst demo_typst/README.typ ...` : `PASS`
- `typst compile --root demo_typst demo_typst/fiches/personnage_massif.typ ...` : `PASS`
- `typst compile --root demo_typst demo_typst/fiches/tableau_complexe.typ ...` : `PASS`
- `typst compile --root demo_typst demo_typst/roman/chapitre_reel.typ ...` : `PASS`
- `typst compile --root demo_typst demo_typst/roman/chapitre_charge.typ ...` : `PASS`
- `typst compile /tmp/eh_typst_check/invalid.typ ...` : `PASS` sur l’erreur attendue

## Limites restantes

- l’UI complète n’a pas été lancée dans cette session
- les actions Kate/Okular n’ont pas été exécutées en interaction graphique ici
- la mesure `typst watch` est un relevé ponctuel, pas une longue session

