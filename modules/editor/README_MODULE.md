# Module `editor` — Éditeur de prose

## 1. Ce que je fais

J'édite les fichiers du projet (markdown d'écrivain, pas de WYSIWYG).
**Un fichier ouvert = une fenêtre OS native séparée. Jamais d'onglets.**
Cinq fichiers ouverts = cinq fenêtres, le WM tile comme il veut.

- Ouverture : double-clic dans le file_tree, clic sur un `[[wikilink]]`,
  CLI `engram_hive editor open <chemin>`, ou restauration de session.
- Édition : curseur clignotant, sélection clavier/souris (double-clic =
  mot, triple-clic = ligne), undo/redo par **transactions groupées**,
  couper/copier/coller, Tab = 2 espaces, auto-continuation de listes
  (`- `, `* `, `+ `, `> `, `1. ` — ligne vide + Entrée sort de la liste).
- Coloration markdown **Kate-like** : les marqueurs restent visibles et
  colorés (coloration locale + cache d'états par ligne, voir §2).
  Frontmatter YAML
  zoné, `[[wikilinks]]` aux crochets quasi invisibles, `#tags`,
  `% commentaires` grisés, blocs de code sur fond dédié, citations à barre.
- Smart typography fr/en : `«»`, `’`, `—` (protégé en début de ligne),
  `…`, espaces insécables avant `: ; ? !`. Chaque conversion est annulée
  AVEC la frappe par un seul Ctrl+Z.
- Zoom **par fenêtre** (Ctrl+Molette, Ctrl+0 reset, 10–48 pt) — jamais
  `set_zoom_factor` global.
- Mode focus (F11) : plein écran, file_tree masqué (via le core), status
  bar minimale. Mode typewriter : ligne courante maintenue à 0.4 de la
  hauteur, scroll souris libre.
- Recherche Ctrl+F / remplacement Ctrl+H : casse, mot entier, regex,
  compteur n/m, surlignage. La recherche tourne dans un thread.
- Stats : `mots | caractères | LxCy | session: +N | [=====> ] n/goal`,
  objectif `goal:` lu dans le frontmatter, `sél: N mots`.
- Snippets : `;sce`, `;per`, `;cha`, `;date`, `;dt` (un `.toml` par
  snippet dans `~/.config/engram_hive/snippets/`, créés au 1er lancement).
- Auto-complétion `[[` + 1 caractère : fuzzy insensible à la casse sur
  l'index du projet **fourni par le core** (publié par file_tree — je ne
  scanne jamais le filesystem).
- Sauvegarde : Ctrl+S, auto-save 5 min, astérisque dans le titre,
  détection des modifications externes (notify + hash xxh3 + flag
  « c'est moi qui écris » 2 s), Ctrl+Shift+R recharge. Tout buffer
  modifié est sauvé au quit.
- Session : fichiers, curseur, scroll, zoom, position/taille de fenêtre
  et mode typewriter restaurés au relancement.

### Hotkeys (tous remappables dans `~/.config/engram_hive/keybinds.ron`)

| Action | Défaut |
|---|---|
| `editor.save` | Ctrl+S |
| `editor.reload` | Ctrl+Shift+R |
| `editor.search` / `editor.replace` | Ctrl+F / Ctrl+H |
| `editor.search_next` / `editor.search_prev` | F3 / Shift+F3 |
| `editor.focus_mode` | F11 |
| `editor.typewriter` | Ctrl+Alt+T |
| `editor.zoom_reset` | Ctrl+0 |
| `editor.undo` / `editor.redo` | Ctrl+Z / Ctrl+Shift+Z |
| `editor.select_all` | Ctrl+A |
| `editor.new_view` | Ctrl+Shift+N (2e fenêtre sur le MÊME buffer) |

### CLI (Streamdeck via ExecCmd)

```bash
engram_hive editor open /chemin/vers/fichier.md
engram_hive editor zoom 150          # % de la taille de base, fenêtre active
engram_hive editor focus-mode toggle
engram_hive editor typewriter toggle
engram_hive editor save
```

Transport : socket Unix `~/.local/share/engram_hive/engram.sock`, une
ligne JSON par commande (`{"module":"editor","action":"zoom","value":150}`).
Pas d'instance qui tourne = message clair et code retour 1.

## 2. Comment je marche

- **Buffers** (`buffer.rs`) : ropey, partagés `Arc<RwLock<…>>` via
  `BufferMap` qui résout le chemin canonique AVANT d'allouer —
  `06_en_cours/svetlana.md` (symlink) et son original = **un seul buffer**,
  deux fenêtres dessus voient les mêmes modifications en temps réel
  (test : `buffer::tests::symlink_same_inode_same_buffer`).
- **Undo** : transactions, pas de snapshots. Frappes consécutives
  coalescées (1,5 s, max 64 ops) ; smart typography et snippets rejoignent
  la transaction de la frappe déclenchante.
- **Per-frame = O(visible)** (`viewport.rs`) : hauteurs de lignes
  mesurées quand visibles / estimées sinon, sommes préfixes pour
  `line_at_y` en O(log N) ; seules les lignes visibles sont colorées,
  mises en page et peintes. Le déplacement vertical est VISUEL (lignes
  wrappées) via la géométrie des galleys.
- **Coloration** (`highlight.rs`) : coloration locale ligne par ligne,
  les couleurs viennent du thème RON — jamais d'un thème externe.
  `state_cache[i]` garde l'état de bloc à la fin de la ligne i :
  scroller à la ligne 80 d'un frontmatter de 150 lignes part de
  `state_cache[79]`. Une frappe n'invalide que l'aval ; le rattrapage est
  budgété (2000 lignes/frame max) — jamais de freeze.
- **Threads** : recherche regex et comptage de mots clonent le rope
  (O(1) chez ropey) et répondent par `mpsc` avec compteur de génération.
  Le thread UI ne fait jamais de O(N fichier).
- **Erreurs — règle GLaDOS** : tout `Err` est logué dans
  `~/.local/share/engram_hive/logs/modules/editor.log` ET affiché en
  bandeau dans les fenêtres. Jamais l'un sans l'autre.
- **Config** : `~/.config/engram_hive/modules/editor/editor.ron` (créé au
  1er lancement) + `licorne-a-gerber_editor.ron` (expert, optionnel — modèle
  commenté dans `modules/editor/config/`, testé au parsing). Champ commenté =
  défaut. Syntaxe cassée = GLaDOS + défauts.
- **Couleurs** : dérivées de la palette globale `theme.ron` (titres H1-H6,
  wikilinks, sélection, code/tags), surchargeables finement dans
  `licorne-a-gerber_editor.ron`.
- **Saisie** : auto-fermeture des `()` et `[]` (donc `[[`→`[[]]`) ;
  formatage Ctrl+B/I/`/K/Shift+K autour de la sélection ou du mot courant.

## 3. Comment me virer

Supprimer `modules/editor/` + retirer `"editor"` de
`~/.config/engram_hive/modules.ron` + retirer les deux lignes me
concernant dans `app/src/main.rs` (registre) et `app/Cargo.toml`.
Le reste de la ruche ne me connaît pas : je ne parle au core que via
`ModuleResponse`, je ne touche à aucun autre module.

## 4. Mes dépendances

| Crate | Pourquoi |
|---|---|
| `engram_core` | trait Module, ModuleResponse/CoreEvent, IPC |
| `ropey` | buffer texte O(log N), clone O(1) pour les threads |
| `highlight.rs` local | coloration ligne par ligne |
| `serde_yaml` | frontmatter (`goal:`, `language:`, `smart_typography:`) |
| `serde` / `ron` | configs + session |
| `serde_json` | commandes IPC |
| `toml` | fichiers de snippets |
| `regex` | recherche/remplacement |
| `xxhash-rust` (xxh3) | vraie modif externe vs bruit notify |
| `chrono` | snippets `;date` / `;dt` |
| `notify` | détection des modifications externes |
| `egui` | rendu ; une fenêtre = un viewport natif |
| `dirs`, `tracing` | chemins XDG, logs |

## Déviations assumées par rapport au brief

1. **`core/src/ipc/` n'existait pas** (le brief le disait livré) → créé
   dans cette session (`core/src/ipc.rs` + test d'intégration
   `core/tests/ipc.rs`).
2. **Contrat core étendu** : `CreateAndOpenFile` n'existait pas ; ajoutés
   aussi `PublishFileIndex`/`FileIndexUpdated` (l'index des `.md` voyage
   file_tree → core → éditeur) et `FocusModeChanged` (le core ordonne au
   file_tree de se masquer). Les modules ne se parlent toujours jamais
   entre eux.
3. **Pas de `theme.ron` dans le repo** : défauts = palette OLED noir +
   rose néon du core, chaque couleur surchargeable dans `licorne-a-gerber_editor.ron`.
4. **`comrak` non embarqué** : la coloration locale + analyses de ligne
   couvrent la détection d'éléments. Un second parseur markdown complet
   inutilisé serait une dette ; il entrera en Phase 2 (export).
5. **`rayon` non embarqué** : `std::thread` + `mpsc` suffisent (une
   recherche = un thread). Rayon se justifiera avec du vrai parallélisme
   de données (recherche multi-fichiers, Phase 2).
6. **`arboard` non instancié directement** : egui-winit utilise déjà
   arboard comme couche clipboard ; je consomme `Event::Copy/Cut/Paste`
   plutôt que d'ouvrir un second contexte clipboard concurrent.
7. **Ouvrir deux fois le même fichier focalise la fenêtre existante** (le
   file_tree canonicalise avant d'envoyer : deux chemins du même inode
   sont indistinguables à l'arrivée). La seconde vue du même buffer est
   explicite : Ctrl+Shift+N — c'est elle qui démontre le critère n°4
   (deux fenêtres, un buffer).
8. **Gras typographique** : epaint n'a pas de gras synthétique. Si une
   variante Bold de la police est trouvée, elle est utilisée ; sinon le
   gras est rendu par la couleur/le marquage. L'italique est synthétique
   (epaint `italics`) si pas de variante Italic.
9. **Remplacement regex** : le texte de remplacement est littéral (pas de
   groupes `$1`) en v1 — documenté, Phase 2 si besoin.
