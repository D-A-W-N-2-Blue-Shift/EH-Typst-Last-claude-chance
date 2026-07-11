---
# ═══════════════════════════════════════════════
# MÉTADONNÉES SYSTÈME
# ═══════════════════════════════════════════════
uid: ORGA-####                         # Format ORGA-XXXX
schema_version: "1.0"
created: YYYY-MM-DD
updated: YYYY-MM-DD

# ═══════════════════════════════════════════════
# TAXONOMIE
# ═══════════════════════════════════════════════
tags:
  - organisation
  # Choisir UNE catégorie principale :
  # société_secrète | culte | entreprise | agence | groupe_informel | institution

type: "Organisation"
# Valeurs autorisées : Société_Secrète | Culte | Entreprise_Écran | Agence_Gouvernementale | Groupe_Informel | Institution_Académique | Réseau_Criminel

statut_orga: ""
# Valeurs autorisées : Active | Dissoute | Dormante | Infiltrée | Compromise | Détruite

tomes_actif:
  - T1
  - T2
  - T3

chapitre_apparition:
  - "Ch_##/##"

# ═══════════════════════════════════════════════
# IDENTITÉ
# ═══════════════════════════════════════════════
alias:
  - ""

nom_officiel: ""
nom_courant: ""
date_fondation: "YYYY"   # ## si non arbitré
fondateurs:
  - "[[NOM_PERSONNAGE]]"
siege: "[[NOM_LIEU]]"

# ═══════════════════════════════════════════════
# STRUCTURE
# ═══════════════════════════════════════════════
hierarchie:
  - niveau: "Direction"
    titre: ""
    occupant: "[[NOM_PERSONNAGE]]"
  - niveau: "Cadres"
    titre: ""
    occupant: ""
  - niveau: "Membres"
    titre: ""
    occupant: ""

effectif_estime: ""      # "~12 membres actifs" | "inconnu" | "plusieurs centaines"
recrutement: ""          # Comment l'organisation recrute ses membres

# ═══════════════════════════════════════════════
# AGENDA
# ═══════════════════════════════════════════════
mission_officielle: ""   # Ce qu'elle prétend faire
agenda_reel: ""          # Ce qu'elle fait vraiment
methodes:
  - ""
ressources:
  - ""                   # Financières, humaines, cosmiques, logistiques

# ═══════════════════════════════════════════════
# RELATIONS
# ═══════════════════════════════════════════════
membres_actifs:
  - "[[NOM_PERSONNAGE]]"
membres_inactifs:
  - "[[NOM_PERSONNAGE]]"
alliees:
  - "[[NOM_ORGANISATION]]"
ennemies:
  - "[[NOM_ORGANISATION]]"
entite_tutelaire: ""     # Entité cosmique associée si applicable

# ═══════════════════════════════════════════════
# CHARGE NARRATIVE
# ═══════════════════════════════════════════════
theme:
  - ""
  # Exemples : "#pouvoir" | "#secret" | "#manipulation" | "#corruption"
  # "#culte" | "#surveillance" | "#sacrifice" | "#contrôle"

note_auteur: ""
donnees_a_confirmer:
  - ""
---

# [[NOM_ORGANISATION]]

> *Ce que cette organisation représente narrativement — en une phrase.*

---

## I. Présentation

**Façade officielle :**

**Réalité :**

**Histoire :**

---

## II. Structure Hiérarchique

```
[Direction]
    └── [Niveau 2]
            └── [Membres]
```

**Modes de communication interne :**

**Protocoles et rituels :**

---

## III. Agenda et Méthodes

**Objectif à court terme :**

**Objectif à long terme :**

**Leviers d'action :**

---

## IV. Membres Notables

| Membre | Rang | Rôle réel | Statut |
| :--- | :--- | :--- | :--- |
| [[]] | | | Actif / Décédé |

---

## V. Historique des Opérations

| Date | Opération | Résultat | Chapitre |
| :--- | :--- | :--- | :--- |
| | | | |

---

## VI. Points en Suspens

| Question | Priorité |
| :--- | :--- |
| | |

---

*Fiche Conservateur — [date].*
