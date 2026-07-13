# nexus_db — couche de données de Nexus

## Ce que je fais
Je définis et j'ouvre `nexus.db`, la base SQLite de Nexus (schéma du document de
conception §8 : `medications`, `med_doses`, `mood_log`, `sleep_log`, `tasks`,
`task_recurrence`, `task_history`, `journal_entries`, `articles`, `fts_content`).
J'expose un accès **typé** : un `struct` par table et, pour chaque entité,
insertion / listing / lecture par clé unique.

## Comment je marche
- `open_db(path)` crée le dossier `.engram/` au besoin, active WAL + les clés
  étrangères, applique le schéma de façon idempotente (motif calqué sur
  `file_tree/src/indexer.rs`).
- `open_ro_db(path)` ouvre en lecture seule (pour le futur `nexus_inspect`, §5.8).
- `open_in_memory()` sert aux tests.
- Les modules Nexus (health, todo, journal, dashboard) m'appellent pour lire et
  écrire ; ils ne rédigent jamais de SQL eux-mêmes.
- Les « vues » de corrélation (doc §4.2) ne sont **pas** stockées ici : le
  document précise qu'elles sont exécutées à la demande. Elles vivront dans le
  module `dashboard`, testées sur données réelles.

## Comment me virer
1. Supprimer le dossier `nexus_db/`.
2. Retirer `"nexus_db"` des `members` du `Cargo.toml` du workspace.
3. Retirer la dépendance `nexus_db` des modules qui l'utilisent.

Aucun autre fichier n'a besoin d'être touché : `nexus_db` ne dépend d'aucun
module ni du core, et rien dans le core ne le connaît.

## Mes dépendances (justifiées, §7.4)
- `rusqlite` (SQLite bundled) : le moteur de stockage, déjà utilisé par la
  version écrivain.
- `serde_json` : les champs `tags` (doc §8) sont des tableaux JSON stockés en
  TEXT.
- `thiserror` : erreurs typées (`NexusDbError`), conformément à §7.5 (pas de
  `String` d'erreur nue, pas de `unwrap`/`expect`).

## Limites connues (§10)
- Les identifiants (`id TEXT PK`) sont générés par `new_id()` = millisecondes
  UNIX + compteur atomique de process (pas de dépendance `uuid`). Unicité
  garantie au sein d'un process ; entre deux lancements, le préfixe
  milliseconde diffère.
- La couche ne contient aucune logique métier (moyennes, transitions de statut,
  corrélations) : elle stocke et restitue. La logique appartient aux modules.
