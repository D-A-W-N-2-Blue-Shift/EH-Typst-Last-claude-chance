# assets/

Assets visuels de Hive_RBMK_Tcherenkov. EMBARQUÉS dans le binaire à la
compilation (`include_bytes!`, voir `app_nexus/src/assets.rs`) : les fichiers
doivent exister au build, mais un contenu corrompu ne fait que déclencher un
fallback propre au lancement (icône système / pas de fond), jamais de panique.

| Fichier                   | Usage                      | Format conseillé           |
|---------------------------|----------------------------|----------------------------|
| `Hive-RBMK-icone.png`     | Icône d'application        | PNG carré, ≥ 256×256, RGBA |
| `Hive-RBMK-bck_core.png`  | Fond de la fenêtre core    | PNG paysage (filigrane)    |

Les pixels actuels sont hérités de l'ancien projet (placeholders) : remplace
le CONTENU des fichiers par tes visuels Hive, sans changer les noms — aucun
changement de code requis, un simple rebuild suffit.
