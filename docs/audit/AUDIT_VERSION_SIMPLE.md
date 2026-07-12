# Bilan de santé du logiciel — version sans jargon

**Date :** 12 juillet 2026
**Ce qu'on a regardé :** est-ce que le logiciel est **solide** (est-ce qu'il
risque de casser ou de perdre ton travail ?) et est-ce qu'il est **rapide et
économe** (est-ce qu'il rame ou vide la batterie ?).
**Ce qu'on n'a PAS regardé :** la sécurité (tu ne l'as pas demandé).
**Important :** on a seulement **lu** le programme. On n'a **rien modifié**.

---

## En une phrase

**Le logiciel est bien construit et sérieux.** Il y a **une chose importante à
corriger en priorité** (la façon dont il enregistre tes fichiers), deux ou trois
réglages à améliorer, et le reste n'est que du confort pour plus tard.

Imagine une maison : les fondations sont bonnes, la charpente est saine. Mais il
y a **une porte de secours** qu'il faut installer avant d'emménager pour de bon,
et **le chauffage tourne à fond en permanence** alors qu'on pourrait le baisser.

---

## Ce qui va bien (et c'est beaucoup)

- 🟢 **Le programme ne « plante » pas à la moindre erreur.** Les développeurs ont
  été très rigoureux : quand quelque chose se passe mal, le logiciel te prévient
  au lieu de se fermer brutalement. Ton texte reste en mémoire.
- 🟢 **Le « annuler » (Ctrl+Z) est bien fichu.** Une frappe et la petite
  correction automatique qui va avec s'annulent d'un seul coup, comme on
  s'y attend.
- 🟢 **Ouvrir un même fichier par deux chemins différents ne crée pas deux
  versions qui se contredisent.** Le logiciel comprend que c'est le même fichier.
- 🟢 **Il ne rame pas quand tu écris.** Même dans un très gros document, il ne
  s'occupe que de ce qui est affiché à l'écran, pas de tout le reste.
- 🟢 **Les erreurs ne sont jamais cachées.** S'il y a un souci, il est écrit
  quelque part et affiché — pas étouffé en silence.
- 🟢 **Les vérifications automatiques passent.** Les tests du programme
  fonctionnent, et les contrôles de qualité du code sont au vert.

En clair : **la qualité générale est là.** L'audit ne remet pas ça en cause.

---

## Ce qu'il faut corriger — par ordre d'importance

### 1. 🔴 Priorité n°1 : la façon d'enregistrer les fichiers

**Le problème, en clair :** quand le logiciel enregistre ton fichier, il
efface d'abord l'ancien contenu, puis écrit le nouveau **par-dessus, au même
endroit**. Pendant ce très court instant, si l'ordinateur s'éteint (coupure de
courant, batterie à plat, plantage), le fichier peut rester **coupé en deux ou
vide**. C'est-à-dire : ton chapitre.

**Faut-il paniquer ?** Non. Le risque ne se produit qu'au moment exact d'un
enregistrement **et** d'un arrêt brutal simultané — c'est rare. Et il y a des
garde-fous : sauvegarde automatique, sauvegarde à la fermeture, et des archives
complètes toutes les 30 minutes.

**Mais** pour un logiciel dont le seul métier est de **ne jamais perdre ton
texte**, ce risque ne devrait pas exister du tout. La bonne nouvelle : le
remède est **simple et connu**. Au lieu d'écrire par-dessus, le logiciel doit
écrire d'abord une copie neuve à côté, puis basculer d'un coup vers elle. Ainsi,
à tout moment, tu as **soit l'ancienne version complète, soit la nouvelle
complète** — jamais un fichier abîmé.

👉 **C'est LE point à faire traiter en premier.**

### 2. 🟠 Le logiciel « tourne à fond » en permanence

**En clair :** dès qu'une fenêtre d'édition est ouverte, le logiciel
rafraîchit l'écran 60 fois par seconde **même quand tu ne fais rien**. C'est
comme laisser le moteur tourner à l'arrêt. Résultat : **ordinateur portable qui
chauffe et batterie qui se vide plus vite** que nécessaire.

Ce n'est pas dangereux, mais c'est du gaspillage. On peut lui apprendre à se
mettre au repos quand rien ne bouge, comme le font les autres logiciels.

### 3. 🟠 Deux parties du logiciel écrivent dans le même carnet en même temps

**En clair :** l'outil qui suit tes fichiers et l'outil de **notes** écrivent
tous les deux dans le **même petit carnet interne** (une base de données cachée
dans le projet). Quand ils veulent écrire en même temps, l'un des deux peut être
recalé. Conséquences possibles : une mise à jour de l'index qui **passe à la
trappe** (le logiciel « oublie » temporairement un changement), ou une
**recherche qui échoue** juste au mauvais moment.

Ça n'abîme rien de définitif, mais ça peut donner l'impression que « ça déconne
un peu parfois ». Le réglage qui corrige ça est standard et rapide.

### 4. 🟠 Les notes modifient discrètement tes fichiers

**En clair :** quand tu attaches une note à un fichier, le logiciel glisse une
petite marque **à l'intérieur du fichier**. En faisant ça, il **réécrit tout le
fichier** — et au passage il peut **changer des détails invisibles** (le type de
retour à la ligne, la présence ou non d'une ligne vide à la fin). Tu ne le vois
pas, mais si tu utilises un outil de suivi de versions (comme git), ça crée du
« bruit » : le fichier a l'air modifié partout alors que tu n'as touché à rien.

À corriger pour que les notes **respectent exactement** le fichier d'origine.

---

## Ce qui peut attendre (confort et gros projets)

Ces points ne se verront **que si tes projets deviennent très gros** (des
milliers de fichiers). Aujourd'hui, tu ne les sens probablement pas :

- Au **démarrage**, le logiciel relit **tout** le projet à chaque fois, même ce
  qui n'a pas changé. On pourrait lui apprendre à ne relire que les nouveautés →
  démarrage plus rapide.
- Quand tu **enregistres un fichier**, il recopie en interne la liste **complète**
  de tous les fichiers, au lieu de ne noter que ce qui a bougé.
- Le panneau de **notes**, quand il se rafraîchit, relit tout le projet d'un coup
  → petit à-coup possible sur un très gros projet.

Rien de grave : ce sont des optimisations « pour plus tard », quand le volume
augmentera.

---

## Un point d'hygiène (pas urgent, mais utile)

Il manque un **contrôle automatique** qui vérifierait, à chaque modification du
programme, que tout est encore au vert. Les vérifications existent et
fonctionnent, mais il faut les lancer à la main. Mettre en place un contrôle
automatique protégerait toute la qualité déjà en place. **C'est le genre de
filet de sécurité qui évite les mauvaises surprises** au fil des futures
évolutions.

---

## Le tableau de bord

| Sujet | État | À faire ? |
|---|---|---|
| Le programme plante-t-il facilement ? | 🟢 Non, très solide | Rien |
| Peut-il abîmer un fichier en l'enregistrant ? | 🔴 Oui, cas rare mais réel | **Priorité 1** |
| Consomme-t-il trop de batterie ? | 🟠 Oui, inutilement | Priorité 2 |
| Les données internes peuvent-elles se marcher dessus ? | 🟠 Un peu | Priorité 3 |
| Les notes respectent-elles tes fichiers ? | 🟠 Pas tout à fait | Priorité 4 |
| Est-il rapide au quotidien ? | 🟢 Oui | Plus tard (gros projets) |
| Y a-t-il un filet de sécurité automatique ? | 🟡 Non | Utile, pas urgent |

---

## Ce qu'il faut retenir

1. **Tu peux l'utiliser.** La base est saine.
2. **Fais corriger en priorité la façon d'enregistrer** (point 1) : c'est
   quelques lignes pour un logiciel, et ça sécurise l'essentiel — ton texte.
3. En attendant, **garde l'habitude de sauvegarder régulièrement** et laisse les
   sauvegardes automatiques activées : elles couvrent déjà beaucoup.
4. Le reste (batterie, petits réglages) rend l'outil **plus agréable et plus
   propre**, mais ne t'empêche pas de travailler dès maintenant.

> Bref : bonne maison, bonnes fondations. On pose la porte de secours, on baisse
> le chauffage, et c'est du solide.
