# todo — module Todo de Nexus

## Ce que je fais
Kanban orienté énergie disponible, pas priorité abstraite (doc de
conception §5.4). 6 colonnes : `backlog → today → doing → done`, avec
`blocked` et `dropped` comme branches distinctes de `doing` (obstacle
externe identifié vs décision active d'abandon — jamais un oubli
silencieux). Filtre « maintenant » (croise l'énergie des tâches avec le
score `cognitif` le plus récent de `mood_log`), filtre contexte rapide,
signalement visuel des tâches sans durée estimée, sous-tâches (1 niveau),
récurrence (régénération automatique d'une nouvelle occurrence quand une
tâche récurrente passe en `done`).

## Comment je marche
- `OwnViewport`, fenêtre à la demande (bouton « ✅ Todo » du hub).
- J'apprends la racine du projet actif via `CoreEvent::ProjectRootUpdated`
  (même mécanisme que health/journal) et j'ouvre ma propre connexion à
  `nexus.db`.
- Changement de statut via menu déroulant (pas de drag-and-drop — doc §5.4,
  explicitement hors périmètre : « complexité egui non justifiée pour le
  MVP »). Chaque changement passe par `nexus_db::transition_task_status`,
  transactionnel : le statut ET l'entrée `task_history` sont écrits
  ensemble, ou aucun des deux.

## Comment me virer
1. Supprimer `modules/todo/`.
2. Retirer `registry.register("todo", …)` de `app_nexus/src/main.rs`.
3. Retirer la dépendance `todo` de `app_nexus/Cargo.toml`.
4. Retirer `"modules/todo"` des `members` du `Cargo.toml` du workspace.

## Mes dépendances (justifiées, §7.4)
- `engram_core` : le trait `Module`.
- `nexus_db` : couche de données Nexus.
- `chrono` : dates et calcul de récurrence (`checked_add_months` pour le
  mensuel — vérifié contre la source vendue de chrono 0.4.45 avant usage).

## Décision — ambiguïté du doc sur le filtre « maintenant » (§4.4, §A2)
Le doc ne donne qu'**un seul point** : « si cognitif = 2, affiche uniquement
les tâches `energie: low` », sans préciser la courbe complète (valeurs 1,
3, 4, 5) ni confirmer le sens de l'échelle. Faute de données suffisantes
pour reconstituer l'intention exacte, j'ai retenu la lecture la plus
conservatrice cohérente avec ce point unique et avec la convention déjà
utilisée dans `health::mood` (1 = jour favorable, 5 = jour difficile) :
seul le score 1 débloque `medium`/`high`, tout le reste (2 à 5) reste sur
`low`. Politique isolée dans `logic::allowed_energy_levels`, une fonction
pure d'une ligne — triviale à corriger si l'intention réelle diffère
(§A4 réversibilité). Voir les tests de cette fonction pour le comportement
exact actuellement livré.

## Limites connues (§10)
- Pas de drag-and-drop, pas de partage/collaboration, pas de Gantt —
  explicitement hors périmètre du doc lui-même (§5.4 « Ce qu'il ne fait
  pas »).
- La récurrence régénère la tâche suivante avec `statut: backlog` et
  reprend titre/énergie/contexte/durée/notes à l'identique ; seule
  l'échéance avance selon la règle. Une règle de récurrence non reconnue
  (« custom » au sens libre) ne régénère rien automatiquement — signalé en
  GLaDOS, jamais une interprétation inventée de la règle.
