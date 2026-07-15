# Guide de validation fonctionnelle — Nexus (Hive-RBMK-mod-Tcherenkov)

Format normatif (doctrine §12) : chaque étape précise action, prérequis,
résultat attendu, erreurs possibles, raison diagnostique. Aucune étape ne
se contente de « ça démarre » ou « la fenêtre s'ouvre » — chaque résultat
attendu est une observation précise et vérifiable.

**Pourquoi ce guide existe** : cet environnement de développement est
headless (pas d'écran, `wayland` absent). La preuve produite ici (compile +
tests + binaire produit) couvre la logique ; elle NE couvre PAS le rendu
visuel ni l'interaction clavier/souris réelle. Ce guide est à exécuter sur
une machine avec écran (Fedora ou équivalent Wayland/X11) pour fermer cet
écart.

## 0. Prérequis généraux

- `wayland-devel` (Fedora) ou `libwayland-dev` (Debian) installé.
- `cargo build --workspace` termine sans erreur (déjà prouvé en headless à
  chaque incrément — revérifier sur la machine cible avant de commencer).
- Binaires : `target/debug/Hive_RBMK_Tcherenkov` et `target/debug/nexus_inspect`.
- Un dossier vide et accessible en écriture pour servir de projet de test.

---

## 1. Hub Nexus — ouverture/création de projet

### 1.1 Création d'un nouveau projet

- **Action** : lancer `./target/debug/Hive_RBMK_Tcherenkov`, saisir le chemin d'un dossier
  vide dans le champ, cliquer « ✨ Nouveau (créer la structure) ».
- **Prérequis** : le dossier existe et est accessible en écriture (ou
  n'existe pas encore — `canonicalize` échoue alors, voir erreurs).
- **Résultat attendu** : le dossier contient exactement
  `01_journal/`, `02_sante/{medicaments.typst,notes_sante.typst}`,
  `03_todo/{backlog.typst,archive/}`, `04_articles/`, `05_reference/`,
  `.engram/nexus.db`. Le hub affiche « nexus.db : 10/10 tables présentes ».
- **Erreurs possibles** : « Dossier introuvable » si le chemin n'existe pas
  du tout (créer le dossier lui-même avant, ou utiliser un chemin absolu) ;
  message d'erreur explicite (pas de crash) si le dossier est en lecture
  seule.
- **Raison diagnostique** : si le compte de tables n'est pas 10/10, le
  schéma SQL (`nexus_db::SCHEMA`) n'a pas été appliqué — vérifier que
  `nexus_db::open_db` a bien été appelé sur ce chemin exact.

### 1.2 Ré-ouverture d'un projet existant

- **Action** : fermer `Hive_RBMK_Tcherenkov`, le relancer, saisir le MÊME chemin, cliquer
  « 📂 Ouvrir » (pas « Nouveau »).
- **Prérequis** : le projet 1.1 existe déjà.
- **Résultat attendu** : même statut « 10/10 tables présentes », aucune
  perte de données (le schéma est idempotent — `CREATE TABLE IF NOT
  EXISTS`), les 6 boutons de module apparaissent (🏥/📓/✅/📊/📰/🛠).
- **Erreurs possibles** : « Ce dossier n'est pas encore un projet Nexus »
  si on pointe « Ouvrir » vers un dossier sans `.engram/`.
- **Raison diagnostique** : ce message précis (pas une erreur générique)
  prouve que `project::is_nexus_project` distingue bien un dossier
  quelconque d'un projet Nexus réel.

---

## 2. Health — sommeil, médication, état psy

### 2.1 Sommeil — saisie et moyenne glissante

- **Action** : ouvrir Santé, sous-vue Sommeil, saisir durée=7.5, qualité=4,
  valider. Répéter avec des durées différentes sur au moins 7 jours
  simulés (ou modifier directement `sleep_log` via `nexus_inspect` pour
  aller plus vite).
- **Résultat attendu** : après la 7e nuit, l'écart en % à la moyenne
  personnelle 28 jours s'affiche (avant, rien ou un message d'insuffisance
  — comportement attendu, pas un bug).
- **Raison diagnostique** : la moyenne apparaît exactement à la 7e entrée
  parce que `MIN_SAMPLE_SLEEP = 7` (doc §5.6) est un seuil fixe, pas
  arrondi ni approximatif — 6 nuits ne doivent JAMAIS produire de moyenne.

### 2.2 Médication — prise et ressenti différé

- **Action** : dans le registre, ajouter un médicament (nom, molécule,
  dose par défaut). Cliquer « Prise » : timestamp pré-rempli à maintenant,
  dose pré-remplie à `dose_default`. Confirmer.
- **Résultat attendu** : la prise apparaît immédiatement dans l'historique
  du jour. Le champ « ressenti » n'est PAS proposé tout de suite.
- **Action suivante** : avancer l'horloge système de 3h (ou modifier
  `taken_at` via `nexus_inspect`/SQL direct pour le test), rouvrir la vue.
- **Résultat attendu** : le champ ressenti (1-5) est maintenant proposé.
- **Raison diagnostique** : le délai de 3h (doc §5.3) est un seuil
  temporel réel comparé à `taken_at`, pas un simple booléen "après la
  première visite".

### 2.3 État psy — radar et alerte

- **Action** : saisir un score < 2 sur une dimension, 3 jours calendaires
  consécutifs (pas 3 saisies le même jour).
- **Résultat attendu** : une alerte visuelle apparaît nommant exactement
  cette dimension. Une 4e saisie où le score remonte ≥ 2 fait disparaître
  l'alerte au prochain rafraîchissement.
- **Erreur possible à provoquer volontairement** : sauter un jour (saisir
  J1, J2, puis J4 sans J3) — l'alerte ne doit PAS se déclencher.
- **Raison diagnostique** : `low_dimension_alerts` exige des jours
  calendaires strictement consécutifs, jamais une extrapolation sur un
  jour manquant (doc §5.6 : aucune donnée inventée).

### 2.4 Redrop — friction minimale

- **Action** : depuis N'IMPORTE QUELLE fenêtre Nexus (hub, todo,
  dashboard…), `Ctrl+Shift+P` → choisir l'action Redrop dans la palette.
- **Résultat attendu** : popup à 2 champs (molécule pré-remplie si un seul
  médicament existe, dose pré-remplie), notification « logged » qui
  disparaît seule après 2 secondes, retour immédiat à la fenêtre d'origine.
- **Erreur possible** : si aucun projet n'est ouvert, message explicite
  (« aucun projet ouvert »), jamais un crash ni un popup vide.
- **Raison diagnostique** : le popup n'a JAMAIS de champ timestamp éditable
  (doc §6) — si un champ date/heure apparaît, c'est un écart au doc.

---

## 3. Journal

### 3.1 Auto-ouverture du jour

- **Action** : ouvrir Journal (bouton hub ou `Ctrl+Shift+J` depuis
  n'importe quelle fenêtre).
- **Résultat attendu** : le fichier `01_journal/AAAA/AAAA-MM-JJ.typst` du
  jour courant s'ouvre automatiquement, créé depuis le gabarit si absent.
- **Raison diagnostique** : re-cliquer `Ctrl+Shift+J` doit BASCULER
  (toggle) la fenêtre, pas en ouvrir une seconde.

### 3.2 Template configurable

- **Action** : éditer manuellement `~/.config/hive_rbmk_tcherenkov/engram.ron`,
  ajouter une section `"journal": (template: "Gabarit perso {date}\n")`.
  Ouvrir Cockpit → ligne « journal » → « Recharger depuis disque » (ou
  relancer l'app). Supprimer l'entrée du jour dans `journal_entries`
  (via `nexus_inspect`) puis rouvrir Journal un jour où le fichier
  n'existe pas encore.
- **Résultat attendu** : le nouveau fichier créé contient « Gabarit perso »
  suivi de la date du jour, PAS le gabarit par défaut (« = Journal — … »).
- **Raison diagnostique** : prouve que `JournalConfig` est réellement lue
  au runtime (`ctx.licorne.section("journal", …)`), pas juste acceptée et
  ignorée — exactement ce que Cockpit Nexus affirme sur ses lignes de
  config.

---

## 4. Todo

### 4.1 Filtre « maintenant »

- **Action** : créer 2 tâches, énergie `low` et `high`. Saisir un score
  cognitif = 2 dans État psy (health). Activer le filtre « maintenant ».
- **Résultat attendu** : seule la tâche `low` reste visible.
- **Raison diagnostique** : `allowed_energy_levels(2)` ne retourne QUE
  `["low"]` (doc §5.4 : « si cognitif = 2, affiche uniquement les tâches
  energie: low »).

### 4.2 Récurrence

- **Action** : créer une tâche avec échéance + récurrence « hebdo ». La
  faire passer en `done` (menu déroulant de la carte).
- **Résultat attendu** : une NOUVELLE tâche apparaît en `backlog`, même
  titre/énergie/contexte/durée, échéance = +7 jours. L'historique
  (`task_history`) garde la trace de la transition de l'ancienne tâche.
- **Raison diagnostique** : régénération automatique, jamais silencieuse
  (visible dans `nexus_inspect`, onglet Tâches, sous `task_history`).

---

## 5. Dashboard

### 5.1 Référentiel insuffisant, jamais de donnée inventée

- **Action** : sur un projet neuf (moins de 7 nuits de sommeil), ouvrir
  l'onglet Sommeil.
- **Résultat attendu** : le message « référentiel insuffisant (N=X,
  minimum requis : Y) » s'affiche, AUCUNE ligne de moyenne glissante n'est
  tracée.
- **Raison diagnostique** : si une ligne apparaît avec N < 7, c'est une
  violation du principe « no black box / aucune donnée inventée » du doc.

### 5.2 Export .ics

- **Action** : créer 2 tâches avec échéance, 1 sans échéance. Onglet
  Corrélations → « 📅 Exporter les tâches à échéance en .ics ».
- **Résultat attendu** : fichier `<projet>/.engram/exports/taches.ics`
  contenant exactement 2 `BEGIN:VEVENT` (pas 3), importable dans un agenda
  (Google Calendar, Nextcloud, iCloud — glisser-déposer ou import manuel).
- **Raison diagnostique** : la tâche sans échéance ne doit produire AUCUN
  VEVENT — vérifier en ouvrant le fichier `.ics` dans un éditeur texte
  avant même de l'importer.

---

## 6. Articles

### 6.1 Création et désambiguïsation de slug

- **Action** : créer un article titré « Test ». Retourner à la liste, créer
  un SECOND article titré exactement « Test » à nouveau.
- **Résultat attendu** : 2 fichiers distincts dans `04_articles/`
  (`test.typst` et `test_2.typst`), 2 lignes dans la liste.
- **Raison diagnostique** : si le second écrase le premier, la
  désambiguïsation de `create_new` a régressé.

---

## 7. Cockpit Nexus

### 7.1 Thème — rechargement à chaud

- **Action** : éditer `engram.ron`, changer une couleur de la section
  `theme`. Ouvrir Cockpit → ligne « theme » → « Recharger depuis disque ».
- **Résultat attendu** : la couleur change IMMÉDIATEMENT dans toutes les
  fenêtres ouvertes, sans relancer l'application.
- **Raison diagnostique** : seule la ligne « theme » a ce comportement
  (`ReloadMode::Live`) — les autres lignes (journal/dashboard/todo/
  theme_expert) affichent « Relance requise », vérifier que cette
  distinction est bien visible.

### 7.2 Redémarrage

- **Action** : cliquer « ⏻ Redémarrer l'application ».
- **Résultat attendu** : la fenêtre core se ferme, une nouvelle instance
  s'ouvre dans les ~1 seconde qui suit, le même projet doit être ré-ouvert
  manuellement (pas de session persistée — comportement attendu).
- **Raison diagnostique** : avant cet incrément, ce bouton ne faisait
  RIEN (`ModuleResponse::RestartApp` tombait dans un `_ => {}` silencieux
  côté `app_nexus`). Si le redémarrage échoue silencieusement, la
  régression est revenue.

---

## 8. nexus_inspect

### 8.1 Lecture seule garantie

- **Action** : ouvrir un projet dans `nexus_inspect`, tenter de modifier
  quoi que ce soit dans l'interface (il n'y a aucun champ éditable —
  c'est le point).
- **Résultat attendu** : aucune action d'écriture n'est même proposée à
  l'écran.
- **Vérification renforcée (optionnelle, ligne de commande)** : avec
  `nexus_inspect` ouvert sur le projet, tenter une écriture directe via
  `sqlite3 <projet>/.engram/nexus.db "PRAGMA query_only;"` dans un AUTRE
  terminal — doit retourner `1` tant que `nexus_inspect` détient la
  connexion en lecture seule.
- **Raison diagnostique** : la garantie est au niveau SQLite
  (`SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only`), pas seulement l'absence
  de bouton — c'est ce qui rend l'outil sûr même en cas de bug futur.

### 8.2 Intégrité — détection réelle

- **Action** : avec `Hive_RBMK_Tcherenkov` FERMÉ (pour éviter un conflit d'écriture),
  supprimer manuellement un fichier journal indexé (`rm
  <projet>/01_journal/AAAA/AAAA-MM-JJ.typst` pour une date déjà indexée).
  Ouvrir `nexus_inspect` → onglet Intégrité.
- **Résultat attendu** : une ligne « fichier manquant » citant exactement
  ce chemin.
- **Raison diagnostique** : prouve que le contrôle compare vraiment
  l'index DB au disque, pas une vérification cosmétique toujours verte.
