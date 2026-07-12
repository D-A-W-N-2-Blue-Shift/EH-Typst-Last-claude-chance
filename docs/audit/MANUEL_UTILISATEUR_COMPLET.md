# Manuel utilisateur — Engram_Hive (édition Typst)

*Guide simple et complet. Chaque chapitre est autonome : tu peux sauter
directement à ce qui t'intéresse.*

---

## Sommaire

1. [Présentation](#chapitre-1--présentation)
2. [Prérequis et installation](#chapitre-2--prérequis-et-installation)
3. [Premier lancement](#chapitre-3--premier-lancement)
4. [Ouvrir ou créer un projet](#chapitre-4--ouvrir-ou-créer-un-projet)
5. [La structure d'un projet](#chapitre-5--la-structure-dun-projet)
6. [Naviguer : l'arbre de fichiers](#chapitre-6--naviguer--larbre-de-fichiers)
7. [Écrire dans l'éditeur](#chapitre-7--écrire-dans-léditeur)
8. [Mise en forme, typographie, snippets, liens](#chapitre-8--mise-en-forme-typographie-snippets-liens)
9. [Raccourcis clavier](#chapitre-9--raccourcis-clavier)
10. [La palette de commandes](#chapitre-10--la-palette-de-commandes)
11. [Voir le rendu PDF (Typst)](#chapitre-11--voir-le-rendu-pdf-typst)
12. [Les notes liées](#chapitre-12--les-notes-liées)
13. [Chercher dans le corpus](#chapitre-13--chercher-dans-le-corpus)
14. [COH2B : l'assistant en lecture seule](#chapitre-14--coh2b--lassistant-en-lecture-seule)
15. [Le cockpit : réglages](#chapitre-15--le-cockpit--réglages)
16. [Sauvegardes automatiques (WrapDrive)](#chapitre-16--sauvegardes-automatiques-wrapdrive)
17. [Personnaliser le logiciel](#chapitre-17--personnaliser-le-logiciel)
18. [Dépannage](#chapitre-18--dépannage)
19. [Limites connues](#chapitre-19--limites-connues)

---

## Chapitre 1 — Présentation

**Engram_Hive (édition Typst)** est un atelier d'écriture pour les longs textes
(romans, univers de fiction). Il réunit dans une seule application :

- un **éditeur de texte** confortable, une fenêtre par fichier ;
- un **arbre de fichiers** qui comprend la structure de ton projet ;
- un **rendu PDF** local via le moteur **Typst** ;
- des **notes** attachées à tes fichiers ;
- une **recherche plein texte** dans tout ton corpus ;
- un **assistant en lecture seule** (COH2B) pour interroger ton texte ;
- des **sauvegardes automatiques** invisibles.

**Philosophie :** pas de barre de menus, pas d'onglets. Tout passe par une
**palette de commandes** (comme un « menu qui se cherche au clavier ») et par
des raccourcis. Rien ne se cache : les erreurs s'affichent, ton texte reste
visible et éditable en clair.

Les fichiers d'écriture sont au format **`.typ`** (Typst), mais l'éditeur ouvre
aussi `.md`, `.txt`, et divers fichiers de configuration.

---

## Chapitre 2 — Prérequis et installation

### 2.1 Ce qu'il te faut

- **Le binaire `typst`** installé et accessible dans ton `PATH`. C'est lui qui
  fabrique les PDF. Sans lui, tout le reste fonctionne, mais le rendu PDF est
  indisponible (le logiciel te le dira clairement).
- **Un lecteur PDF** (Okular recommandé) et, en option, l'éditeur **Kate**, si
  tu veux ouvrir tes fichiers/PDF dans ces outils externes.
- **Système :** Linux (Wayland natif ou X11).

### 2.2 Vérifier que Typst est présent

Dans un terminal :

```bash
typst --version
```

Si une version s'affiche, tu es prêt. Sinon, installe Typst puis relance
l'application.

### 2.3 Lancer l'application

Lance le binaire de l'application comme d'habitude. Au premier démarrage, elle
crée toute seule ses dossiers de configuration et de données (chapitre 3).

---

## Chapitre 3 — Premier lancement

Au tout premier lancement, l'application crée deux dossiers personnels :

| Dossier | Rôle |
|---|---|
| `~/.config/engram_hive_typst/` | **Tes réglages** : raccourcis, thème, config des modules, snippets, polices |
| `~/.local/share/engram_hive_typst/` | **Les données** : journaux, sauvegardes, cache de rendu PDF |

Contenu utile du dossier de configuration :

```
~/.config/engram_hive_typst/
├── modules.ron              quels modules sont actifs
├── theme.ron                couleurs et polices
├── keybinds.ron             tes raccourcis (modifiables)
├── snippets/*.toml          tes modèles de texte
├── fonts/*.ttf              tes polices perso
└── modules/editor/editor.ron  réglages de l'éditeur
```

> **Bon à savoir :** ces fichiers sont créés avec les valeurs par défaut et des
> commentaires. **Commenter une ligne = revenir au défaut.** Tu peux tout éditer
> à la main (chapitre 17) ou via le cockpit (chapitre 15).

Chaque module tient aussi son propre **journal** dans
`~/.local/share/engram_hive_typst/logs/modules/`.

---

## Chapitre 4 — Ouvrir ou créer un projet

Un « projet », c'est simplement un **dossier** qui contient ton travail.

### 4.1 Ouvrir un projet existant

1. Ouvre la **palette** : **Ctrl+Shift+P**.
2. Tape `project` et choisis **« project: ouvrir projet »**.
3. Sélectionne le dossier de ton projet dans le sélecteur intégré.

### 4.2 Créer un nouveau projet

1. Palette → **« project: nouveau projet »**.
2. Choisis un dossier.
3. Si le dossier est **vide ou incomplet**, l'application **demande confirmation**
   avant de créer la structure de référence (chapitre 5). Elle ne t'impose
   jamais une arborescence sans te prévenir.

### 4.3 Le sélecteur de dossier

Le choix du dossier se fait **dans l'application** (pas de fenêtre système
externe). Le dernier projet ouvert est mémorisé et rouvert au lancement suivant.

---

## Chapitre 5 — La structure d'un projet

Quand tu laisses l'application créer un projet, elle propose une arborescence de
référence, pensée pour l'écriture longue. Les grandes lignes :

```
<projet>/
├── 01_architecture/     plan, chronologie (biographies, événements), modifications
├── 02_worldbuilding/    personnages, lieux, concepts, lore
├── 05_texte/            scenes/ et chapitres/
├── 06_session/          ton espace de travail « en cours » (voir 5.1)
└── 09_poubelle/         scènes coupées — visible, jamais vraiment supprimé
```

Tu n'es **pas obligé** de tout utiliser. Le logiciel s'adapte à ce qui existe.

### 5.1 Le dossier « en cours » (session)

`06_session/` est ta zone de travail active. Elle peut contenir des **raccourcis**
(symlinks) vers les fichiers que tu es en train d'écrire, sans les déplacer de
leur emplacement d'origine. Ainsi, un même fichier reste rangé à sa place **et**
accessible dans « en cours ».

Trois commandes de palette gèrent cette zone :

- **« context: ajouter à en cours »** — mettre le fichier courant dans la table
  de travail.
- **« context: vider en cours »** — nettoyer la table.
- **« context: aller à l'original »** — depuis un raccourci, sauter au fichier
  d'origine.

> L'ancien nom `06_en_cours/` reste reconnu pour compatibilité — les deux
> fonctionnent.

### 5.2 Fiches et métadonnées (frontmatter)

Une fiche (personnage, scène…) commence souvent par un **bloc de métadonnées**
en YAML, soit nu, soit encapsulé dans un commentaire Typst :

```typst
/*
---
tags:
  - personnage
goal: 1500
statut: en cours
---
*/

= Svetlana Volkova

Le corps du texte commence ici…
```

Champs utiles reconnus :

| Champ | Effet |
|---|---|
| `goal:` | objectif de mots (barre de progression) |
| `statut:` / `status:` | état affiché (voir ci-dessous) |
| `tags:` | étiquettes (dont `personnage` pour la coloration des fiches) |
| `date:`, `lieu:`, `ordre:`, `personnages:` | métadonnées de scène (chronologie) |

États reconnus pour `statut:` : `brouillon`/`draft`, `wip`/`en cours`,
`a_relire`, `ready`, `blocked`. Un statut inconnu n'est pas une erreur : il est
simplement ignoré.

> **Important :** les mots du bloc de métadonnées **ne comptent jamais** dans le
> total de mots de ton texte. Seul le corps compte.

---

## Chapitre 6 — Naviguer : l'arbre de fichiers

L'arbre de fichiers occupe le bas de la fenêtre principale. Il affiche la
structure de ton projet et des **statistiques** (mots, objectif, statut).

Ce que tu peux faire depuis l'arbre :

- **ouvrir un `.typ`** dans l'éditeur ;
- **ouvrir un `.md`, `.txt`, `.ron`, `.toml`, `.json`, `.csv`, `.tsv`** en texte ;
- **ouvrir un `.pdf`** dans Okular ;
- **ouvrir une image** dans le système.

Commandes de palette liées à l'arbre :

- **« tree: nouveau fichier »**
- **« tree: nouveau dossier »**
- **« tree: renommer »**

> **Mode focus :** quand tu passes l'éditeur en plein écran (**F11**), l'arbre
> se masque pour te laisser seul avec ton texte, puis réapparaît quand tu sors.

L'arbre se met à jour tout seul : dès que tu ajoutes, modifies ou supprimes un
fichier (dans l'app ou en dehors), l'index se rafraîchit après un court délai.

---

## Chapitre 7 — Écrire dans l'éditeur

### 7.1 Une fenêtre par fichier

Chaque fichier ouvert s'affiche dans **sa propre fenêtre**. Tu peux en ouvrir
plusieurs et les disposer comme tu veux (le logiciel ne t'impose aucune
disposition).

Ouvrir une **seconde vue** du même fichier : **Ctrl+Shift+N**.

### 7.2 Le confort d'écriture

- **Colonne centrée :** par défaut, le texte est présenté sur une colonne de
  **72 caractères**, centrée, marges neutres. (Réglable, `0` = pleine largeur.)
- **Machine à écrire (typewriter) :** la ligne courante reste au centre vertical.
  Bascule : **Ctrl+Alt+T**.
- **Ligne courante surlignée** discrètement (activé par défaut).
- **Numéros de ligne :** désactivés par défaut, activables (chapitre 17).
- **Police :** Georgia 15 pt par défaut. Tu peux déposer tes propres polices
  dans `~/.config/engram_hive_typst/fonts/`.

### 7.3 Le zoom (comportement voulu)

Deux zooms **distincts**, c'est **intentionnel** :

| Geste | Effet |
|---|---|
| **Ctrl + =** / **Ctrl + -** | zoom **global** : toutes les fenêtres d'édition en même temps |
| **Ctrl + molette** | zoom **local** : seulement la fenêtre sous le pointeur |
| **Ctrl + 0** | revenir à la taille de base |

### 7.4 Annuler / rétablir

- **Ctrl+Z** annule, **Ctrl+Shift+Z** rétablit.
- Les frappes qui se suivent s'annulent **par groupes** cohérents : une frappe
  et la petite correction typographique associée s'annulent **ensemble**, comme
  tu t'y attends. Une rafale de frappes = un seul « annuler ».

### 7.5 Enregistrer

- **Ctrl+S** enregistre immédiatement.
- **Sauvegarde automatique :** toutes les **5 minutes** par défaut (réglable,
  `0` = désactivée), et **tout est sauvé à la fermeture**.
- **Fichiers `.typ` :** ils sont aussi **enregistrés automatiquement ~½ seconde
  après que tu arrêtes de taper**, pour rafraîchir le rendu PDF (chapitre 11).

> **Si un fichier change en dehors de l'application** (autre éditeur, git…), une
> alerte apparaît. **Ctrl+Shift+R** recharge la version du disque — *attention,
> tes modifications locales non sauvées seraient alors perdues.* À toi de choisir.

---

## Chapitre 8 — Mise en forme, typographie, snippets, liens

### 8.1 Coloration

L'éditeur colore de façon légère et lisible : titres Typst (`=`, `==`…), blocs
de code, tableaux, frontmatter YAML, commentaires, **wikilinks** et **#tags**.

### 8.2 Typographie intelligente

Activée par défaut (langue **française**). Elle transforme au fil de la frappe :

- guillemets → « … » (français) ou " … " (anglais) ;
- `--` → tiret cadratin — ;
- apostrophes droites → courbes ;
- points de suspension `...` → … ;
- espaces insécables aux bons endroits.

Désactivable par fichier via le frontmatter : `smart_typography: false`, ou
changer la langue : `language: "en"`.

### 8.3 Auto-fermeture des paires

Les paires `()`, `[]` et `[[` se ferment automatiquement. Les guillemets `"` et
`'` sont **volontairement exclus** (gérés par la typographie intelligente).

### 8.4 Raccourcis de mise en forme

| Action | Raccourci |
|---|---|
| Gras | **Ctrl+B** |
| Italique | **Ctrl+I** |
| Code | **Ctrl+`** |
| Lien | **Ctrl+K** |
| Wikilink | **Ctrl+Shift+K** |

### 8.5 Snippets (modèles de texte)

Tape un **préfixe** commençant par `;` pour insérer un modèle. Fournis par
défaut :

| Déclencheur | Insère |
|---|---|
| `;sce` | squelette de **scène** |
| `;per` | squelette de **personnage** |
| `;cha` | squelette de **chapitre** |
| `;tab` | un **tableau** |
| `;date` | une **date** |
| `;dt` | date/heure |

Tu peux ajouter les tiens dans `~/.config/engram_hive_typst/snippets/` (un
fichier `.toml` par snippet).

### 8.6 Wikilinks : relier tes fichiers

Écris `[[Nom du fichier]]` pour créer un lien vers une autre fiche.
`[[Svetlana|la Russe]]` affiche « la Russe » mais pointe vers *Svetlana*.

- Une **auto-complétion** apparaît dès `[[` + un caractère.
- Un lien **valide** (le fichier existe) et un lien **orphelin** (le fichier
  n'existe pas encore) sont colorés différemment.
- **Cliquer sur un lien orphelin** propose de **créer** le fichier manquant, puis
  l'ouvre.

### 8.7 Tags

Écris `#climax`, `#mystere`, `#acte/2`… n'importe où dans le texte. Ils sont
colorés et indexés (recherche, futur regroupement).

### 8.8 Tableaux

Un bloc de tableau Markdown (`| a | b |`) est rendu sous forme de **grille
éditable** (une cellule = un champ). La largeur d'une colonne s'ajuste à son
contenu. Le tableau est ré-écrit au format texte quand tu quittes le bloc.

### 8.9 Table des matières

**Ctrl+Shift+O** ouvre la table des matières du fichier (navigation rapide par
titres).

---

## Chapitre 9 — Raccourcis clavier

Voici les raccourcis **par défaut**. Tous sont **modifiables** dans
`~/.config/engram_hive_typst/keybinds.ron` (chapitre 17).
*(« Ctrl » vaut « Cmd » sur macOS.)*

### Fichier et vues

| Action | Raccourci |
|---|---|
| Enregistrer | **Ctrl+S** |
| Recharger depuis le disque | **Ctrl+Shift+R** |
| Nouvelle vue du fichier | **Ctrl+Shift+N** |
| Table des matières | **Ctrl+Shift+O** |
| Mode focus (plein écran) | **F11** |
| Machine à écrire | **Ctrl+Alt+T** |

### Édition

| Action | Raccourci |
|---|---|
| Annuler | **Ctrl+Z** |
| Rétablir | **Ctrl+Shift+Z** |
| Tout sélectionner | **Ctrl+A** |
| Rechercher | **Ctrl+F** |
| Remplacer | **Ctrl+H** |
| Gras / Italique / Code | **Ctrl+B** / **Ctrl+I** / **Ctrl+`** |
| Lien / Wikilink | **Ctrl+K** / **Ctrl+Shift+K** |

### Zoom

| Action | Raccourci |
|---|---|
| Agrandir (global) | **Ctrl+=** |
| Réduire (global) | **Ctrl+-** |
| Réinitialiser | **Ctrl+0** |
| Zoom local | **Ctrl+molette** |

### Rendu Typst

| Action | Raccourci |
|---|---|
| Relancer le rendu | **Ctrl+Alt+R** |
| Ouvrir dans Kate | **Ctrl+Alt+K** |
| Ouvrir dans Okular | **Ctrl+Alt+O** |
| Ouvrir le dossier de rendu | **Ctrl+Alt+D** |
| Aller à l'erreur | **Ctrl+Alt+E** |

### Global

| Action | Raccourci |
|---|---|
| Ouvrir la palette de commandes | **Ctrl+Shift+P** |

---

## Chapitre 10 — La palette de commandes

La **palette** (**Ctrl+Shift+P**) est le cœur de la navigation : au lieu de
chercher dans des menus, tu tapes quelques lettres et tu choisis. Commandes
disponibles :

| Commande | Ce qu'elle fait |
|---|---|
| **project: ouvrir projet** | ouvrir un dossier de projet |
| **project: nouveau projet** | créer un projet (avec confirmation) |
| **project: chercher dans le projet** | recherche plein texte (chapitre 13) |
| **tree: nouveau fichier** | créer un fichier |
| **tree: nouveau dossier** | créer un dossier |
| **tree: renommer** | renommer l'élément sélectionné |
| **context: ajouter à en cours** | mettre le fichier courant dans « en cours » |
| **context: vider en cours** | vider la table « en cours » |
| **context: aller à l'original** | depuis un raccourci, aller à l'original |
| **sticky notes: ouvrir la fenêtre** | ouvrir les notes (chapitre 12) |
| **sticky notes: afficher / masquer** | basculer la fenêtre des notes |
| **backup: sauvegarder maintenant** | déclencher une sauvegarde immédiate |
| **backup: afficher le dossier de sauvegarde** | ouvrir le dossier des archives |
| **wrapdrive: ouvrir le panel** | panneau Analyse / Dialogue projet |
| **cockpit: ouvrir la fenêtre de configuration** | réglages (chapitre 15) |
| **cockpit: afficher / masquer** | basculer le cockpit |
| **coh2b: ouvrir** | l'assistant en lecture seule (chapitre 14) |
| **timeline: ouvrir la vue** | vue chronologique* |
| **timeline: chronologie** | liste triée par date* |

\* *Timeline et COH2B sont des modules qui peuvent être désactivés par défaut ;
ils s'activent via `modules.ron` (chapitre 17).*

---

## Chapitre 11 — Voir le rendu PDF (Typst)

### 11.1 Comment ça marche

Le rendu est produit **localement** par le binaire `typst`. Le déroulé normal :

1. Ouvre un fichier `.typ`.
2. Écris.
3. Le fichier est enregistré automatiquement peu après ta dernière frappe (ou
   avec **Ctrl+S**), puis le **PDF se régénère tout seul**.
4. Ouvre le résultat avec **Ctrl+Alt+O** (Okular).

Tu peux aussi forcer un rendu avec **Ctrl+Alt+R**.

### 11.2 En cas d'erreur Typst

Si ton fichier contient une erreur de syntaxe Typst :

- le **dernier PDF valide reste affiché** (tu ne perds pas ta prévisualisation) ;
- **Ctrl+Alt+E** t'amène directement à l'endroit de l'erreur.

### 11.3 Où sont les PDF

Les PDF sont mis en cache dans
`~/.local/share/engram_hive_typst/typst-render/`. La commande **Ctrl+Alt+D**
ouvre le dossier de rendu du fichier courant.

### 11.4 La racine du projet Typst

Pour résoudre les `#import`/`#include`, Typst a besoin d'une **racine**.
L'application la détecte en remontant les dossiers jusqu'à trouver
`templates/engram.typ`. Si tu utilises des imports partagés, garde ce fichier de
template à la racine de ton projet.

---

## Chapitre 12 — Les notes liées

Le panneau **sticky notes** attache des notes à tes fichiers.

### 12.1 Ouvrir les notes

Palette → **« sticky notes: ouvrir la fenêtre »**.

### 12.2 Ce que tu peux faire

- créer une note (rattachée au fichier courant et à une ligne) ;
- lui ajouter des **tags** (type + valeur) ;
- **lier** d'autres fichiers ;
- rouvrir, modifier, **supprimer** une note ;
- repérer les notes **orphelines** (dont le fichier a disparu).

### 12.3 Où sont stockées les notes

Les notes vivent dans la **base de données du projet**
(`<projet>/.engram/index.db`). Pour retrouver une note même si tu déplaces le
fichier, l'application insère une **petite marque discrète** dans le fichier :

- dans un `.typ` : `/* note:<identifiant> */`
- dans un `.md`/`.txt` : `<!-- note:<identifiant> -->`

Ces marques sont invisibles au rendu et servent d'ancre.

> ⚠️ **À savoir (voir l'audit technique) :** l'insertion d'une marque réécrit le
> fichier. Aujourd'hui, cela peut modifier des détails invisibles (type de fin
> de ligne, ligne vide finale). Sans conséquence sur ta lecture, mais à garder à
> l'esprit si tu suis tes fichiers avec git. Un correctif est recommandé dans le
> rapport d'audit.

---

## Chapitre 13 — Chercher dans le corpus

La recherche plein texte s'appuie sur l'index du projet.

1. Palette → **« project: chercher dans le projet »**.
2. Tape ta requête.
3. Valide avec **Entrée** ou le bouton **Rechercher**.

Chaque résultat affiche le **fichier**, sa **section**, un **extrait** et le
**nombre de mots**. La recherche est classée par pertinence.

> La recherche a besoin de l'index `.engram/` du projet. S'il est absent, il se
> reconstruit tout seul au chargement du projet (très rapide).

---

## Chapitre 14 — COH2B : l'assistant en lecture seule

COH2B interroge ton texte **sans jamais le modifier**.

Tu choisis :

- **le périmètre** : fichier courant, texte sélectionné, corpus, projet complet ;
- **le mode** : question libre, cohérence micro, cohérence macro, piste
  narrative, audit ;
- **le fournisseur** et le mode d'authentification.

COH2B affiche la **réponse**, l'**usage** (si disponible), un **historique local**
et un **journal horodaté**. Quand c'est pertinent, il joint automatiquement des
extraits du corpus (fichier, section, extrait, nombre de mots) à ta question.

> COH2B est **strictement consultatif** : il **ne touche pas** à ton projet.

---

## Chapitre 15 — Le cockpit : réglages

Le **cockpit** est le tableau de bord de configuration. Ouvre-le via la palette
(**« cockpit: ouvrir la fenêtre de configuration »**).

Il permet de :

- consulter et ajuster les **réglages** ;
- vérifier la **configuration** (thème, modules, fournisseur d'assistant) ;
- **recharger** certaines valeurs sans tout relancer.

Chaque réglage possède un **miroir `.md` pédagogique** qui explique ce qu'il
fait, pour éviter de bricoler à l'aveugle.

---

## Chapitre 16 — Sauvegardes automatiques (WrapDrive)

**WrapDrive** archive ton projet en fond, **sans rien te demander**.

| Réglage | Défaut |
|---|---|
| Sauvegarde automatique | activée |
| Intervalle | toutes les **30 minutes** |
| Rétention | **30 jours** (les archives plus vieilles sont purgées) |
| Format | `.tar.gz` |
| Destination | `~/.local/share/engram_hive_typst/backups/` |

- **Sauvegarde immédiate :** palette → **« backup: sauvegarder maintenant »**.
- **Voir les archives :** palette → **« backup: afficher le dossier de
  sauvegarde »**.

WrapDrive est **invisible** : aucune notification, sauf **en cas d'erreur** (elle
s'affiche alors dans la barre de statut). Tu peux aussi configurer une **copie
supplémentaire** de la dernière archive vers un autre emplacement (disque
externe, dossier synchronisé…).

> **Conseil :** laisse WrapDrive activé. Combiné à la sauvegarde automatique de
> l'éditeur, c'est ton filet de sécurité principal.

---

## Chapitre 17 — Personnaliser le logiciel

Tout se règle dans des fichiers texte du dossier de configuration. **Commenter
une ligne = revenir au défaut.**

### 17.1 Les réglages de l'éditeur

`~/.config/engram_hive_typst/modules/editor/editor.ron` :

| Réglage | Défaut | Rôle |
|---|---|---|
| `font_editor` | `"Georgia"` | police d'écriture |
| `font_size` | `15` | taille de base (pt) |
| `column_width` | `72` | largeur de colonne (caractères ; `0` = pleine largeur) |
| `typewriter_mode` | `false` | ligne courante centrée |
| `smart_typography` | `true` | typographie intelligente |
| `language` | `"fr"` | langue (guillemets, insécables) |
| `wikilink_autocomplete` | `true` | auto-complétion des `[[` |
| `snippet_prefix` | `";"` | préfixe des snippets |
| `line_numbers` | `false` | numéros de ligne |
| `highlight_current_line` | `true` | surlignage de la ligne courante |
| `autosave_minutes` | `5` | sauvegarde auto (`0` = off) |
| `auto_close_pairs` | `true` | auto-fermeture des paires |

### 17.2 Les raccourcis

`~/.config/engram_hive_typst/keybinds.ron`. Format : `"Ctrl+Shift+R"`, `"F11"`,
`"Alt+X"`… Un raccourci mal écrit est signalé, et le défaut est conservé.

### 17.3 Le thème

`~/.config/engram_hive_typst/theme.ron` : couleurs et polices de l'interface.

### 17.4 Les modules actifs

`~/.config/engram_hive_typst/modules.ron` : la liste des modules chargés. C'est
ici qu'on active/désactive des modules comme la timeline ou COH2B. Un
`modules.ron` illisible est sauvegardé en `.bak` et régénéré (aucun blocage).

### 17.5 Les polices et snippets

- Polices perso : dépose des `.ttf` dans `~/.config/engram_hive_typst/fonts/`.
- Snippets perso : un `.toml` par modèle dans
  `~/.config/engram_hive_typst/snippets/`.

---

## Chapitre 18 — Dépannage

**Le rendu PDF ne se lance pas.**
→ Vérifie que `typst` est installé (`typst --version`) et dans ton `PATH`.
Vérifie que le fichier courant est bien un `.typ`.

**« Aucun projet ouvert. »**
→ Ouvre d'abord un projet (palette → *project: ouvrir projet*).

**La recherche corpus échoue ou ne renvoie rien.**
→ L'index `.engram/` doit exister (il se crée à l'ouverture du projet). Réessaie
après quelques secondes. Si le message d'erreur mentionne la base occupée,
réessaie : une réindexation était sans doute en cours (voir l'audit, §3.2).

**« … a changé sur le disque. »**
→ Un autre programme a modifié le fichier. **Ctrl+Shift+R** recharge la version
du disque (tes modifications non sauvées seraient perdues) ; sinon, sauve pour
écraser avec ta version.

**Un panneau (notes, cockpit, timeline) ne s'affiche pas.**
→ Ouvre-le via la palette. S'il s'agit d'un module désactivé, active-le dans
`modules.ron` (chapitre 17.4).

**Une base d'index semble corrompue.**
→ Supprime `<projet>/.engram/index.db` et relance : l'index se reconstruit en
quelques centaines de millisecondes.

**Où sont les journaux ?**
→ `~/.local/share/engram_hive_typst/logs/modules/<module>.log`.

---

## Chapitre 19 — Limites connues

Ce que l'application **ne fait pas** (et n'a pas vocation à faire) :

- pas de **WYSIWYG** complet (l'aperçu se fait via le PDF Typst) ;
- pas d'**édition collaborative** ni de **cloud** ;
- pas de **moteur de cohérence destructif** : COH2B **lit**, ne réécrit pas ;
- **aucune modification automatique** de ton projet par l'assistant.

Deux comportements **voulus** à ne pas confondre avec des bugs :

- le **double zoom** (global au clavier, local à la molette) est intentionnel ;
- les fichiers `.typ` sont **enregistrés automatiquement** peu après ta frappe,
  pour le rendu.

Enfin, pour les points d'amélioration technique (sauvegarde atomique,
consommation d'énergie, etc.), voir les rapports d'audit joints
(`AUDIT_TECHNIQUE.md` et `AUDIT_VERSION_SIMPLE.md`).

---

*Bonne écriture.*
