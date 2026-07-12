# Audit technique — Engram_Hive / EH-Typst-Labs

**Date :** 2026-07-12
**Périmètre :** solidité (robustesse) + efficacité (performance).
**Hors périmètre (à la demande) :** sécurité.
**Nature :** audit **lecture seule**. Aucune ligne de code n'a été modifiée.
**Version auditée :** branche `claude/code-audit-documentation-zm4x75`, commit `68f3159` (« Version Alpha »).

---

## 0. Résumé exécutif

Codebase de ~21 000 lignes de Rust, workspace de 8 crates (core + 7 modules).
**C'est du travail sérieux et discipliné.** Les fondamentaux de robustesse sont
là : zéro `unwrap()`/`expect()`/`panic!`/`unsafe` en code de production, verrous
résistants à l'empoisonnement, undo transactionnel, déduplication des buffers par
inode, remontée systématique des erreurs. La suite de tests passe, `clippy`
strict passe, le formatage passe (vérifié + rapport VOENPRED existant).

L'audit ne remet donc pas en cause la qualité générale. Il identifie **un point
dur prioritaire** (écritures fichier non atomiques dans un outil dont le métier
est de ne pas perdre le texte de l'auteur), **une contention de base de données
partagée entre deux modules**, et **une consommation CPU/batterie permanente à
60 fps**. Le reste relève de l'optimisation de montée en charge (gros projets)
et de l'hygiène de process (CI absente).

| Axe | Verdict | Détail |
|---|---|---|
| Discipline d'erreurs | ✅ Excellent | 0 unwrap/expect/panic/unsafe hors tests |
| Concurrence | 🟠 Bon, 2 réserves | contention DB partagée, threads IPC non bornés |
| Intégrité des données | 🔴 1 point dur | sauvegarde non atomique |
| Performance par frame | ✅ Bon | rendu O(visible), caches à préfixes |
| Performance de fond | 🟠 Perfectible | réindexation non incrémentale, clones O(N) |
| Énergie | 🟠 À corriger | repaint 60 fps permanent |
| Process | 🟡 Lacune | pas de CI, `audit.toml` référencé mais absent |

---

## 1. Méthode et vérifications

- **Lecture** du core (`lib`, `module_api`, `ipc`, `backup`, `config`,
  `licorne`, `theme`) et des modules lourds : `file_tree/indexer`,
  `editor/{buffer, highlight, viewport, input, typst_render}`,
  `sticky_notes/db`, plus le binaire `app/main`.
- **Balayage statique** : `unwrap`/`expect`/`panic`/`unsafe`, verrous, threads,
  channels, `clone`, arithmétique (saturating/checked).
- **Exécution réelle** : `cargo test -p engram_core` → **14 + 1 tests OK**
  (backup, licorne, theme, config, IPC round-trip).
- **Corroboration** : le rapport `voenpred_report.txt` (variante *depsweep*, daté
  2026-07-11) confirme `fmt-check`, `check-all`, `clippy-strict`, `test-all`,
  `doc` **tous OK** ; seul `audit` (cargo-audit) est en échec — voir §4.7.

Le binaire complet (`app`) n'a pas été lié dans cet environnement (bibliothèques
Wayland absentes), ce qui est sans incidence sur l'analyse statique et sur les
tests du core, qui eux tournent.

---

## 2. Points forts vérifiés (à préserver)

1. **Zéro déballage brutal en production.** `grep` exhaustif : les 5 seuls
   `unwrap()` sont dans un `#[test]`. Aucun `expect`, `panic!`, `unreachable!`,
   `todo!`, `unsafe`.
2. **Verrous résistants à l'empoisonnement.** `SharedBufferExt`
   (`buffer.rs:382`) et tous les accès `Mutex`/`RwLock` récupèrent via
   `PoisonError::into_inner` : un panic isolé n'écroule pas l'app en cascade.
   C'est exactement le bon réflexe pour un outil d'écriture.
3. **Undo/redo transactionnel avec coalescence** (`buffer.rs`) : frappe +
   mutation typographique = une seule annulation ; garde-fous `COALESCE_WINDOW`
   (1,5 s) et `COALESCE_MAX_OPS` (64).
4. **Déduplication par inode** : symlink et original partagent un buffer unique
   (`BufferMap::open` canonicalise avant d'allouer). Testé.
5. **Transactions SQLite** pour les écritures groupées (`full_scan`,
   `process_pending`, `save_note`) : atomicité par lot.
6. **Résilience config** : `modules.ron` illisible → `.bak` + régénération ;
   thème/licorne cassés → repli + erreur remontée (pas de crash).
7. **Rendu par frame O(visible)** : `viewport.rs` (LayoutCache à sommes
   préfixes, `line_at_y` en O(log N)) et `highlight.rs` (cache d'état de bloc
   par ligne, budget de rattrapage `CATCHUP_BUDGET`).
8. **Doctrine anti-boîte-noire tenue** : chaque fichier s'ouvre sur un
   commentaire qui dit ce qu'il fait ; erreurs remontées (log + fenêtre).

---

## 3. Solidité — constats

Sévérité : 🔴 élevée · 🟠 moyenne · 🟡 faible · ⚪ info.

### 3.1 🔴 Sauvegarde non atomique — risque de perte/troncature du fichier

**Où :** `modules/editor/src/buffer.rs:316` (`save`) ;
`modules/sticky_notes/src/db.rs:369` & `:389` (marqueurs) ;
`modules/editor/src/typst_render.rs:383` (status).

`Buffer::save` écrit via `std::fs::write(&self.path, &text)`. Cet appel
**tronque puis réécrit** le fichier en place. Si le processus est tué, si le
disque est plein, ou en cas de coupure de courant **pendant** l'écriture, le
fichier sur disque reste **tronqué ou vide** — c'est-à-dire le manuscrit de
l'auteur. Le contenu vit encore en mémoire tant que l'app tourne, mais le
fichier, lui, est abîmé.

Atténuations existantes : autosave (5 min par défaut), sauvegarde au quit, et
WrapDrive (backup tar.gz toutes les 30 min). Elles **réduisent** le rayon
d'impact mais ne l'éliminent pas — pire, WrapDrive peut archiver un fichier
tronqué s'il tombe au mauvais moment.

**Correctif recommandé (motif standard « write-temp + rename ») :**
écrire dans un fichier temporaire du **même répertoire** (`.<nom>.tmp`), `flush`
+ éventuellement `sync_all`, puis `std::fs::rename(tmp, cible)`. Le `rename` est
atomique sur un même système de fichiers : à tout instant, la cible est soit
l'ancienne version complète, soit la nouvelle version complète, jamais un
moignon. À appliquer aux trois sites d'écriture ci-dessus (le plus critique
étant `buffer.rs`).

> C'est le correctif au meilleur rapport valeur/effort du rapport : quelques
> lignes, et le métier du produit (ne pas perdre le texte) devient robuste aux
> crashs.

### 3.2 🟠 Base SQLite partagée par deux modules, sans WAL ni `busy_timeout` cohérents

**Où :** `modules/file_tree/src/indexer.rs:364` (`open_db`) et
`modules/sticky_notes/src/db.rs:66` (`open_db`) ouvrent **le même fichier**
`<projet>/.engram/index.db` avec des connexions distinctes.

- `file_tree` (thread indexeur) : `Connection::open`, **sans** `busy_timeout`,
  **sans** `journal_mode = WAL`.
- `sticky_notes` : `busy_timeout(1500 ms)` mais **sans** WAL.

En mode journal par défaut (rollback), SQLite n'autorise qu'un écrivain à la
fois et un écrivain bloque les lecteurs. Conséquences concrètes quand les deux
modules écrivent en même temps :

1. Les écritures de `file_tree` peuvent recevoir `SQLITE_BUSY` **immédiatement**
   (pas de `busy_timeout`). Plusieurs de ces écritures sont avalées
   (`let _ = tx.commit()`, `let _ = conn.execute_batch(...)`). La transaction
   est atomique, donc **pas de corruption** — mais la mise à jour d'index est
   **silencieusement perdue** ce tour-là (index périmé jusqu'au prochain
   événement). Le `DELETE` de nettoyage **hors transaction** de `full_scan`
   (`indexer.rs:474`) peut, lui, laisser des lignes périmées s'il échoue.
2. La recherche corpus (`search_corpus`, connexion lecture seule **sans**
   `busy_timeout`, `indexer.rs:159`) peut échouer en `SQLITE_BUSY` pendant une
   réindexation → « Recherche impossible » remontée à l'utilisateur.

Symptôme dominant : **staleness silencieuse** et échecs de recherche
occasionnels, pas de corruption. Cela contredit néanmoins la doctrine
d'étanchéité (« les modules ne se parlent jamais ») : ils partagent en réalité
un fichier de base.

**Correctif recommandé :** sur **toutes** les connexions à `index.db`, activer
`PRAGMA journal_mode = WAL` (lecteurs et écrivain concurrents) **et**
`busy_timeout` (≥ 1000 ms). Alternative plus stricte : donner à `sticky_notes`
sa propre base (`.engram/notes.db`) pour rétablir l'étanchéité réelle.

### 3.3 🟠 Insertion de marqueurs de notes : réécriture des fichiers de l'auteur

**Où :** `modules/sticky_notes/src/db.rs:357` (`insert_marker`) et `:373`
(`remove_marker`).

Pour ancrer une note, le module insère un marqueur (`/* note:<id> */` en `.typ`,
`<!-- note:<id> -->` sinon) **dans le fichier source de l'auteur**, en
reconstruisant le fichier par `text.lines() … lines.join("\n")`. Ce motif a
trois effets de bord non désirés :

1. **Normalisation des fins de ligne** : un fichier en CRLF (`\r\n`) ressort en
   LF (`\n`). Réécriture massive invisible.
2. **Perte du saut de ligne final** : `lines()` ne renvoie pas d'élément vide
   final, et `join("\n")` n'en rajoute pas → un fichier qui se terminait par
   `\n` perd ce `\n`.
3. **Écriture non atomique** (même motif que §3.1) et **course avec l'éditeur**
   si le fichier est ouvert avec des modifications non sauvegardées : le
   marqueur va sur le disque, la prochaine sauvegarde de l'éditeur l'écrase, ou
   déclenche la bannière « modifié en externe ».

Pour un produit dont la doctrine est « rien ne disparaît », réécrire
silencieusement les fins de ligne et le newline final d'un fichier source est
une régression de fidélité (bruit dans les diffs git).

**Correctif recommandé :** préserver le style de fin de ligne et le newline
final d'origine ; écrire atomiquement (§3.1) ; idéalement, faire passer
l'insertion de marqueur par le buffer de l'éditeur quand le fichier est ouvert,
plutôt que par une écriture disque directe.

### 3.4 🟠 Suppression d'index par chemin non canonique

**Où :** `modules/file_tree/src/indexer.rs:695` (`remove_one`).

Quand un fichier est supprimé, il ne peut plus être canonicalisé. Le code
supprime alors les lignes en base avec `WHERE canonical_path = key_exact`, où
`key_exact` est le **chemin brut** reçu du watcher. Or les lignes ont été
insérées sous le **chemin canonique** (`index_one` canonicalise, `:537`). Si les
deux diffèrent (symlink dans un dossier parent, chemin relatif vs absolu), les
lignes des tables `files`, `fts_content`, `wikilinks`, `tags` **ne sont pas
supprimées** → entrées d'index fantômes et résultats de recherche corpus vers un
fichier disparu. Le nettoyage mémoire (`mem`), lui, teste les deux formes et est
correct — d'où une **divergence mémoire ↔ base**.

**Correctif recommandé :** conserver une clé stable (p. ex. chemin relatif au
projet) comme identifiant, ou supprimer en base en testant les deux formes de
chemin, ou réconcilier au prochain `full_scan` (supprimer de la base tout
`canonical_path` absent du disque).

### 3.5 🟡 UUID de note : échec de `/dev/urandom` ignoré → collisions silencieuses

**Où :** `modules/sticky_notes/src/db.rs:459` (`new_uuid_v4`).

`let _ = f.read_exact(&mut bytes)` ignore l'échec de lecture. Si `/dev/urandom`
est illisible, `bytes` reste à zéro → toutes les notes créées dans cet état
partagent l'id `00000000-0000-4000-8000-000000000000`. Comme `id` est clé
primaire avec `ON CONFLICT(id) DO UPDATE`, la seconde note **écrase**
silencieusement la première. Probabilité très faible, mais chemin de **perte de
données silencieuse**.

**Correctif recommandé :** utiliser la crate `getrandom`, ou propager l'erreur
(refuser de créer une note sans entropie plutôt que d'en fabriquer une à id nul).

### 3.6 🟡 IPC : fenêtre TOCTOU au bind + threads par connexion non bornés

**Où :** `core/src/ipc.rs:57` (`bind`), `:96` (thread par connexion), `:91`
(`incoming`).

- `bind` fait `exists()` → `instance_alive()` → `remove_file` → `bind` : deux
  instances démarrant simultanément peuvent franchir la vérification ensemble
  (course). Socket Unix local, risque faible.
- `accept_loop` lance un thread **non nommé** par connexion, **sans plafond**.
  Les clients CLI vivent < 1 ms (le commentaire l'assume), mais rien ne borne un
  client pathologique.
- Une erreur de `listener.incoming()` fait `continue` : sur une erreur
  persistante (p. ex. `EMFILE`, trop de descripteurs), boucle chaude possible.

Impact réel faible (surface locale). À garder en tête si l'IPC s'ouvre un jour à
autre chose qu'un CLI local de confiance.

### 3.7 ⚪ Validation de date trop permissive

**Où :** `modules/file_tree/src/indexer.rs:855` (`normalize_date_marker`).

Le jour est validé `1..=31` quel que soit le mois (`1942-02-31` accepté). Aucun
impact fonctionnel (chronologie de fiction, tri lexicographique), noté pour
exhaustivité.

---

## 4. Efficacité — constats

### 4.1 🟠 Repaint 60 fps permanent dès qu'une fenêtre éditeur est ouverte

**Où :** `app/src/main.rs:571‑579`.

```
let active_children = … m.active_viewport_count() …;
if active_children > 0 {
    ctx.request_repaint_after(Duration::from_millis(16));
}
```

Tant qu'au moins une fenêtre éditeur est ouverte (c'est-à-dire l'usage normal),
l'application **redessine à ~60 fps en continu, même totalement inactive**. Une
app egui bien réglée retombe à 0 fps quand rien ne bouge ; ici, le heartbeat
force le réveil permanent du CPU/GPU. Sur un portable, pour un traitement de
texte, c'est une **ponction batterie continue**.

Le heartbeat n'est réellement nécessaire que lorsque la fenêtre core est
**minimisée** (Wayland cesse alors d'envoyer les *frame callbacks* aux viewports
enfants — régression documentée §5 du brief). En fenêtre visible, la vsync du
compositeur suffit.

**Correctif recommandé :** conditionner le `request_repaint_after(16ms)` à
l'état minimisé/occulté de la fenêtre core (via
`ViewportInfo::minimized`/`focused`), et laisser le rendu à la demande sinon.
Gain batterie important pour un risque nul sur la fonctionnalité §5. Voir aussi
le clignotement du curseur (`editor/lib.rs:1249`) qui suffit déjà à animer
l'éditeur focalisé.

### 4.2 🟠 Réindexation : ~12 requêtes SQL non préparées par fichier

**Où :** `modules/file_tree/src/indexer.rs:555‑679` (`index_one`).

Chaque fichier indexé exécute une douzaine de `conn.execute("INSERT…"/"DELETE…")`
en chaîne. Chaque appel **recompile** le SQL. Sur un `full_scan` de N fichiers,
c'est ~12·N compilations. La cible « < 200 ms pour 500 fichiers » tient
aujourd'hui, mais c'est le premier poste qui mord sur un gros projet et à chaque
réindexation incrémentale.

**Correctif recommandé :** `Connection::prepare_cached` (cache de statements) ou
statements préparés hors boucle. Gain direct sur le temps de démarrage et sur la
latence de sauvegarde→réindex.

### 4.3 🟠 `publish()` clone toute la table d'index à chaque changement

**Où :** `modules/file_tree/src/indexer.rs:334` (`*snapshot.write() = mem.clone()`).

À **chaque** publication (donc à chaque sauvegarde d'un seul fichier, après
debounce), toute la `HashMap<PathBuf, FileStats>` est clonée en O(N). Sur un
projet de 5 000 fichiers, sauver une scène clone 5 000 entrées. Le mécanisme
snapshot + compteur de génération est un bon design ; c'est la granularité qui
coûte.

**Correctif recommandé :** publier des deltas, ou faire un `Arc`-swap d'une carte
immuable (l'UI lit l'`Arc`, l'indexeur en publie une nouvelle version), ou une
structure persistante type `im::HashMap` (partage structurel, clone O(1)).

### 4.4 🟠 `full_scan` réécrit tout au démarrage sans consulter `last_modified`

**Où :** `modules/file_tree/src/indexer.rs:467` (`full_scan`), mtime stocké en
`:539` mais jamais relu pour arbitrer.

Le `last_modified` est bien **stocké** dans la table `files`, mais jamais
**utilisé** pour sauter les fichiers inchangés. À chaque lancement, tous les
fichiers sont relus, re-parsés et ré-insérés, même si rien n'a changé depuis la
session précédente.

**Correctif recommandé :** comparer le mtime disque au mtime stocké et ne
re-parser que le delta. Transforme le démarrage de O(tous les fichiers) en
O(fichiers modifiés). (À conserver malgré tout un `full_scan` forcé pour la
commande explicite de re-scan.)

### 4.5 🟠 `refresh_orphans` : UPDATE pleine table à chaque changement incrémental

**Où :** `modules/file_tree/src/indexer.rs:727`, appelé dans `process_pending`
(`:766`) à chaque cycle de debounce.

`UPDATE wikilinks SET is_orphan = NOT EXISTS(SELECT 1 FROM files …)` balaie
**tous** les wikilinks avec une sous-requête corrélée sur `files`, à chaque
sauvegarde. Sur un gros corpus (milliers de liens), coût non négligeable pour la
mise à jour d'un seul fichier.

**Correctif recommandé :** restreindre la mise à jour aux `target_name` touchés
par le changement (ceux du fichier ré-indexé, plus ceux dont le stem vient
d'apparaître/disparaître), ou matérialiser l'ensemble des stems et diffuser les
deltas.

### 4.6 🟠 `sync_from_project` (notes) : projet entier lu + DB ouverte 3× sur le thread UI

**Où :** `modules/sticky_notes/src/db.rs:267` ; déclenché depuis
`modules/sticky_notes/src/lib.rs:216` (sur `list_refresh_pending`).

Un rafraîchissement du panneau de notes : lit **tous** les fichiers texte du
projet en mémoire pour y chercher des marqueurs, **ouvre la base trois fois** et
**recharge toutes les notes trois fois** (`load_notes` appelé 3×, chacun avec un
N+1 sur tags/liens), le tout **sur le thread UI**. Bien que ce ne soit pas
par-frame (déclenché sur événement de rafraîchissement), c'est un à-coup visible
sur un gros projet.

**Correctif recommandé :** une seule connexion réutilisée, un seul `load_notes`,
et déporter le scan de fichiers hors du thread UI (ou l'appuyer sur l'index
`file_tree` déjà construit plutôt que de re-scanner le disque).

### 4.7 🟡 Constats mineurs (impact faible / rare)

- **`poll_watcher` lit tout le fichier sur le thread UI** par événement notify
  (`editor/lib.rs:422`, `disk_really_changed` lit+hashe le fichier entier) +
  `canonicalize` par événement (`:414`). Borné au nombre de buffers ouverts ;
  à-coup possible si un gros fichier ouvert change en externe.
- **Backup : `canonicalize()` par entrée** (`core/src/backup.rs:227`) pour tester
  l'appartenance au dossier de destination, et `walk()` matérialise tous les
  chemins avant d'archiver (`:262`). Thread de fond toutes les 30 min → non
  urgent, mais O(fichiers) en syscalls et en mémoire.
- **`parse_typst` fait ~6 passes sur le corps** (2× `split_whitespace`,
  `chars().count()`, wikilinks, tags, date-marker ; `indexer.rs:795`). Correct
  pour de petits fichiers ; un tokenizer en une passe aiderait sur de très
  grandes scènes.
- **`read_clipboard` recrée un `arboard::Clipboard` à chaque collage**
  (`editor/input.rs:31`). arboard recommande une instance persistante ; collage
  rare → micro.
- **`load_note`/`delete_note` chargent toutes les notes pour en trouver une**
  (`sticky/db.rs:145`, `load_notes().find(id)`), et `load_notes` fait du N+1.
  Négligeable tant que les notes sont peu nombreuses.

---

## 5. Process et hygiène (hors code, mais structurant)

### 5.1 🟡 Pas d'intégration continue

`.github/workflows/` est **absent**. 35 fichiers portent des tests, 3 suites
d'intégration existent, et un script de validation maison (VOENPRED) est
référencé — mais **rien ne garantit automatiquement** que `fmt`/`clippy`/`test`
restent verts à chaque push.

**Recommandation :** un workflow CI minimal reproduisant les étapes VOENPRED :
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all`. C'est le filet qui protège tous les points forts du §2.

### 5.2 🟡 `.cargo/audit.toml` référencé mais absent + `cargo audit` en échec

`Cargo.toml:61‑65` annonce que les advisories ignorées sont documentées dans
`.cargo/audit.toml` — **ce fichier n'existe pas**. Par ailleurs le rapport
VOENPRED montre `cargo audit` en échec : **2 vulnérabilités + 2 crates non
maintenues** dans l'arbre de 418 dépendances.

L'analyse de ces advisories relève de la **sécurité**, explicitement **hors du
périmètre demandé** — je ne la conduis donc pas ici. Constat factuel seulement :
la référence de config est cassée et l'étape audit est rouge ; à traiter dans un
passage dédié « dépendances/sécurité ».

---

## 6. Plan d'action priorisé

| # | Action | Axe | Sévérité | Effort | Fichier(s) |
|---|---|---|---|---|---|
| 1 | Écritures atomiques (temp + rename) | Solidité | 🔴 | Faible | `buffer.rs:316`, `sticky/db.rs:369/389`, `typst_render.rs:383` |
| 2 | WAL + `busy_timeout` sur toutes les connexions `index.db` | Solidité | 🟠 | Faible | `indexer.rs:159/364`, `sticky/db.rs:66` |
| 3 | Heartbeat 60 fps conditionné à la minimisation | Efficacité/énergie | 🟠 | Faible | `app/main.rs:571` |
| 4 | Préserver EOL + newline final des marqueurs de notes | Solidité | 🟠 | Faible | `sticky/db.rs:357/373` |
| 5 | `prepare_cached` dans `index_one` | Efficacité | 🟠 | Faible | `indexer.rs:555` |
| 6 | Démarrage incrémental (skip via mtime) | Efficacité | 🟠 | Moyen | `indexer.rs:467` |
| 7 | Publier des deltas au lieu de cloner la carte | Efficacité | 🟠 | Moyen | `indexer.rs:334` |
| 8 | Suppression d'index robuste au chemin non canonique | Solidité | 🟡 | Faible | `indexer.rs:695` |
| 9 | `sync_from_project` : 1 connexion, 1 load, scan hors UI | Efficacité | 🟡 | Moyen | `sticky/db.rs:267` |
| 10 | UUID via `getrandom` | Solidité | 🟡 | Faible | `sticky/db.rs:455` |
| 11 | CI (fmt + clippy + test) | Process | 🟡 | Faible | `.github/workflows/` |
| 12 | Recréer `.cargo/audit.toml` ou corriger la référence | Process | 🟡 | Faible | `Cargo.toml:61` |

**Ordre conseillé :** 1 → 2 → 3 → 4, puis les optimisations de montée en charge
(5 → 6 → 7) quand les projets grossiront, puis l'hygiène (11 → 12).

---

## 7. Ce qui n'a délibérément PAS été audité

- **Sécurité** (exclue à la demande) : advisories `cargo audit`, surface IPC en
  tant que vecteur d'attaque, injection dans les commandes externes (`typst`,
  `okular`, `kate`, `xdg-open`), permissions de fichiers.
- **Exactitude fonctionnelle métier** (ex. justesse du comptage de mots vis-à-vis
  d'une norme éditoriale) : seuls la robustesse et l'efficacité du code ont été
  regardées.
- **Modules désactivés par défaut** (`timeline`, `claude_terminal`) : survolés,
  pas audités en profondeur puisque hors du chemin nominal.
- **Rendu visuel / ergonomie** : hors périmètre technique.

---

*Audit réalisé en lecture seule. Aucune modification de code. Les numéros de
ligne renvoient au commit `68f3159`.*
