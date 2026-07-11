---
# ═══════════════════════════════════════════════
# MÉTADONNÉES SYSTÈME — NE PAS MODIFIER MANUELLEMENT
# ═══════════════════════════════════════════════
uid: PERS-####                         # Identifiant unique — format PERS-XXXX (ex: PERS-0001)
schema_version: "1.0"
created: YYYY-MM-DD
updated: YYYY-MM-DD

# ═══════════════════════════════════════════════
# TAXONOMIE — OBLIGATOIRE POUR LA BASE
# ═══════════════════════════════════════════════
tags:
  - personnage
  # Choisir UNE catégorie principale :
  # civil | wingate | starseed | antagoniste | entité | secondaire | figurant
  - CATEGORIE_PRINCIPALE

type: "Personnage"
# Valeurs autorisées : Protagoniste | Antagoniste Principal | Antagoniste Secondaire | Personnage Secondaire Majeur | Personnage Secondaire | Figurant

role_narratif: ""
# Valeurs autorisées : Protagoniste | Deutéragoniste | Antagoniste | Mentor | Allié | Catalyseur | Obstacle | Miroir | Victime | Enquêteur

statut: ""
# Valeurs autorisées : Actif | Décédé | Disparu | Statut_Inconnu | Transformé
# Peuvent être combinés avec / : ex. "Actif / Marqué", "Décédé / Post-T1"

tomes_actif:
  - T1       # Supprimer les tomes où le personnage n'apparaît pas
  - T2
  - T3

chapitre_apparition:
  - "Ch_##/##"   # Format strict : Ch_XX/XX

# ═══════════════════════════════════════════════
# IDENTITÉ CIVILE
# ═══════════════════════════════════════════════
alias:
  - ""           # Prénoms usuels, surnoms, pseudonymes

prenom: ""
nom: ""
date_naissance: "YYYY-##-##"   # ## pour les données non arbitrées
age: ##
origine:
  - ""           # Nationalité / Région
profession: ""
affiliation:
  - ""           # Organisation(s) — lien wikilink si fiche existante

# ═══════════════════════════════════════════════
# PHYSIQUE — DONNÉES ANCRÉES MANUSCRIT
# ═══════════════════════════════════════════════
apparence_generale:
  - ""
taille: ""       # ex. "1m73" — laisser vide si non arbitré
cheveux:
  - ""
yeux:
  - ""
style: ""
marques_distinctives:
  - ""
signature_olfactive: ""   # Laisser vide si non applicable

# ═══════════════════════════════════════════════
# PSYCHOLOGIE — DONNÉES ANCRÉES MANUSCRIT
# ═══════════════════════════════════════════════
conflit_interne: ""
motivation_principale: ""
faille_psychologique: ""
mecanisme_defense:
  - ""
arc_narratif: ""

# ═══════════════════════════════════════════════
# RELATIONS — FORMAT WIKILINK OBLIGATOIRE
# ═══════════════════════════════════════════════
relation_principale:
  - "[[NOM_PERSONNAGE]]"
couple: ""               # "[[NOM]]" ou null
meilleur_ami:
  - "[[NOM_PERSONNAGE]]"
mentor: ""               # "[[NOM]]" ou null
ex:
  - ""
antagoniste_de:
  - ""

# ═══════════════════════════════════════════════
# THÈMES — TAGS NARRATIFS
# ═══════════════════════════════════════════════
theme:
  - ""
  # Exemples : "#trauma" | "#manipulation" | "#possession" | "#alexithymie"
  # "#conflit_interne" | "#dualité" | "#rédemption" | "#miroir" | "#identité_fictive"
  # "#culpabilité" | "#loyauté" | "#trahison" | "#quête_de_pouvoir" | "#doute_réalité"

# ═══════════════════════════════════════════════
# FLAGS AUTEUR — NE JAMAIS ÉCRIRE DANS LE MANUSCRIT
# ═══════════════════════════════════════════════
note_auteur: ""          # Repères d'auteur uniquement — ironie dramatique, symétries secrètes
sort_non_arbitre: false  # true si le sort du personnage n'est pas encore décidé
donnees_a_confirmer:
  - ""                   # Points en suspens sur cette fiche
---

# [[NOM_PERSONNAGE]]

> *Tagline ou citation définissant le personnage en une phrase.*

---

## I. Profil Physique et Sensoriel

| Caractéristique | Description | Ancrage manuscrit |
| :--- | :--- | :--- |
| **Âge** | ## ans | — |
| **Traits généraux** | | |
| **Cheveux** | | |
| **Yeux** | | |
| **Style** | | |
| **Marques distinctives** | | |
| **Habitudes** | | |

---

## II. Profil Psychologique

### A. Structure de Personnalité

| Domaine | Description | Fonction psychologique |
| :--- | :--- | :--- |
| **Mécanisme principal** | | |
| **Rôle social** | | |
| **Comportement relationnel** | | |

### B. La Faille Fondamentale

| Origine | Manifestation | Conséquence narrative |
| :--- | :--- | :--- |
| | | |

### C. Mécanismes de Défense

- 

---

## III. Micro-Biographie Événementielle

| Année | Âge | Événement clé | Impact |
| :--- | :--- | :--- | :--- |
| **YYYY** | ## | | |

---

## IV. Relations Clés

### [[NOM_PERSONNAGE]]
*Nature du lien. Dynamique. Tension ou enjeu narratif.*

---

## V. Synthèse — Jeu de Dominos Psychologique

1. 
2. 
3. 

---

## VI. Arc Narratif — Phases

### Phase 1 — *[Titre]* (T#, jusqu'au XX/XX)
Texte.

### Phase 2 — *[Titre]*
Texte.

---

## VII. Points en Suspens

| Question | Priorité |
| :--- | :--- |
| | Avant T# / À confirmer |

---

*Fiche Conservateur — [date]. [Notes de révision si applicable.]*
