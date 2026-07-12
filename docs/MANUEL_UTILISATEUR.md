# Manuel utilisateur - EH-Typst-Labs

## 1. Ce que c'est

`EH-Typst-Labs` est une version de travail d'Engram orientée Typst.
Elle sert à:

- ouvrir un projet;
- éditer des fichiers `.typ`;
- sauvegarder;
- lancer un rendu PDF local;
- consulter des notes liées;
- lancer une analyse COH2B en lecture seule;
- naviguer dans le projet.

## 2. Lancer l'application

Utilise le lancement habituel de l'application.
Le fork utilise ses propres dossiers de configuration et de données:

- `~/.config/engram_hive_typst/`
- `~/.local/share/engram_hive_typst/`

## 3. Ouvrir un projet

1. Ouvre le file tree.
2. Choisis un dossier de projet.
3. Si le dossier est vide ou incomplet, l'application demande une confirmation avant de créer une structure.

## 4. Éditer un fichier Typst

1. Ouvre un fichier `.typ` depuis le tree.
2. Modifie le texte.
3. Sauvegarde.
4. Le rendu PDF est relancé automatiquement après sauvegarde ou après un court délai.

## 5. Rendu et PDF

Le rendu est produit localement avec le binaire `typst`.

Actions visibles:

- `Relancer le rendu`
- `Ouvrir dans Okular`
- `Ouvrir le dossier de rendu`
- `Aller à l'erreur`

Si le fichier contient une erreur de syntaxe, le dernier PDF valide reste affiché.

## 6. Notes liées

Le panneau `sticky_notes` sert à créer et gérer des notes attachées à des fichiers.

Ce que tu peux faire:

- créer une note depuis le fichier courant;
- ajouter des tags;
- lier des fichiers;
- ouvrir la liste des notes;
- rouvrir une note existante;
- supprimer une note.

Les notes sont stockées dans la base SQLite du projet.
Le module est chargé par défaut dans `modules.enabled` de `engram.ron`; si tu
le retires de cette liste, l'application le considérera comme désactivé.
Quand le fichier source est déjà ouvert dans l'éditeur, le marqueur est appliqué
directement au buffer partagé avec une seule transaction undo.
Quand le fichier source n'est pas ouvert, le marqueur est écrit atomiquement.
Dans les deux cas, les fins de ligne du fichier source sont préservées.

## 7. COH2B

Le panneau COH2B sert à poser une question à un agent en lecture seule.

Ce que tu peux choisir:

- périmètre: fichier courant, texte sélectionné, corpus, projet complet;
- mode: question libre, cohérence micro, cohérence macro, piste narrative, audit;
- fournisseur et mode d'authentification.

COH2B affiche:

- la réponse;
- l'usage visible si disponible;
- un historique local;
- un journal horodaté.

## 8. Recherche dans le corpus

La recherche corpus s'appuie sur l'index SQLite du projet.

Quand tu poses une question, COH2B peut joindre des extraits pertinents au prompt:

- fichier;
- section;
- extrait;
- nombre de mots.

## 9. Filtre du file tree

Le tree peut afficher les fichiers utiles du projet, pas seulement les `.typ`.

Actions principales:

- ouvrir un `.typ` dans l'éditeur;
- ouvrir un `.md`, `.txt`, `.ron`, `.toml`, `.json`, `.csv` ou `.tsv` en texte;
- ouvrir un `.pdf` dans Okular;
- ouvrir les images dans le système.

## 10. Cockpit

Le cockpit est désactivé par défaut.

Tu peux l'ouvrir depuis la palette si tu veux:

- consulter les réglages;
- vérifier la config;
- recharger certaines valeurs.
Le heartbeat de repaint n’est maintenu que lorsque la fenêtre principale est
minimisée et qu’au moins un viewport enfant est actif.

## 11. Raccourcis utiles

Les raccourcis exacts peuvent être personnalisés, mais les actions visibles à retenir sont:

- palette globale;
- sauvegarde;
- rendu;
- ouverture Kate;
- ouverture Okular;
- notes liées;
- COH2B.

## 12. Si quelque chose ne marche pas

Vérifie d'abord:

- que le projet est bien ouvert;
- que le fichier courant est bien un `.typ`;
- que le binaire `typst` est installé;
- que le dossier de projet contient un index `.engram/` si tu veux la recherche corpus;
- que le panneau voulu est bien ouvert dans la palette.

## 13. Ce qu'il ne faut pas attendre

Ne pas attendre ici:

- un WYSIWYG complet;
- une édition collaborative;
- un cloud;
- un moteur de cohérence destructif;
- une modification automatique du projet par COH2B.

## 14. Règle simple d'usage

Pour travailler:

1. ouvre le projet;
2. ouvre le `.typ`;
3. édite;
4. sauvegarde;
5. lis le PDF;
6. ajoute une note si besoin;
7. pose une question COH2B si tu veux une analyse.
