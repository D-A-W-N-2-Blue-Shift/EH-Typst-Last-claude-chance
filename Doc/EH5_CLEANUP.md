# EH5 Cleanup

## Architecture actuelle
- `core` orchestre le bus `ModuleResponse` / `CoreEvent` et charge la config unifiée.
- `app` câble les modules dans le `ModuleRegistry`.
- La palette de commandes est globale, dessinée par `app`, pas par `file_tree`.
- `editor`, `file_tree` et `cockpit` sont les modules de base.
- `timeline` reste un placeholder temporel en attente de DB V2.
- `claude_terminal` reste un assistant CLI en attente de refonte.

## Modules actifs par défaut
- `file_tree`
- `editor`
- `cockpit`

## Modules désactivés par défaut
- `timeline`
- `claude_terminal`

## Config cible
- `~/.config/engram_hive/engram.ron`
- Sections:
- `basic`
- `deep`
- `modules`
- `providers`
- `theme`
- `editor`
- `file_tree`
- `backup`
- `basic.font_size` alimente la taille de base de l'éditeur au démarrage.

## Ce qui a été supprimé ou adouci
- Les références UX qui présentaient `timeline` comme un viewer SQL actif.
- Le chargement par défaut a été recentré sur la base EH5.
- Le cockpit expose maintenant la cible `engram.ron`.
- Le mode `docked` a été retiré.
- Le message de recherche full-text a été normalisé pour annoncer DB V2.

## Reporté à DB V2 / sticky_notes / COH2B
- DB V2 pour la timeline.
- sticky_notes.
- COH2B.
- provider bridge / llm_bridge.
