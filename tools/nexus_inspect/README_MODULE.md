# nexus_inspect — outil d'audit lecture seule de Nexus

## Ce que je fais
Binaire séparé, lecture seule (doc de conception §5.8). Ouvre `nexus.db`
d'un projet Nexus et expose 5 onglets, tous lisibles sans écrire une seule
ligne de SQL (règle « no black box » du doc) :

- **Fichiers** : `journal_entries` + `articles`, bruts.
- **Santé** : `sleep_log` + `med_doses` + `mood_log`, bruts.
- **Tâches** : `tasks` + `task_history`, bruts.
- **Corrélations** : `weekly_load` et `med_observance` (doc §4.2) calculées
  à la demande + tâches bloquées > 7 jours. Voir « Écart documenté »
  ci-dessous pour les 2 vues du doc qui ne sont pas ici.
- **Intégrité** : clés orphelines, dates incohérentes, fichiers indexés
  mais absents du disque.

## Comment je marche
- Binaire `eframe` autonome, **pas** un module Nexus : aucun trait
  `Module`, aucun `CoreContext`, aucun registre. Un chemin de projet saisi
  au clavier (même limite que `nexus_hub` : pas de sélecteur natif), ouvert
  via `nexus_db::open_ro_db`.
- Lecture seule GARANTIE au niveau SQLite (`SQLITE_OPEN_READ_ONLY` +
  `PRAGMA query_only = ON`, `nexus_db/src/open.rs`) — pas seulement une
  convention de code respectée par ce binaire, une contrainte que la base
  elle-même fait respecter. Une tentative d'écriture échouerait même en cas
  de bug dans ce code.

## Écart documenté (doc §4.2, §5.8)
Le doc nomme 4 vues de corrélation : `corr_sommeil_cognition`,
`corr_medication_etat`, `weekly_load`, `med_observance`. Seules les 2
dernières sont implémentées ici — **dupliquées** depuis
`modules/dashboard/src/correlations.rs` (logique identique, déjà prouvée à
l'incrément 7), pas importées : `dashboard::correlations` est un module
privé de sa crate, l'import serait de toute façon impossible, et
`nexus_inspect` suit le même principe que les modules pairs (§7.1) — pas de
dépendance croisée.

`corr_sommeil_cognition` et `corr_medication_etat` ne sont PAS calculées :
aucune des deux n'a de précédent Rust éprouvé ailleurs dans ce dépôt —
`dashboard` les a lui-même explicitement exclues de son propre périmètre à
l'incrément 7, pour la même raison (titre littéral de session, pas de
logique à copier). Les inventer sous pression de temps pour cet incrément
aurait signifié une logique de corrélation JAMAIS vérifiée, dans un outil
dont la seule raison d'être est la fiabilité de lecture. La règle « no
black box » reste respectée : les données brutes dont ces 2 vues auraient
besoin (`sleep_log`, `mood_log`, `med_doses`) sont toutes lisibles dans
l'onglet Santé, juste pas pré-corrélées.

## Comment me virer
1. Supprimer `tools/nexus_inspect/`.
2. Retirer `"tools/nexus_inspect"` des `members` du `Cargo.toml` du
   workspace.

Aucun autre binaire n'en dépend : outil totalement autonome, pas de
registre à mettre à jour ailleurs.

## Mes dépendances (justifiées, §7.4)
- `nexus_db` : couche de données (lecture seule ici).
- `egui`/`eframe` : interface, même stack que le reste du projet.
- `chrono` : parsing/formatage de dates pour les contrôles d'intégrité et
  les corrélations.

## Limites connues (§10)
- Pas de sélecteur de dossier natif (même limite que `nexus_hub`).
- 2 vues de corrélation absentes — voir « Écart documenté » ci-dessus.
- Les contrôles d'intégrité sont un filet de sécurité pour une base
  modifiée hors de l'app (édition manuelle, ancien schéma) — les chemins
  d'écriture normaux de Nexus (via `nexus_db::open_db`, clés étrangères
  actives) ne devraient jamais produire les incohérences détectées ici ;
  l'onglet existe pour le jour où ce ne serait plus vrai.
