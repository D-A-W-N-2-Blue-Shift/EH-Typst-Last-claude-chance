---
# ═══════════════════════════════════════════════
# MÉTADONNÉES SYSTÈME
# ═══════════════════════════════════════════════
uid: OBJE-####                         # Format OBJE-XXXX
schema_version: "1.0"
created: YYYY-MM-DD
updated: YYYY-MM-DD

# ═══════════════════════════════════════════════
# TAXONOMIE
# ═══════════════════════════════════════════════
tags:
  - objet
  # Choisir UNE catégorie principale :
  # objet_mundain | objet_symbolique | objet_cosmique | artefact | document | substance

type: "Objet"
# Valeurs autorisées : Arme | Accessoire | Artefact_Cosmique | Substance | Document_Physique | Outil | Relique | Symbole

statut_objet: ""
# Valeurs autorisées : Actif | Détruit | Perdu | Transmis | Latent | Activé

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

nom: ""
nom_commun: ""           # Comment les personnages l'appellent
categorie_physique: ""   # Ex. "Jeu de cartes", "Zippo", "Tatouage"

# ═══════════════════════════════════════════════
# PROPRIÉTÉ ET CIRCULATION
# ═══════════════════════════════════════════════
proprietaire_actuel: "[[NOM_PERSONNAGE]]"
historique_propriete:
  - proprietaire: "[[NOM]]"
    periode: "YYYY – YYYY"
    mode_acquisition: ""   # Héritage | Don | Vol | Fabrication | Activation
lieu_habituel: "[[NOM_LIEU]]"

# ═══════════════════════════════════════════════
# DESCRIPTION PHYSIQUE
# ═══════════════════════════════════════════════
description_physique: ""
materiaux: ""
dimensions: ""           # Si pertinent
etat: ""                 # Neuf | Usé | Endommagé | Altéré
detail_signature: ""     # Le détail qui le rend immédiatement reconnaissable

# ═══════════════════════════════════════════════
# FONCTION
# ═══════════════════════════════════════════════
fonction_pratique: ""    # Ce que l'objet fait concrètement
fonction_symbolique: ""  # Ce qu'il représente narrativement
fonction_cosmique: ""    # Propriétés surnaturelles/cosmiques — laisser vide si N/A
activation: ""           # Conditions d'activation si applicable

# ═══════════════════════════════════════════════
# CHARGE NARRATIVE
# ═══════════════════════════════════════════════
evenements_lies:
  - date: "YYYY-MM-DD"
    description: ""
    chapitre: "Ch_##/##"

theme:
  - ""
  # Exemples : "#contrôle" | "#identité" | "#trauma" | "#pouvoir" | "#masque"
  # "#héritage" | "#corruption" | "#révélation" | "#lien"

note_auteur: ""
donnees_a_confirmer:
  - ""
---

# [[NOM_OBJET]]

> *Une phrase sur ce que cet objet incarne narrativement.*

---

## I. Description Physique

**Apparence :**

**Détail signature :**

---

## II. Fonctions

**Pratique :**

**Symbolique :**

**Cosmique :** *(si applicable)*

---

## III. Historique de Circulation

| Période | Propriétaire | Mode d'acquisition | Événement lié |
| :--- | :--- | :--- | :--- |
| | | | |

---

## IV. Occurrences Manuscrit

| Chapitre | Contexte | Usage |
| :--- | :--- | :--- |
| Ch_##/## | | |

---

## V. Points en Suspens

| Question | Priorité |
| :--- | :--- |
| | |

---

*Fiche Conservateur — [date].*
