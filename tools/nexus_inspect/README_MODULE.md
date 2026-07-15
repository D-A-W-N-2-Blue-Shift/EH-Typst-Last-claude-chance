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

## Vues de corrélation (doc §4.2, §5.8)
Le doc nomme 4 vues : `corr_sommeil_cognition`, `corr_medication_etat`,
`weekly_load`, `med_observance`. Les 4 sont implémentées ici — **dupliquées**
depuis `modules/dashboard/src/correlations.rs` (logique identique, déjà
prouvée), pas importées : `dashboard::correlations` est un module privé de
sa crate, l'import serait de toute façon impossible, et `nexus_inspect` suit
le même principe que les modules pairs (§7.1) — pas de dépendance croisée.

`corr_sommeil_cognition`/`corr_medication_etat` avaient été absentes
(différées à tort comme hors périmètre à l'origine — relecture complète
doc-vs-code a montré qu'aucune des deux n'était en fait hors de portée).
Contrairement au dashboard, pas de scatter peint ici : les points bruts sont
listés en texte (onglet Corrélations), cohérent avec le rôle d'audit lecture
seule de cet outil — la visualisation vit côté dashboard.

Écart restant, identique au dashboard : `corr_medication_etat` n'a pas de
lissage LOESS (algorithme itératif sans précédent dans ce dépôt) — les
points bruts sont listés, la LISSE elle-même non calculée (§A2).

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
