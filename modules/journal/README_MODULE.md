# journal — module Journal de Nexus

## Ce que je fais
Entrée quotidienne (doc de conception §5.2). J'ouvre automatiquement
`01_journal/YYYY/YYYY-MM-DD.typst` du jour, je le crée depuis un template
minimal s'il n'existe pas, j'affiche les 7 dernières entrées en sidebar
(cliquables, avec leur nombre de mots), et je synchronise `journal_entries`
dans `nexus.db` (word_count, tags extraits du frontmatter) à chaque
sauvegarde — au focus-out du champ de texte, au changement de jour, à la
fermeture de la fenêtre.

## Comment je marche
- `OwnViewport`, fenêtre à la demande (bouton « 📓 Journal » du hub, ou
  hotkey directe Ctrl+Shift+J depuis n'importe où — doc §5.2, seul module
  concerné par cette exigence explicite).
- J'apprends la racine du projet actif via `CoreEvent::ProjectRootUpdated`
  (même mécanisme que `health`) et j'ouvre ma propre connexion à `nexus.db`.
- Écriture sur disque via `engram_core::atomic_write` (déjà utilisée par
  `file_tree` côté écrivain) — jamais un `std::fs::write` brut.

## Comment me virer
1. Supprimer `modules/journal/`.
2. Retirer `registry.register("journal", …)` de `app_nexus/src/main.rs`
   (et le hotkey Ctrl+Shift+J associé).
3. Retirer la dépendance `journal` de `app_nexus/Cargo.toml`.
4. Retirer `"modules/journal"` des `members` du `Cargo.toml` du workspace.

## Mes dépendances (justifiées, §7.4)
- `engram_core` : le trait `Module` + `atomic_write`.
- `nexus_db` : couche de données Nexus.
- `chrono` : dates (déjà dépendance du workspace).

## Écart documenté au doc (§5.2 « éditeur identique à Hive ») — §A2/§16
Le doc demande explicitement un éditeur identique à celui de l'app
écrivain : buffer ropey, coloration `.typst`, wikilinks, zoom. Ce module
utilise pour l'instant un `egui::TextEdit::multiline` standard.

**Cause vérifiée, pas supposée.** `editor::EditorWindow` (la struct qui
assemble buffer + coloration + wikilinks côté écrivain) a un constructeur
**privé** (`fn new`, ligne ~152 de `modules/editor/src/lib.rs` — pas
`pub fn new`) et plusieurs champs privés (`closed`, `restore`, `last_rect`,
`request_os_focus`, `last_title`, `seen_version`, `note_count`,
`table_grids_version`). Rust interdit la construction d'une struct par
littéral dès qu'un seul champ n'est pas public : `EditorWindow` est donc
structurellement irréutilisable tel quel depuis un autre crate — ce n'est
pas une limite de convenance, c'est une contrainte du langage, vérifiée par
lecture directe du code source avant toute décision.

**Ce qui EST public et réutilisable** : `editor::buffer::open_standalone`,
`editor::highlight::HighlightCache`, `editor::wikilinks::*`,
`editor::viewport::show_text_area` (avec `TextAreaParams`). Reconstruire
l'équivalent d'`EditorWindow` en assemblant ces briques est possible, mais
`show_text_area` (~800 lignes) est le rendu le plus complexe du dépôt —
l'intégrer correctement sans pouvoir vérifier visuellement le résultat dans
cet environnement headless (aucun affichage, `wayland-client` absent —
limite déjà signalée aux incréments précédents) risquerait de produire du
code qui compile mais rend mal, sans preuve possible que ce soit correct.

**Décision** (§A3 robustesse > simplicité) : livrer une zone de texte
**réelle et fonctionnelle** (`TextEdit::multiline`, widget standard déjà
éprouvé) plutôt qu'une intégration risquée et invérifiable. C'est un écart
assumé, pas une fonctionnalité manquante maquillée en faite.

**Reste à faire, explicitement** : intégration du moteur ropey/coloration/
wikilinks — une unité de travail à part entière, à traiter comme telle
(vérification visuelle sur une machine avec affichage requise avant de la
déclarer terminée).

## Autres limites connues (§10)
- Template configurable via `journal.ron` (doc §5.2) : différé, pas de
  Cockpit Nexus pour le porter avant la session 8. Le template par défaut
  est en dur dans `entry.rs`.
- Pas d'analyse de sentiment, pas de résumé automatique, pas de
  synchronisation — explicitement hors périmètre du doc lui-même (§5.2
  « Ce qu'il ne fait pas »).
