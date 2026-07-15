# ENGRAM_FULL_TYPST_TO_MARKDOWN_FALLBACK_REPORT

**Date** : 2026-07-15 · **Branche** : `claude/markdown-fallback`
**Périmètre** : zone active du dépôt = l'application Hive
(Hive_RBMK_Tcherenkov). La zone archive (app écrivain, exclue du workspace)
est intouchée — son fallback appartient au dépôt beta séparé. Décision
détaillée dans `TYPST_TO_MARKDOWN_FALLBACK_AUDIT.md` §1.

## 1. État initial

Hive nommait tous ses fichiers narratifs en `.typst` (journal, articles,
notes santé, scaffold) et ses gabarits utilisaient des titres Typst (`=`).
AUCUN moteur Typst n'a jamais existé dans la zone active : zéro dépendance,
zéro compilation, zéro CLI (preuves dans l'audit §3) — le « full Typst » de
Hive était une convention de nommage, pas un pipeline.

## 2. Inventaire `.typ` / `.md`

13 fichiers `.typ`, tous dans `demo_typst/` (corpus de l'app écrivain
archivée) — zéro dans la zone active. 30 fichiers `.md` (docs). Les
`.typst` de Hive vivent dans les projets utilisateur au runtime, pas dans
le dépôt. Aucun fichier du dépôt n'a été supprimé, renommé ni converti.

## 3. Code et configurations modifiés

- `modules/journal/src/entry.rs` : `path_for` crée en `.md` ;
  `legacy_path_for` + `resolve_path_for` (`.md` prioritaire, `.typst`
  hérité respecté, création en `.md`) ; gabarit `# Journal — {date}`.
- `modules/journal/src/lib.rs` : résolution via `resolve_path_for` ;
  bouton « Créer une copie Markdown » sur entrée `.typst`.
- `modules/articles/src/entry.rs` : `path_for` en `.md`, `legacy_path_for`
  pour la désambiguïsation, gabarit `# {titre}`.
- `modules/articles/src/lib.rs` : désambiguïsation anti-collision avec les
  hérités ; bouton « Créer une copie Markdown ».
- `modules/health/src/notes.rs` : résolution `.md`/`.typst` hérité, clé FTS
  = chemin réel, `create_md_copy`.
- `modules/health/src/lib.rs` : bouton de copie sur notes héritées.
- `modules/nexus_hub/src/project.rs` : scaffold en `.md`, garde
  anti-compagnon (un projet hérité ne reçoit pas de doublon `.md`).
- `modules/nexus_hub/src/lib.rs` : arbre — les `.typ(st)` sont marqués
  « Typst — secondaire » ; en-tête « Format principal : Markdown ».
- `core/src/typst_fallback.rs` (NOUVEAU) : conversion sûre pure + rapport
  (utilitaire texte sans nom de module, même statut que `atomic_write`).
- `config/Hive_RBMK.ron` : gabarit d'exemple en Markdown.
- `nexus_db/src/schema.rs` : 2 commentaires actualisés (zéro migration).

## 4. Format par défaut confirmé

Toute création (journal, articles, notes, scaffold) produit du `.md` —
prouvé par tests (`nouvelle_entree_creee_en_markdown_sans_compagnon_typst`,
`creation_en_markdown_par_defaut`, `scaffold_cree_la_structure_doc_3`).
Aucun `.typ` compagnon n'est jamais créé (testé).

## 5. Fonctions Markdown restaurées

File tree, ouverture/édition/sauvegarde, FTS (contenu + clés), frontmatter
YAML, tags, statistiques de mots, watcher — TOUTES étaient déjà agnostiques
à l'extension (audit §4) et fonctionnent sur `.md` (tests FTS/word count
passent sur les nouveaux chemins). Wikilinks/session/Git auto-commit/zoom :
inexistants dans Hive — sans objet (audit §4).

## 6. Typst hors du chemin critique

Déjà le cas par construction : aucun moteur à débrancher (audit §3).
Hive démarre et fonctionne sans Typst installé — c'est son état permanent.

## 7. Préservation des `.typ` existants

- Ouverture d'un `.typst` hérité : octets STRICTEMENT identiques après
  ouverture (testé : `entree_typst_heritee_ouverte_telle_quelle_octets_intacts`,
  `typst_herite_utilise_tel_quel_sans_conversion`).
- Aucun compagnon `.md` créé en silence (testé, scaffold inclus).
- Aucune conversion automatique nulle part.
- Édition texte simple d'un `.typst` hérité : possible (brief Phase 4).

## 8. Conversion assistée : IMPLÉMENTÉE

Commande explicite « Créer une copie Markdown » (journal, articles, notes
santé) : lit le buffer courant, crée `nom.md` (refus si existant), ne
modifie JAMAIS l'original, convertit uniquement les structures sûres
(titres `=` → `#` ; frontmatter verbatim ; prose verbatim), conserve tout
code Typst dans des blocs balisés `ENGRAM_TYPST_UNCONVERTED_BEGIN/END`
(format exact du brief), produit `<nom>.typ-to-md-report.md`, ouvre la
copie. Testée bout-en-bout dans les 3 modules (originaux intacts à l'octet).

## 9. Templates

Gabarits journal (`# Journal — {date}`) et articles (`# {titre}`) en
Markdown standard + frontmatter YAML, zéro appel/macro Typst. Le gabarit
reste configurable (section `journal` de `Hive_RBMK.ron`). Aucun template
`.typ` n'existait dans la zone active ; ceux de l'archive écrivain
(demo_typst/templates/) sont conservés tels quels en legacy.

## 10. Indexation

FTS5 : clé = chemin relatif réel (`.md` pour le neuf, `.typst` pour
l'hérité), contenu = corps brut — vérifié par tests dans journal, articles
et notes. `.md` = index principal de fait (toute nouvelle création).
DB V2 : non commencée (interdit du brief respecté).

## 11. Compatibilité session / config

Hive ne persiste aucune session de fichiers et n'a aucune option Typst en
config (audit §4/§6) : un ancien projet s'ouvre tel quel, ses `.typst`
gardent leur extension, toute nouvelle création est en `.md`. Aucune
deuxième config créée (`Hive_RBMK.ron` reste l'unique fichier). Aucun
défaut implicite full-Typst ne subsiste (gabarits + chemins basculés).

## 12. Tests

161 → 181 tests (20 nouveaux, dont les tests dédiés au fallback), 0 échec :
- création `.md` sans compagnon (journal, notes, scaffold) ;
- ouverture `.typst` hérité intacte à l'octet (journal, notes) ;
- priorité `.md` quand les deux existent (journal, notes) ;
- anti-collision de slug avec hérité (articles) ;
- conversion sûre (7 tests core : titres, frontmatter, code balisé,
  prose verbatim, groupage des blocs, faux positifs `#`) ;
- copie explicite bout-en-bout ×3 modules (original intact, rapport
  généré, bascule d'édition, refus d'écrasement) ;
- gabarits Markdown (pas de `=`).
Gates : fmt exit 0 · clippy -D warnings 0 · build 0 warning · lancement
réel 12 s sous Xvfb.

## 13. Limites

- La conversion assistée ne traduit QUE les titres — tout autre Typst est
  conservé en bloc balisé (aucune invention, brief Phase 5). Suffisant pour
  les fichiers émis par Hive (qui n'ont jamais contenu que des titres `=`).
- L'inline Typst (`*gras*`, `_italique_`) n'est pas réécrit : la sémantique
  d'emphase diffère entre les deux formats et réécrire de la prose sans
  garantie serait une normalisation destructive (interdit du brief).
- L'index FTS d'un `.typst` copié garde aussi l'entrée de l'original tant
  qu'il existe sur le disque : exact (les deux fichiers existent).

## 14. Reste à faire

- Rien dans la zone active pour ce brief.
- Le fallback de l'app ÉCRIVAIN (moteur typst réel, éditeur, sessions,
  templates .typ, Ferrite) appartient au dépôt beta séparé — ce rapport et
  l'audit documentent la frontière (point d'entrée futur : reprendre les
  Phases 1-13 du brief sur cette base de code là-bas).
