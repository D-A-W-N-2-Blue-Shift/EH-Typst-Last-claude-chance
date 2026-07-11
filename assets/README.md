# assets/

Assets visuels d'Engram_Hive. Chargés au runtime (jamais `include_bytes!` :
un asset manquant ne casse pas le build, l'app démarre sans).

| Fichier        | Usage                          | Format conseillé              |
|----------------|--------------------------------|-------------------------------|
| `icon.png`     | Icône d'application (§2.3)      | PNG carré, ≥ 256×256, RGBA    |
| `core_bg.png`  | Fond de la fenêtre core (§2.4)  | PNG paysage (badge/bannière)  |

Le chargement est géré par `app/src/assets.rs`. Si un fichier est absent ou
illisible : avertissement dans les logs, fallback propre (icône système /
pas de fond).
