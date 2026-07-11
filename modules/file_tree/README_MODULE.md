# Module : file_tree

Hub de navigation et d'indexation sémantique d'Engram_Hive.

## 1. Ce que je fais

- J'affiche le projet comme un **tableau de bord sémantique** : comptages de
  mots par dossier, progression vers les `goal:`, icônes de statut
  (⚪ brouillon/draft, 🔵 en_cours/wip, 🟡 a_relire/review, 🟢 ready/final,
  🔴 bloque/blocked), 🔗 sur les symlinks de `06_en_cours/`.
- J'ouvre et je crée des projets via un **sélecteur de dossier intégré**
  (`folder_picker`, 100% egui — aucune dépendance au portail XDG, même
  comportement sur tout WM). Un dossier sans `.engram/` est traité comme
  nouveau projet : je génère l'arborescence standardisée (01_architecture …
  06_en_cours) sans toucher aux fichiers existants. La CLI
  `engram_hive project open --path <dossier>` ouvre aussi un projet sans GUI.
- Je maintiens l'index SQLite (`.engram/index.db`) : word counts corps/YAML
  séparés, wikilinks `[[]]` (+ détection d'orphelins), tags inline `#tag`,
  index full-text FTS5. La palette globale ouvre la recherche corpus locale
  sur cet index.
- Je surveille le projet avec `notify` (debounce 500 ms configurable) : une
  modification externe met les stats à jour en moins d'une seconde.
- Je gère `06_en_cours/` : des **symlinks**, jamais de copies. Le fichier
  `.context` garde un mapping lisible. Je refuse de supprimer autre chose
  qu'un symlink depuis En Cours.
- Double-clic sur un `.md` → `ModuleResponse::OpenFile(chemin canonique)`.
  En Cube 0, le core le logue (pas encore d'éditeur) — le chemin canonique
  garantit que le futur éditeur dédupliquera les buffers par inode.
- Clic droit contextuel + command palette `Ctrl+Shift+P` (fuzzy). Pas de
  barre de menu, jamais. Équivalents CLI :
  `engram_hive tree new-file --path … [--name …]`,
  `engram_hive context add --file …`, `engram_hive context clear`.
- Session par projet dans `.engram/session.ron` : position/taille de fenêtre,
  mode `Independent`/`Docked` (bascule via palette). En Docked, j'envoie
  `WindowMoved` au core qui arbitre les positions — je ne parle jamais aux
  autres modules.
- Erreurs GLaDOS : tout `Err` est logué dans
  `~/.local/share/engram_hive/logs/modules/file_tree.log` ET affiché dans ma
  fenêtre.

## 2. Comment je marche

```
lib.rs           orchestrateur, impl trait Module, viewport egui, dialogues
project.rs       ouverture/création de projet, structure standard, fs ops
indexer.rs       thread dédié : SQLite + notify + parsing markdown
                 (UI ← snapshot Arc<RwLock> + génération atomique,
                  UI → channel de commandes Reindex/Remove/FullRescan/Silence)
stats.rs         modèle TreeNode, agrégats par dossier, statuts, formatage
tree_view.rs     rendu egui de l'arbre (ne modifie rien, remonte les intentions)
context_menu.rs  menus clic droit (construction pure, exécution dans lib.rs)
palette.rs       command palette Ctrl+Shift+P, filtrage fuzzy
symlinks.rs      06_en_cours/ : créer/retirer/vider les liens, .context
layout.rs        session.ron, bascule Independent/Docked
config.rs        file_tree.ron (simple) + file_tree_vomi.ron (expert)
```

Config : `~/.config/engram_hive/modules/file_tree/file_tree.ron` (créé au
premier lancement) et `file_tree_vomi.ron` (optionnel, 14 réglages experts).
Commenter une ligne = valeur par défaut.

Déviations assumées vs le brief (validées par la logique, pas par caprice) :
- La table FTS5 est **autonome** (pas de `content='files'`) : le schéma
  externe du brief référençait une colonne `body_text` que `files` n'a pas.
- Colonne additionnelle `files.file_stem` pour résoudre les wikilinks
  orphelins en une requête SQL.
- `06_en_cours/` est exclu de l'index (les liens pointent vers des originaux
  déjà indexés — pas de double comptage d'inode).

## 3. Comment me virer

1. Retirer `"file_tree"` de `~/.config/engram_hive/modules.ron`.
2. Supprimer le dossier `modules/file_tree/` et la ligne correspondante dans
   le `Cargo.toml` du workspace + l'enregistrement dans `app/src/main.rs`.
3. Optionnel : supprimer `~/.config/engram_hive/modules/file_tree/`.

Les projets restent du markdown standard : rien dans le dossier projet ne
m'appartient (`.engram/` est régénérable et peut être supprimé sans perte).

## 4. Mes dépendances

- `engram_core` (le trait Module — seule dépendance interne)
- `egui` (rendu + sélecteur de dossier intégré, plus aucune dépendance à
  rfd/portail XDG), `rusqlite` bundled (SQLite + FTS5), `notify` (watcher),
  `ron` + `serde` (configs/session), `dirs` (zéro chemin hardcodé),
  `tracing` (jamais de println!)
- Linux uniquement en Phase 1 (`std::os::unix::fs::symlink`).
