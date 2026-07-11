# BRIEF CODEX — EH5 v0.5.0 CORRECTIFS STRICTS

Objectif :
Corriger EH5 sans ajouter de feature.

1. Palette globale
- La palette ne doit plus appartenir à file_tree.
- La palette doit être gérée par core/app comme UI globale.
- Tout viewport peut envoyer ModuleResponse::OpenPaletteRequested.
- core/app ouvre alors une palette unique, indépendante du file_tree.
- Ne pas créer plusieurs palettes.
- Ne pas bricoler le focus.
- Ne pas laisser la palette dessinée dans le panneau file_tree.

2. Config unique
- Créer une source unique : ~/.config/engram_hive/engram.ron
- Si le fichier n’existe pas : le générer avec valeurs par défaut commentées.
- Si option absente : default.
- Si option invalide : warning + default pour cette option.
- Si fichier illisible : safe mode.
- engram.ron gagne toujours.
- Anciennes configs éclatées : ne plus les utiliser comme source active, sauf migration explicite simple.
- Ne pas ajouter de nouvelle option pour contourner un bug.

3. Font size
- Après config unique, appliquer font_size depuis engram.ron.
- Si une valeur comme 160 est définie, elle doit produire un effet visible ou être bornée avec warning explicite.
- Rapport : chemin exact config → struct Rust → rendu UI.

4. Docked
- Retirer le terme “docked” de l’UI si ce n’est pas un vrai docking.
- Ne pas implémenter un vrai docking maintenant.
- Remplacer par terme honnête ou supprimer l’option.

5. Message FTS5
- Remplacer tout message troll/artefact par :
  “Recherche full-text désactivée en EH5. Prévue après DB V2.”

  6.6. Suppression complète du mode docked

- Supprimer le mode docked / layout docked s’il existe.
- Retirer :
  - option config
  - entrée palette
  - texte UI
  - état session associé
  - code mort lié au toggle docked
  - tests obsolètes s’ils ne testent que ça
- Ne pas remplacer par une autre feature.
- File_tree reste dans son fonctionnement actuel stable.
- Editor garde ses fenêtres indépendantes.
- Si un champ de session ancien existe, l’ignorer avec compatibilité douce ou le supprimer si sans risque.
- Aucun crash si une ancienne session contient encore docked/layout_mode.
  
Interdit :
- DB V2
- sticky_notes
- COH2B
- Prisma
- llm_bridge
- refactor global
- nouvelle UI lourde
- nouvelle dépendance

Tests :
- cargo check
- cargo build
- cargo run
- Ctrl+Shift+P depuis editor ouvre palette globale
- Ctrl+Shift+P depuis cockpit ouvre palette globale
- palette ne dépend pas du file_tree visible
- engram.ron absent → généré
- engram.ron cassé → safe mode
- font_size depuis engram.ron appliqué ou warning clair