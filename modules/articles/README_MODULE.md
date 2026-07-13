# articles — module Articles de Nexus

## Ce que je fais
Éditeur de prose pour les articles (doc de conception §5.5). Contrairement
au journal, pas de concept « aujourd'hui » : une liste des articles
existants (indexés dans `articles`, table SQLite) + création d'un nouvel
article depuis un titre — slug généré automatiquement, désambiguïsé par
suffixe numérique si le fichier existe déjà (`04_articles/<slug>.typst`,
doc §3).

Frontmatter à 5 champs (doc §5.5), édité par formulaire :
`titre` / `statut` (brouillon → revue → publié → archivé) / `tags` / `date_cible`
/ `destination`. Stats simples affichées en direct (doc §5.5, « pas de stats
orientées roman ») : nombre de mots + temps de lecture estimé — dérivés du
corps à l'affichage, jamais stockés (le schéma `articles`, doc §8, n'a pas de
colonne dédiée et n'en a pas besoin).

## Comment je marche
- `OwnViewport`, fenêtre à la demande (bouton « 📰 Articles » du hub).
- J'apprends la racine du projet actif via `CoreEvent::ProjectRootUpdated`
  (même mécanisme que journal/health/todo/dashboard) et j'ouvre ma PROPRE
  connexion à `nexus.db`.
- Je ne perds jamais silencieusement une modification non enregistrée :
  `open_existing` et `create_new` sauvent l'article en cours avant de
  basculer vers un autre (garde symétrique dans les deux sens, testée).

## Écart documenté (doc §5.5 « réutilise le moteur éditeur de Hive »)
IDENTIQUE à l'écart déjà investigué et documenté pour `journal`
(`modules/journal/README_MODULE.md` « Écart documenté ») : `editor::
EditorWindow` a un constructeur privé et des champs privés — Rust interdit
la construction par littéral dès qu'un seul champ est privé, donc
structurellement inconstructible depuis un autre crate. Ce fait a été établi
par lecture directe du source à l'incrément 4 et n'a pas été re-vérifié ici
— c'est le même moteur, donc le même mur. Zone de texte du corps :
`egui::TextEdit::multiline`, réel et fonctionnel, mais pas le moteur
ropey/coloration Typst/wikilinks demandé par le doc.

## Décision — parsing du frontmatter (§7.4, §A2)
Même patron que `journal::entry` (parsing manuel ligne à ligne), étendu de
1 champ (`tags`) à 5 champs plats (`titre`/`statut`/`tags`/`date_cible`/
`destination`). Pas de `serde_yaml` : vérifié que ce n'est PAS le patron
établi ailleurs dans ce dépôt pour du frontmatter — `modules/editor` a
justement RETIRÉ son formulaire frontmatter dédié (commentaire dans
`modules/editor/src/lib.rs` : « formulaire frontmatter custom supprimé »)
et édite désormais le frontmatter comme du texte brut avec coloration, pas
comme une structure typée. Le format ici reste plat, interne, contrôlé par
ce seul module — un vrai parseur YAML serait une dépendance pour un besoin
qui ne la justifie pas encore (§7.4).

Dupliqué plutôt qu'importé depuis `journal` : deux modules pairs du même
registre (§7.1), aucune dépendance croisée entre crates de modules.

## Décision — temps de lecture (doc §5.5 : « temps de lecture estimé »)
200 mots/minute — borne basse de la fourchette courante (200-250 mots/min)
pour ne jamais SOUS-estimer le temps affiché, faute de mesure personnelle
disponible dans ce projet. Constante nommée (`entry::WORDS_PER_MINUTE`),
changeable en un point.

## Comment me virer
1. Supprimer `modules/articles/`.
2. Retirer `registry.register("articles", …)` de `app_nexus/src/main.rs`.
3. Retirer la dépendance `articles` de `app_nexus/Cargo.toml`.
4. Retirer `"modules/articles"` des `members` du `Cargo.toml` du workspace.

## Mes dépendances (justifiées, §7.4)
- `engram_core` : le trait `Module` + `atomic_write` (écriture sûre).
- `nexus_db` : couche de données Nexus (table `articles`, déjà construite
  et testée à l'incrément 1).
- `chrono` : horodatage `updated_at`.

## Limites connues (§10)
- Pas de sélecteur de fichier natif : la liste vient uniquement de la table
  `articles` (un article créé hors de Nexus et jamais ouvert ici n'apparaît
  pas tant qu'aucun mécanisme d'indexation externe ne l'ajoute — même limite
  de fond que l'absence de watcher `notify` sur ce module, non demandée par
  le doc pour cette session).
- Écart éditeur ci-dessus.
