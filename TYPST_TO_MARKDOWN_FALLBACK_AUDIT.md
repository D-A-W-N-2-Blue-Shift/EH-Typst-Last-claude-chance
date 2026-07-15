# TYPST_TO_MARKDOWN_FALLBACK_AUDIT

**Date** : 2026-07-15
**Brief** : « FALLBACK TOTAL DE TYPST VERS MARKDOWN » (Étape 0 — aucun code
modifié avant ce document).
**Dépôt** : Hive_RBMK_Tcherenkov (ce dépôt ne construit QUE l'app Hive ;
l'app écrivain Engram_Hive est archivée hors workspace et son fallback se
joue dans son dépôt beta séparé — voir « Périmètre » ci-dessous).

---

## 1. Périmètre — décision documentée

Le brief est écrit pour le vocabulaire écrivain (fiches, scènes, chapitres,
manuscrit, sessions, Ferrite). Dans CE dépôt :

- **Zone ACTIVE** (compilée, testée, livrée) : `core/`, `nexus_db/`,
  `modules/{nexus_hub,health,journal,todo,dashboard,articles,cockpit_nexus}`,
  `app_nexus/` — l'application Hive. C'est ELLE que le brief conclut :
  « hive redevient un éditeur Markdown-first complet ».
- **Zone ARCHIVE** (exclue du workspace, non compilée, intouchée) : `app/`,
  `modules/{editor,file_tree,cockpit,sticky_notes,timeline,claude_terminal}`,
  `demo_typst/`. Le moteur Typst réel (modules/editor/src/typst_render.rs,
  pipeline de rendu, corpus de démo) vit là. **Aucune modification ici** :
  ce code n'est pas construit, et le fallback écrivain appartient au dépôt
  beta. Principe du brief respecté à la lettre : rien n'y est supprimé,
  écrasé, converti ni renommé.

Le fallback appliqué ici = la zone active, « adapté aux structures
réelles » (Phase 10 du brief).

## 2. Inventaire des fichiers

### `.typ` / `.typst` sur le disque (hors target/)

13 fichiers, TOUS dans `demo_typst/` (corpus de démonstration de l'app
écrivain archivée) :

```
demo_typst/1_atelier/chapitre_actif.typ      demo_typst/7_notes/index_tags.typ
demo_typst/1_atelier/notes_actives.typ       demo_typst/README.typ
demo_typst/1_atelier/scene_active.typ        demo_typst/fiches/personnage_massif.typ
demo_typst/3_plan/chronologie/frise_principale.typ  demo_typst/fiches/tableau_complexe.typ
demo_typst/5_scenes/scene_1.typ              demo_typst/notes/notes_test.typ
demo_typst/roman/chapitre_charge.typ         demo_typst/roman/chapitre_reel.typ
demo_typst/templates/engram.typ
```

→ ZÉRO fichier `.typ`/`.typst` dans la zone active du dépôt. Les fichiers
`.typst` de Hive existent uniquement dans les PROJETS UTILISATEUR créés au
runtime (journal, articles, notes santé, scaffold), pas dans le dépôt.

### `.md`

30 fichiers (docs, README par module), dont 3 dans la zone archive.

## 3. Typst dans le CODE ACTIF — constat exhaustif

**Aucun moteur.** Preuves mécaniques :

- Zéro dépendance `typst*` dans les manifests actifs ni dans `Cargo.lock`
  (grep : aucune occurrence).
- Zéro compilation, zéro appel CLI, zéro rendu, zéro `_typst/`, zéro
  export implicite, zéro surveillance de fichiers typst, zéro « goto
  erreur typst », zéro blocage si typst absent. **La Phase 3 du brief
  (« retirer Typst du chemin critique ») est déjà satisfaite par
  construction dans la zone active** : Hive n'a jamais lancé Typst.

Ce qui reste de Typst dans la zone active est une **convention de nommage
et de gabarit**, en 4 points de code + 2 gabarits :

| Site | Rôle | Action du fallback |
|---|---|---|
| `modules/journal/src/entry.rs:21` (`path_for`) | nouvelles entrées créées en `AAAA-MM-JJ.typst` | créer en `.md` ; ouvrir un `.typst` existant tel quel (`.md` prioritaire si les deux existent) |
| `modules/articles/src/entry.rs:87` (`path_for`) | nouveaux articles créés en `<slug>.typst` | créer en `.md` ; les `.typst` existants s'ouvrent inchangés (chemin lu depuis la DB, agnostique) |
| `modules/nexus_hub/src/project.rs:28-30` (scaffold) | `medicaments.typst`, `notes_sante.typst`, `backlog.typst` amorcés vides | scaffolder en `.md` pour les NOUVEAUX projets (le scaffold n'écrase jamais l'existant — déjà testé) |
| `modules/health/src/notes.rs:43` + `FTS_KEY` | notes santé lues/écrites dans `notes_sante.typst` | utiliser `notes_sante.typst` s'il existe (projet ancien), sinon `notes_sante.md` ; clé FTS = chemin réel |
| `modules/journal/src/entry.rs:27` (`DEFAULT_TEMPLATE`) | gabarit `= Journal — {date}` (titre syntaxe Typst) | gabarit Markdown `# Journal — {date}` |
| `modules/articles/src/entry.rs:91-95` (`default_template`) | gabarit `= {titre}` | gabarit Markdown `# {titre}` |

Plus le gabarit d'exemple dans `config/Hive_RBMK.ron` (section `journal`).

## 4. Fonctions inspectées (liste du brief, structures réelles)

| Fonction | État vis-à-vis de l'extension |
|---|---|
| Création de fichiers | `.typst` codé en dur aux 3 sites ci-dessus → à basculer |
| File tree (hub) | agnostique : liste tout, aucune extension filtrée |
| Ouverture/sauvegarde | agnostique : chemins portés par l'état/la DB, `atomic_write` |
| Rendu | AUCUN rendu de document dans Hive (TextEdit brut) — rien à retirer |
| Indexation/recherche (FTS5) | agnostique : clé = chemin relatif, contenu = corps brut |
| Wikilinks | aucun moteur wikilink actif dans Hive — sans objet |
| Tags/frontmatter | parsing YAML manuel sur bloc `---` : identique en `.md` |
| Statistiques (word count) | `split_whitespace` sur le corps : agnostique |
| Session | Hive ne persiste aucune session de fichiers — sans objet |
| Git auto-commit | inexistant dans Hive — sans objet |
| Templates | 2 gabarits chaîne (journal, articles) → à passer en Markdown |
| Palette | 1 action (redrop), aucune action fichier — sans objet |
| Cockpit | statut de config uniquement, aucun lien typst — sans objet |
| Tests | littéraux `.typst` dans des tests de mécanisme (nexus_db, tree, watcher) : le mécanisme testé est agnostique ; littéraux mis à jour là où ils documentent la convention |

## 5. Base de données

Schéma inchangé (le brief interdit DB V2). `journal_entries.file_path` et
`articles.file_path` stockent des chemins relatifs quelconques — aucun champ
ne suppose une extension. Deux commentaires SQL (« le .typst est la
source ») seront actualisés (commentaires, zéro migration).

## 6. Configuration

La config réelle (`Hive_RBMK.ron`) n'a AUCUNE clé typst : il n'existe pas de
moteur à désactiver. Inventer une section `typst: (enabled: false, …)` pour
un moteur inexistant serait du décor en plastique — le brief demande
d'« adapter aux structures réelles » : le défaut Markdown-first est porté
par le code de création (aucun défaut implicite full-Typst ne subsiste), et
le gabarit d'exemple de la section `journal` passe en Markdown. Un seul
fichier de config, comme déjà en place.

## 7. Préservation (Phases 4/13)

Le fallback ne touche AUCUN fichier de projet existant : seuls les chemins
de CRÉATION changent. Un `.typst` existant est ouvert tel quel, jamais
converti, jamais renommé, jamais écrasé (tests de hash à l'appui). Pas de
sauvegarde de masse nécessaire puisque rien n'est déplacé. La copie
Markdown est une action EXPLICITE qui crée `nom.md` + un rapport
`<nom>.typ-to-md-report.md` sans modifier `nom.typ(st)`.

## 8. Hors périmètre (documenté, non fait)

- Zone archive (moteur typst de l'écrivain, demo_typst/, templates .typ
  legacy) : intouchée, non compilée — fallback à mener dans le dépôt beta.
- Étude Ferrite : séparée (brief).
- DB V2 : interdite (brief).
- Édition structurée des tableaux : hors scope (tableaux Markdown bruts).
