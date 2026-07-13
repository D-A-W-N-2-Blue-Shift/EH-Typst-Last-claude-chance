# health — module Santé de Nexus

## Ce que je fais
Sous-vues **Sommeil**, **État psy** et **Médication** (doc de conception
§5.3). Formulaires minimaux (boutons radio 1-5, jamais de slider), écriture
dans `nexus.db`, affichage immédiat de moyennes glissantes recalculées
depuis la base à chaque saisie (28 jours pour le sommeil, 7 jours pour
l'état psy — jamais stockées en dur). L'état psy affiche un radar des 5
dimensions (épuisement, cognitif, sensoriel, masquage, fonctionnement —
base ABM/Raymaker 2020) comparé à la moyenne 7 jours. La médication tient
le registre de référence, un bouton « Prise » par molécule (timestamp
éditable, doc §5.3), l'historique jour/7 jours, et le ressenti différé
(disponible 3h après une prise sans ressenti encore enregistré).

## Comment je marche
- `OwnViewport` (comme `cockpit` côté écrivain) : fenêtre OS propre, fermée
  par défaut, ouverte sur commande explicite (bouton du hub, ou palette
  globale Ctrl+Shift+P — voir « Redrop » ci-dessous).
- J'apprends la racine du projet actif via `CoreEvent::ProjectRootUpdated`
  (relayé par le core depuis `nexus_hub`) et j'ouvre ma **propre** connexion
  à `nexus.db` — SQLite en mode WAL supporte plusieurs connexions
  concurrentes au même fichier, donc pas besoin de partager un objet
  `Connection` entre modules (ce qui serait impossible : §7.1 interdit à un
  module de dépendre d'un autre module).
- Erreurs remontées en `ModuleResponse::Error`, jamais silencieuses (règle
  GLaDOS).

## Comment me virer
1. Supprimer `modules/health/`.
2. Retirer `registry.register("health", …)` de `app_nexus/src/main.rs`.
3. Retirer la dépendance `health` de `app_nexus/Cargo.toml`.
4. Retirer `"modules/health"` des `members` du `Cargo.toml` du workspace.

## Mes dépendances (justifiées, §7.4)
- `engram_core` : le trait `Module`.
- `nexus_db` : couche de données Nexus (pas un module au sens du trait
  `Module`).
- `egui` : rendu UI + le radar (peinture personnalisée, `egui::Painter`).
- `chrono` : déjà dépendance du workspace (utilisée par le core). Besoin :
  arithmétique de dates pour les fenêtres glissantes (28j/7j) — hors de
  question de la refaire à la main, moins robuste (§A3).

## Décision structurelle notable (§A10, cause → conséquence → solution)
**Cause** : `health` a besoin de savoir quel projet est actif, mais ne peut
pas interroger `nexus_hub` directement (§7.1, modules aveugles entre eux).
**Conséquence** : sans canal de communication, `health` ne peut jamais
savoir où se trouve `nexus.db`.
**Solution retenue** : extension additive et générique du core —
`ModuleResponse::PublishProjectRoot(Option<PathBuf>)` /
`CoreEvent::ProjectRootUpdated(Option<PathBuf>)` — exactement le même
mécanisme que `PublishFileIndex`/`FileIndexUpdated`, déjà établi dans le
core pour ce type de problème (file_tree → editor côté écrivain). Ne nomme
aucun module. Ricochet vérifié et minimal : un seul bras `match` ajouté
(no-op) dans `app/src/main.rs` (app écrivain), aucune autre modification.
Prouvé sans régression par les 4 gates bibliques complets après le
changement.

## Décision structurelle notable #2 — registre médicaments (§A2, §4.4)
Le doc de conception se contredit : §3 (arborescence projet) place le
registre dans `02_sante/medicaments.typst` (fichier projet) ; §5.3 et §7
(Cockpit) parlent de `medications.ron` (config). Ambiguïté réelle, pas
tranchée arbitrairement : le doc lui-même pose la règle qui décide, §3
« Les fichiers .typst sont la source de vérité pour le texte narratif...
La DB SQLite est la source de vérité pour toute donnée structurée (prises,
scores, tâches, corrélations) ». Un médicament (id/nom/molécule/dose/notes)
est une donnée structurée, pas du texte narratif → le registre vit dans
`nexus_db.medications` (doc §8, déjà construite et testée à l'incrément 1),
édité directement dans cette sous-vue. Pas de fichier RON ni `.typst`
séparé pour l'instant.

## Redrop (doc §6) — pourquoi il n'est PAS ici
Le doc le dit explicitement (§2) : Redrop « n'est pas un module — c'est un
CoreEvent global déclenché depuis n'importe où ». Il vit dans
`app_nexus/src/redrop.rs` (le shell), pas dans `health`. `health` catche
seulement le hotkey Ctrl+Shift+P dans sa propre fenêtre (comme `cockpit`
côté écrivain) et pousse `ModuleResponse::OpenPaletteRequested` — variant
déjà existante du core, aucune modification requise pour ça.

## Limites connues (§10)
- Navigation entre sous-vues par onglets simples dans la fenêtre.
- Le radar est une peinture `egui::Painter` maison (pas de bibliothèque de
  graphiques) — API vérifiée contre le code source vendu d'egui/epaint/
  ecolor 0.31.1 avant écriture, pas devinée. Même discipline appliquée à
  `ComboBox`/`Window::title_bar` pour Redrop (app_nexus).
- Le réglage « ressenti différé configurable off » (doc §5.3) n'est pas
  câblé : le mécanisme (prompt après 3h) est livré, mais rien ne peut encore
  le désactiver — il n'y a pas de Cockpit Nexus pour porter ce réglage avant
  la session 8 (doc §10). Ajouter un toggle non fonctionnel aurait été
  incomplet, pas la fonctionnalité demandée.
- Le hotkey Redrop DIRECT et configurable (doc §6 : « palette + hotkey
  configurable ») est différé au même motif : la palette (déjà accessible
  depuis toutes les fenêtres, déjà son propre hotkey Ctrl+Shift+P) satisfait
  l'exigence explicite prioritaire ; un hotkey séparé n'a de sens que
  configurable, ce qui suppose Cockpit Nexus.
