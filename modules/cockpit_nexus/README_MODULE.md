# cockpit_nexus — module Cockpit de Nexus

## Ce que je fais
Panneau statut + actions sur la configuration Nexus (doc de conception
§5.7 : « identique au Cockpit de Hive »). Pour chaque section réellement
lue par un module Nexus au démarrage — `theme`, `theme_expert`, `journal`,
`dashboard`, `todo` — j'affiche : le fichier cible, l'état de lecture
(absent / erreur de lecture / erreur de parsing / chargé OK), et 3-4
actions (« Ouvrir fichier », « Ouvrir dossier », « Recharger depuis
disque », « Relancer app » si la section n'est pas rechargeable à chaud).

Le thème (`theme`) est rechargeable ET ré-applicable À CHAUD (même
mécanisme que le Cockpit écrivain) : `journal`/`dashboard`/`todo` sont lus
une seule fois à `init()` par leur module respectif, donc une relance
complète est nécessaire pour qu'un changement soit pris en compte.

Une carte d'INFORMATION dédiée pour les médicaments — voir plus bas.

## Ce que je NE fais PAS (décision fondatrice, §A2/§10)
Pas de formulaire d'édition de valeur individuelle. Vérifié par lecture
directe du source du Cockpit écrivain (`modules/cockpit/src/lib.rs`) AVANT
d'écrire ce module : malgré son commentaire d'en-tête (« Édition UI des
fichiers RON »), son implémentation réelle n'a jamais que les 4 actions
listées ci-dessus par section — aucun champ de saisie de couleur, de texte
ou de nombre nulle part dans son code. Le vrai mécanisme d'édition établi
dans ce dépôt est : ouvrir le fichier RON dans l'éditeur externe de
l'utilisateur (`xdg-open`/`open`), pas un formulaire interne. Ce module
reproduit fidèlement CE patron-là (le réel, pas le commentaire) plutôt que
d'inventer une UI d'édition de valeurs qui n'existe nulle part ailleurs
dans ce projet — cohérence avec « modulaire comme l'est la version
écrivain » plutôt qu'invention.

## Décision — pourquoi pas de ligne « medications » (doc §5.7)
Le doc liste « config medications (liste molécules + doses par défaut) »
parmi les configs RON attendues du Cockpit. Ce module n'en a pas : à
l'incrément 3, le registre des médicaments a été tranché comme vivant dans
`nexus_db.medications` (SQLite), pas dans un fichier RON séparé — décision
documentée dans `health/README_MODULE.md`, motivée par la règle générale du
doc lui-même (§3 : « la DB SQLite est la source de vérité pour toute donnée
structurée »). Une ligne de cockpit qui pointerait vers un fichier RON
inexistant et jamais lu par aucun module serait un décor en plastique —
exactement ce que le Cockpit écrivain s'interdit lui-même (son propre texte
d'aide, repris ici : « pas décor en plastique »). À la place : une carte
d'information expliquant la décision + un bouton « 🏥 Ouvrir Santé » qui
renvoie vers le module qui gère réellement les médicaments.

## Décision — pourquoi pas de ligne « colonnes todo » (doc §5.7)
Même raisonnement. Voir `modules/todo/src/config.rs` et
`modules/todo/README_MODULE.md` : les colonnes du kanban sont des chaînes
canoniques dont dépendent `transition_task_status` (nexus_db) et la
régénération de récurrence — les rendre configurables casserait cette
logique déjà testée sans que le doc §5.4 ne le demande explicitement (il
décrit un kanban à 6 colonnes fixes). Seuls les FILTRES par défaut de todo
sont exposés (section "todo", champ `default_filter_maintenant` /
`default_filter_contexte`).

## Comment je marche
- `OwnViewport`, fenêtre à la demande (bouton « 🛠 Cockpit » du hub).
- Découplé (§7.1) : ne dépend d'AUCUNE crate de module Nexus, seulement du
  core (`engram_core::theme::{ThemeConfig, ThemeExpert, Palette,
  apply_palette}`, `engram_core::Licorne` — tous déjà publics et partagés
  avec le Cockpit écrivain). Le renvoi vers Santé passe par un nom de
  module en chaîne (`ModuleResponse::OpenModuleWindow("health")`), jamais
  un import de crate.
- `ModuleResponse::RestartApp` (déjà générique, existante depuis
  l'app écrivain) déclenche la même relance différée que côté écrivain —
  voir `app_nexus/src/main.rs::restart_now`, ajouté dans cet incrément (le
  binaire nexus l'ignorait silencieusement avant : bug réel corrigé ici,
  pas une fonctionnalité nouvelle inventée).

## Comment me virer
1. Supprimer `modules/cockpit_nexus/`.
2. Retirer `registry.register("cockpit_nexus", …)` de
   `app_nexus/src/main.rs`.
3. Retirer la dépendance `cockpit_nexus` de `app_nexus/Cargo.toml`.
4. Retirer `"modules/cockpit_nexus"` des `members` du `Cargo.toml` du
   workspace.

## Mes dépendances (justifiées, §7.4)
- `engram_core` : trait `Module` + `theme::*` + `Licorne` (tous déjà
  publics, aucune extension du core nécessaire pour ce module).
- `ron` : sonde de validité syntaxique du fichier `engram.ron` (même usage
  que le Cockpit écrivain — parse générique, pas de schéma par section).

## Limites connues (§10)
- Pas de rechargement à chaud pour journal/dashboard/todo (ces modules ne
  relisent leur config qu'à `init()`) — relance requise, affichée
  explicitement à l'écran, jamais silencieuse.
- Pas de formulaire d'édition — décision ci-dessus, pas un oubli.
