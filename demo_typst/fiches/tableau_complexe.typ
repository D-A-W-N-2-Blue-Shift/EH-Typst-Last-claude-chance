#import "../templates/engram.typ": apply

#apply()

= Fiche de validation des tableaux

Cette fiche sert à tester les cellules multilignes, le texte long et les
colonnes de largeur asymétrique.

#table(
  columns: (1.7fr, 3.5fr, 3.8fr),
  [*Bloc*], [*Contenu*], [*But du test*],
  [Identité], [Svetlana Volkova, fiche étalon des personnages], [Vérifier les tables longues et les cellules chargées en texte],
  [Friction], [Le texte narratif doit rester lisible même quand la structure prend de la place], [Contrôler l'équilibre entre prose et tableau],
  [Tableau long], [Cellule avec plusieurs phrases. La deuxième phrase doit rester dans le même bloc. La troisième phrase sert à tester la hauteur naturelle de ligne et la lisibilité de la colonne adjacente.], [Tester l'usage quotidien sans couture],
  [Multiligne], [Première ligne
Deuxième ligne
Troisième ligne], [S'assurer que les sauts de ligne ne cassent pas le rendu],
)

== Détail opérationnel

#table(
  columns: 3,
  [Action], [Raccourci], [Effet attendu],
  [Relancer le rendu], [Ctrl+Alt+R], [Compile le fichier courant sans bloquer l'éditeur],
  [Ouvrir dans Kate], [Ctrl+Alt+K], [Ouvre le source dans l'éditeur système],
  [Ouvrir dans Okular], [Ctrl+Alt+O], [Ouvre le dernier PDF valide],
  [Aller à l'erreur], [Ctrl+Alt+E], [Saute vers la ligne signalée par Typst],
)

