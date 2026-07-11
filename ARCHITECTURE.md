# Engram_Hive — Architecture

## Vue d'ensemble

Un **noyau** mince (`core`) + des **modules** isolés, câblés par le binaire
(`app`). Chaque module est un dossier sous `modules/`, implémente le trait
`Module`, et vit dans sa/ses propre(s) fenêtre(s) OS (viewport egui). Étanchéité
stricte :

- Un module ne touche jamais au core directement : il pousse des
  `ModuleResponse` dans le `Vec` fourni à chaque frame.
- Le core ne regarde jamais l'intérieur d'un module : il lui envoie des
  `CoreEvent`.
- Les modules ne se parlent jamais entre eux. Le core est l'arbitre (il relaie,
  p.ex. l'index des fichiers du file_tree vers l'éditeur).

```
core/   trait Module, ModuleResponse, CoreEvent, CoreContext, IPC, thème,
        registre + chargement de modules.ron
app/    binaire engram_hive : GUI (fenêtre core + modules) et CLI ; câble les
        modules dans le ModuleRegistry ; applique le thème global
modules/file_tree/  hub de navigation : sélecteur de projet intégré, arbre
        sémantique, index SQLite, command palette, table 06_session/ (§6 —
        06_en_cours/ toujours reconnu pour la compatibilité)
modules/editor/     éditeur de prose : 1 fichier = 1 fenêtre, buffer ropey
        partagé, coloration markdown, recherche, stats, snippets, wikilinks,
        grille de tableaux (§1 largeur = max contenu). Le formulaire
        frontmatter fiches personnage a été retiré (§2) : le YAML est édité
        directement dans le buffer, la coloration frontmatter reste appliquée.
modules/cockpit/    tableau électrique EH5 (config, thème, modules, provider CLI)
modules/timeline/   placeholder temporel en attente DB V2, désactivé par défaut
modules/claude_terminal/  assistant CLI à refondre plus tard, désactivé par défaut
tools/engram_inspect/  binaire séparé §3.5 : ouvre index.db en lecture
        seule, 4 onglets (Fichiers/Wikilinks/Tags/Événements) + intégrité
```

## État des modules

- **Core** — ✅ trait Module, bus de réponses, registre + `modules.ron`
  (résilient : un fichier illisible est sauvegardé en `.bak` et régénéré),
  IPC socket Unix, thème global (`theme.ron`).
- **File Tree** — ✅ ouverture/création de projet (sélecteur intégré egui,
  zéro dépendance portail), arbre + stats, index SQLite + FTS5, watcher
  notify, symlinks `06_en_cours/`, palette, session.ron, mode docked basique.
- **Éditeur** — ✅ fenêtres OS, buffer ropey + transactions groupées + dédup
  par inode, coloration markdown (cache d'états par ligne), zoom par fenêtre,
  focus/typewriter, recherche/remplacement en thread, stats + objectif,
  snippets, wikilinks + auto-complétion, smart typography, auto-close,
  formatage, IPC, session, rendu grille des tableaux, formulaire frontmatter
  fiches personnage.
- **Cockpit** — ✅ fenêtre OS dédiée, tableau électrique EH5 (config,
  thème, modules, provider CLI), miroir .md pédagogique.
- **Timeline** — ⛔ placeholder temporel en attente DB V2, désactivé par défaut.
- **Claude Terminal** — ⛔ assistant CLI à refondre plus tard, désactivé par défaut.

## Le contrat `ModuleResponse` (module → core)

| Variante | Rôle |
|---|---|
| `OpenFile(PathBuf)` | ouvrir un fichier (chemin canonique) |
| `CreateAndOpenFile { path }` | créer (wikilink orphelin) puis ouvrir |
| `PublishFileIndex { project_root, files }` | publier l'index `.md` (→ auto-complétion éditeur) |
| `WindowMoved { id, new_pos }` | la fenêtre a bougé (arbitrage docked) |
| `LayoutModeChanged(LayoutMode)` | bascule Independent ↔ Docked |
| `FocusModeChanged(bool)` | l'éditeur entre/sort du focus (→ file_tree se masque) |
| `Error { module, message }` | erreur GLaDOS (loguée + affichée) |

## Le contrat `CoreEvent` (core → module)

| Variante | Rôle |
|---|---|
| `WindowRepositioned { id, pos }` | repositionnement en mode docked |
| `OpenFileRequested(PathBuf)` | relais d'OpenFile/CreateAndOpenFile |
| `FileIndexUpdated { project_root, files }` | relais de PublishFileIndex |
| `FocusModeChanged(bool)` | relais du mode focus |
| `IpcCommand(IpcCommand)` | commande arrivée par le socket |
| `OpenModuleWindowRequested(String)` | ouvrir la fenêtre d'un module OwnViewport |

`CoreContext` (passé à `init`) : `config_dir`, `data_dir`, `theme: Palette`.

## Convention de nommage RON (non-négociable)

- `<module>.ron` — config simple, user-facing, créée au premier lancement.
- `engram.ron` — config experte unifiée cible EH5.
- `licorne-a-gerber.ron` — compatibilité legacy, conservée tant que nécessaire.

Idem pour le thème global : `theme.ron` + section `theme` dans `engram.ron`
(et compatibilité legacy dans `licorne-a-gerber.ron` si besoin).

Piège évité : RON vérifie le nom de struct à la relecture. Les structs portent
`#[serde(rename = "…")]` quand le nom de fichier diffère du nom Rust, et un
test de round-trip verrouille l'invariant (sinon : crash au 2e lancement).

## Emplacements

```
~/.config/engram_hive/
  modules.ron                         liste des modules activés (base EH5)
  engram.ron                          config cible unifiée EH5
  theme.ron                           thème simple
  licorne-a-gerber.ron                config experte UNIFIÉE (toutes sections)
  keybinds.ron                        raccourcis éditeur
  snippets/*.toml
  fonts/*.ttf
  modules/<module>/<module>.ron       config simple par module
  projects/<nom>/project.ron          config par projet (goal global)
~/.local/share/engram_hive/
  engram.sock                         socket IPC
  logs/modules/<module>.log
<projet>/.engram/index.db             index SQLite (local au projet)
<projet>/.engram/… + <projet> session.ron
```

## Nomenclature projet (§6 du brief 2/7/2026)

Structure de référence pour tous les nouveaux projets :

```
<projet>/
├── 01_architecture/
│   ├── plan/
│   ├── chronologie/
│   │   ├── evenements.md      un fichier global, sections ## par événement
│   │   └── biographies.md     un fichier global, sections ## par personnage
│   └── modifications.md
├── 02_worldbuilding/
│   ├── personnages/           prenom_nom.md (minuscules + underscore)
│   ├── lieux/
│   ├── concepts/
│   └── lore/
├── 05_texte/
│   ├── scenes/                CH_NNN_slug.md (ex: 01_001_palimpsestus.md)
│   └── chapitres/             chapitre_NN_slug.md
├── 06_session/                remplace 06_en_cours/ (nom historique)
│   ├── en_cours.md            symlink OU copie — les deux valides
│   ├── contexte.md
│   └── todo.md
└── 09_poubelle/               scènes coupées, visible, jamais .trash
```

Le file_tree reconnaît `06_session/` ET `06_en_cours/` (historique) —
mêmes règles d'exclusion des stats + coloration symlinks.

Le projet de test `test_projects/wingate_minimal/` sert de référence
concrète pour cette structure et pour la timeline (§3).

## Schéma SQLite de l'index (`<projet>/.engram/index.db`)

```sql
files(canonical_path PK, file_stem, section, word_count_body,
      word_count_yaml, character_count, goal_words, last_modified, date_marker)
wikilinks(source_path FK→files, target_name, is_orphan)
tags(file_path FK→files, tag_name)
fts_content USING fts5(canonical_path UNINDEXED, body_text)   -- recherche full-text
scenes(scene_path PK, date_sort, date_display, lieu, ordre)   -- matrice timeline
scene_personnages(scene_path FK→scenes, perso)
perso_chrono(perso_path FK→files, perso_name, date_sort, date_display, note)
-- §3.3 du brief 2/7/2026 : sources chronologie centralisées.
timeline_events(id PK AUTOINCREMENT, source_path FK→files, section_title,
                date_raw, date_sortable, body_excerpt)
timeline_links(event_id FK→timeline_events, target_name)
```

`date_sortable` est rempli seulement si la date extraite du titre `##`
matche `YYYY-MM-DD` ou `YYYY` — sinon `NULL` (aucune approximation
inventée). L'outil `engram_inspect` (tools/) permet d'auditer ces tables
sans SQL.

L'index tourne dans son propre thread (rusqlite n'est pas Sync). L'UI lit un
snapshot `Arc<RwLock<HashMap<chemin, FileStats>>>` + un compteur de génération
atomique ; elle reconstruit l'arbre quand la génération change.

## Format de session

- File tree : `<projet>/session.ron` (position/taille de la fenêtre, mode
  docked).
- Éditeur : `~/.config/engram_hive/modules/editor/session.ron` — liste des
  fenêtres (chemin, curseur, scroll, zoom, position/taille, typewriter),
  restaurées au relancement.

## Ajouter un module (guide pas à pas)

1. `modules/<nom>/` avec un `Cargo.toml` dépendant de `engram_core`.
2. Implémenter `trait Module` (`name`, `init`, `update`, `handle_event`,
   `shutdown`). Dessiner dans un viewport egui ; ne communiquer qu'en
   `ModuleResponse`.
3. Config : `config.rs` chargeant `<nom>.ron` (simple) + la **section `<nom>`**
   de la config experte unifiée via `ctx.licorne.section::<TonVomi>("<nom>",
   &mut errors)` (champs `#[serde(default)]`, erreurs renvoyées pour GLaDOS).
   Ajouter la section `"<nom>": TonVomi(...)` au modèle `config/engram.ron`
   (et garder le legacy `config/licorne-a-gerber.ron` en compatibilité si besoin).
4. `README_MODULE.md` répondant aux 4 questions (ce que je fais / comment je
   marche / comment me virer / mes dépendances).
5. Câbler dans `app/src/main.rs` : `registry.register("<nom>", || Box::new(…))`
   + dépendance dans `app/Cargo.toml`. Le module s'active via `modules.ron`
   (un `modules.ron` neuf inclut file_tree, editor et cockpit par défaut).
6. Logs : ajouter `"<nom>"` à la liste des cibles dans `init_logging`.

Pour le virer : supprimer `modules/<nom>/`, retirer la ligne du registre +
la dépendance, retirer `"<nom>"` de `modules.ron` et sa section de
`engram.ron` (ou de `licorne-a-gerber.ron` si le legacy est encore utilisé).
Le reste l'ignore. (La CLI `engram_hive module add/remove <nom>` automatise
tout ça ; un `cargo build` reste requis après.)

## Bloc unifié core + file_tree (le « hub »)

Le core et le file_tree forment **une seule fenêtre** : en haut le hub (statut +
fond logo `assets/core_bg.png` à ~5 %), en dessous le file_tree, séparés par une
poignée draggable (`TopBottomPanel::top(...).resizable(true)` + `CentralPanel`).
Déplacer/minimiser la fenêtre core emporte les deux.

Mécanisme générique (le core ne nomme aucun module) : le trait `Module` expose
`render_mode()`. La plupart des modules sont `OwnViewport` (l'éditeur ouvre une
fenêtre OS par fichier). Le file_tree est `EmbeddedInCore` : son `update` ne fait
que la logique, et le core le dessine via `draw_embedded(ui, out)` dans le
`CentralPanel`. Palette, sélecteur de dossier et dialogues restent des
`egui::Window` flottantes (overlays au niveau du contexte).

Conséquence : la conversion du file_tree en viewport différé (session précédente,
pour survivre à la minimisation du core) est **annulée** — sans fenêtre propre, le
file_tree ne peut plus se désynchroniser du core. Le mode focus de l'éditeur masque
simplement le panel (état conservé).

## Doctrine (rappels)

Pas de barre de menu, jamais. Pas d'onglets, jamais. Anti-black-box : chaque
fichier source s'ouvre sur un commentaire en français qui dit ce qu'il fait ;
toute dépendance se justifie et se documente. Toute erreur est remontée
(log + fenêtre), jamais silencieuse. Voir `ROADMAP.md` pour la Phase 3.

## Rendu de l'éditeur — contrainte ligne-par-ligne

L'éditeur peint le texte **ligne par ligne** : chaque ligne devient un galley
egui peint à un Y calculé, et tout en dépend (positionnement du curseur via
`pos_from_cursor`, clic→curseur, sélection, scroll via `y_of_line`).

### Tableaux — rendu grille

Les blocs tableaux markdown (`| a | b |`) sont détectés par
`table_edit_assist::detect_all_tables()`. Quand le viewport atteint la première
ligne d'un bloc, il rend un `egui::Grid` (un `TextEdit::singleline` par
cellule) et saute les lignes restantes du bloc. La hauteur mesurée du grid est
attribuée à la première ligne dans le LayoutCache, les autres lignes = 0 :
les prefix sums Y restent valides. La sérialisation retour en pipe-table
markdown se fait au focus-out du bloc (dirty flag).

### Frontmatter — édité en clair (§2 du brief 2/7/2026)

Le formulaire structuré des fiches personnage a été **retiré**. Le
frontmatter YAML est désormais rendu dans le buffer comme du texte normal,
sans couche d'abstraction, avec la coloration frontmatter appliquée
(clé:` en accent, valeur en couleur normale, wikilinks cliquables).
Approche Obsidian-style : les propriétés sont visibles et éditables en
haut du fichier, le texte narratif sous le `---` de fermeture s'affiche
normalement à la suite, rien ne disparaît.

La détection `tags: ["personnage"]` reste utilisée pour la coloration
frontmatter et les stats, pas pour déclencher une UI custom.

### Coloration de base (fallback)

Pour les fichiers non-personnage, les fallbacks de lisibilité en coloration
restent actifs :
- tableaux : les `|` des lignes de tableau sont atténués (`theme.markers`) ;
- frontmatter : la clé `clé:` est colorée en accent (`theme.tag`), la valeur
  garde sa couleur, les `[[wikilinks]]` y restent cliquables.

## Comportement zoom — feature intentionnelle

`Ctrl++` / `Ctrl+-` : zoom synchronisé sur toutes les fenêtres éditeur (zoom global)
`Ctrl+Molette` : zoom uniquement sur la fenêtre sous le pointeur (zoom local)

Ce comportement est voulu et non modifiable. Il répond à deux besoins distincts :
- `Ctrl++`/`-` pour ajuster la lisibilité globale rapidement
- `Ctrl+Molette` pour zoomer précisément sur une fenêtre spécifique

Ne pas « corriger » ce comportement.

## Auto-close des paires

Les paires `()` `[]` `[[` sont auto-closées.
Les guillemets `"` et `'` sont exclus : gérés par la smart typography
(« » / " ").
