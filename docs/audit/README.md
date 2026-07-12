# Audit & documentation — 2026-07-12

Audit **lecture seule** (aucune modification de code) portant sur la **solidité**
et l'**efficacité** du projet. Sécurité **hors périmètre** (à la demande).

| Document | Pour qui | Contenu |
|---|---|---|
| [`AUDIT_TECHNIQUE.md`](AUDIT_TECHNIQUE.md) | dev | constats détaillés (fichier:ligne), sévérité, correctifs, plan d'action priorisé |
| [`AUDIT_VERSION_SIMPLE.md`](AUDIT_VERSION_SIMPLE.md) | non-technique | même bilan, sans jargon, avec priorités |
| [`MANUEL_UTILISATEUR_COMPLET.md`](MANUEL_UTILISATEUR_COMPLET.md) | utilisateur | manuel chapitré : installation → écriture → rendu → sauvegardes → dépannage |

## Le résultat en bref

Base saine et disciplinée (0 `unwrap`/`panic`/`unsafe` en prod, verrous
résistants à l'empoisonnement, tests + clippy au vert). Priorités dégagées :

1. 🔴 **Écritures fichier non atomiques** — risque de troncature du texte sur
   crash pendant l'enregistrement. *(Correctif simple : temp + rename.)*
2. 🟠 **Base `.engram/index.db` partagée** par deux modules sans WAL/`busy_timeout`
   cohérents — index périmé + recherches en échec sous contention.
3. 🟠 **Repaint 60 fps permanent** dès qu'une fenêtre éditeur est ouverte —
   batterie.
4. 🟠 **Marqueurs de notes** qui réécrivent les fichiers (fins de ligne, newline
   final).

Détails, autres constats et plan complet dans `AUDIT_TECHNIQUE.md`.
