# nexus_hub — fenêtre principale de Nexus

## Ce que je fais
Je suis la fenêtre principale de Nexus (doc de conception §5.1). Pour cette
fondation : j'ouvre ou je crée un projet Nexus (arborescence §3 :
`01_journal/`, `02_sante/`, `03_todo/`, `04_articles/`, `05_reference/`,
`.engram/`), j'initialise `nexus.db` via `nexus_db::open_db`, et j'affiche
son état (tables présentes / attendues).

## Comment je marche
- `EmbeddedInCore` (comme `file_tree` côté écrivain) : pas de fenêtre OS
  propre, je me dessine dans un panel fourni par le binaire `Hive_RBMK_Tcherenkov`.
- Je ne parle au core QUE via `ModuleResponse` (erreurs remontées en
  `ModuleResponse::Error`, jamais silencieuses — règle GLaDOS).
- Le chemin du projet se saisit au clavier (champ texte), pas de sélecteur
  de dossier natif pour l'instant — voir « Limites » ci-dessous.

## Comment me virer
1. Supprimer `modules/nexus_hub/`.
2. Retirer `registry.register("nexus_hub", …)` de `app_nexus/src/main.rs`.
3. Retirer la dépendance `nexus_hub` de `app_nexus/Cargo.toml`.
4. Retirer `"modules/nexus_hub"` des `members` du `Cargo.toml` du workspace.

## Mes dépendances (justifiées, §7.4)
- `engram_core` : le trait `Module`, obligatoire pour tout module.
- `nexus_db` : couche de données Nexus. Ce n'est pas un module au sens du
  trait `Module` (pas de fenêtre, pas de logique métier) — dépendance
  comparable à celle de `file_tree` vers `rusqlite` directement.
- `egui` : rendu de l'UI embarquée.

## Limites connues (§10, signalées explicitement)
- **Pas de sélecteur de dossier natif.** Le chemin du projet se colle dans
  un champ texte. `file_tree::folder_picker` n'est pas réutilisable
  (module privé, et §7.1 interdit à un module de dépendre d'un autre
  module) ; un sélecteur 100% egui propre à Nexus pourra être ajouté plus
  tard sans changement d'architecture.
- **`engram.ron` généré au premier lancement contient des sections de l'app
  écrivain** (`file_tree`, `editor`, `cockpit`, …). Cause : `Licorne::load`
  (core, non modifié) embarque au moment de la compilation le template
  `config/engram.ron` du dépôt via `include_str!`, qui est celui de l'app
  écrivain — le core n'a pas de template dédié à Nexus. Effet : purement
  cosmétique, ces sections sont ignorées par `nexus_hub` (prouvé : le
  filtrage de `ModulesConfig::load_from_licorne` ne retient que les noms
  effectivement enregistrés par le binaire courant). Non corrigé ici : la
  correction toucherait `core/`, hors périmètre sans validation explicite.
- Pas d'arbre de fichiers, pas d'index FTS, pas de watcher, pas de palette
  de commandes, pas de bouton Redrop : différés aux futurs modules
  (health/todo/journal/dashboard), qui les apporteront à mesure des besoins
  (§7.6 YAGNI — je n'anticipe pas ce qui n'est pas encore demandé).
