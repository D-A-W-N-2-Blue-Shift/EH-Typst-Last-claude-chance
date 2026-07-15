# dashboard — module Dashboard de Nexus

## Ce que je fais
Vue croisée sur les données de Nexus (doc de conception §5.6). Aucune
donnée inventée : sous le seuil minimal, j'affiche « référentiel
insuffisant (N=X, minimum requis : Y) » plutôt qu'une courbe extrapolée.

**Session 6 du doc §10 — Sommeil + État psy** : durée nuit par nuit sur
30/60/90 jours (barres) avec moyenne personnelle glissante 28 jours (ligne)
et qualité en overlay (points colorés) ; radar état psy semaine courante
vs précédente ; tendance 30 jours par dimension (5 lignes distinctes) ;
alerte visuelle si une dimension reste sous 2 pendant 3 jours consécutifs.

**Session 7 du doc §10 — Corrélations médication + tâches** : charge
tâches → épuisement (créées vs terminées vs score épuisement, barres
groupées + ligne) ; vélocité hebdomadaire (8 dernières semaines) ;
répartition par énergie des tâches terminées ; tâches bloquées depuis plus
de 7 jours (liste explicite) ; observance médication → fonctionnement
(barres groupées par médicament + ligne).

**Export** : données brutes de chaque vue en CSV, dans
`<projet>/.engram/exports/`.

## Comment je marche
- `OwnViewport`, fenêtre à la demande (bouton « 📊 Dashboard » du hub).
- J'apprends la racine du projet actif via `CoreEvent::ProjectRootUpdated`
  (même mécanisme que health/journal/todo) et j'ouvre ma propre connexion à
  `nexus.db`.
- Tous les graphiques sont peints à la main (`egui::Painter`), API
  vérifiée contre la source vendue avant chaque nouvelle utilisation.

## Comment me virer
1. Supprimer `modules/dashboard/`.
2. Retirer `registry.register("dashboard", …)` de `app_nexus/src/main.rs`.
3. Retirer la dépendance `dashboard` de `app_nexus/Cargo.toml`.
4. Retirer `"modules/dashboard"` des `members` du `Cargo.toml` du workspace.

## Mes dépendances (justifiées, §7.4)
- `engram_core` : le trait `Module` + `atomic_write` (export CSV).
- `nexus_db` : couche de données Nexus.
- `chrono` : dates, semaines ISO (`Datelike::iso_week`), fenêtres
  glissantes.
- `egui` : rendu, y compris les graphiques peints à la main.

## Décisions notables (§A2, §A10, §4.4)

**Mise à jour (relecture complète doc-vs-code, post-livraison) : les 4 vues
nommées par le doc §4.2 sont maintenant TOUTES représentées.** À la
session 7, seules `weekly_load` et `med_observance` avaient été construites
(lecture littérale du titre de la session : « corrélations médication +
tâches », sans `corr_sommeil_cognition` ni `corr_medication_etat`, différées
« sans date assignée »). Cette différence n'a jamais été refermée par une
session ultérieure du §10 — ni le sommeil→cognitif ni la courbe empirique
médication n'étaient en fait des angles morts nécessitant une décision
architecturale : `corr_sommeil_cognition` est un JOIN + un r² en forme
close (`correlations::r_squared`), et `corr_medication_etat` est un JOIN
identique à `med_observance` (déjà bâti) sur la fenêtre 12h du doc §4.2.
Les deux sont maintenant dans `correlations.rs`/`charts::draw_scatter`.

**Écart restant, cette fois réellement irréductible sans invention :** la
« courbe de tendance locale (LOESS si N>20) » de `corr_medication_etat`
(doc §5.6) N'EST PAS calculée — LOESS est un algorithme de lissage itératif
(régression locale pondérée par fenêtre glissante) sans aucun précédent
dans ce dépôt ; l'improviser sans vérification serait moins honnête que
d'afficher les points bruts seuls, que le doc lui-même prescrit comme repli
sous N=20. Les points bruts SONT affichés pour tout N — seule la courbe
lissée superposée manque.

**Vue `med_observance` du doc (§4.2) : champ manquant, pas inventé.** Le
doc décrit cette vue comme un « taux de prise effectif vs fréquence
attendue », mais `medications` (schéma §8, construit fidèlement à
l'incrément 1) n'a AUCUN champ de fréquence attendue. Je calcule la moitié
réellement dérivable — nombre de prises effectives par médicament par
semaine — appariée au fonctionnement hebdomadaire moyen. Le taux
d'observance (rapport à une fréquence attendue) reste impossible à
calculer tant que ce champ n'existe pas dans le schéma ; l'ajouter
supposerait une décision de modèle de données que je n'ai pas reçue.

**Pourquoi je n'importe pas health (§7.1, rappel de la session 6)** :
`health` et `dashboard` sont des modules pairs du même registre — logique
et rendu radar propres à ce module, pas de dépendance croisée entre
modules.

## Limites connues (§10)
- Pas de sélecteur de fichier natif pour l'export : chemin fixe et
  prévisible, même limite documentée que `nexus_hub`.
- Corrélation sommeil→cognitif : hors périmètre de cette session, voir
  ci-dessus.
