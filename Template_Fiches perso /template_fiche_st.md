# Conventions & Schéma — Système de Fiches

> Documentation technique. Usage interne auteur.

---

## I. Arborescence

```
_fiches/
├── personnages/          → PERS-XXXX
├── lieux/                → LIEU-XXXX
├── objets/               → OBJE-XXXX
├── concepts/             → CONC-XXXX
├── entités/              → ENTI-XXXX
├── organisations/        → ORGA-XXXX
├── documents_internes/   → DOCU-XXXX
└── fragments_lore/       → LORE-XXXX
```

---

## II. Convention UID

Format : `PREFIXE-XXXX` (4 chiffres, zéro-paddé).

| Préfixe | Catégorie |
| :--- | :--- |
| `PERS` | Personnages |
| `LIEU` | Lieux |
| `OBJE` | Objets |
| `CONC` | Concepts |
| `ENTI` | Entités |
| `ORGA` | Organisations |
| `DOCU` | Documents internes |
| `LORE` | Fragments de lore |

Exemples valides : `PERS-0001`, `ENTI-0003`, `LORE-0012`.

**Règle d'or :** L'UID est attribué à la création et ne change jamais, même si la fiche est renommée ou déplacée.

---

## III. Valeurs Contrôlées — Champs Critiques pour la BDD

Ces champs doivent utiliser exclusivement les valeurs listées. Toute valeur hors-liste casse les requêtes SQLite.

### `type` (tous templates)

| Template | Valeurs autorisées |
| :--- | :--- |
| Personnage | `Protagoniste` · `Antagoniste Principal` · `Antagoniste Secondaire` · `Personnage Secondaire Majeur` · `Personnage Secondaire` · `Figurant` |
| Lieu | `Bar` · `Appartement` · `Villa` · `Bâtiment` · `Ville` · `Rue` · `Pays` · `Site_Cosmique` · `Plan_Onirique` · `Organisation_Physique` |
| Objet | `Arme` · `Accessoire` · `Artefact_Cosmique` · `Substance` · `Document_Physique` · `Outil` · `Relique` · `Symbole` |
| Concept | `Mécanisme_Psychologique` · `Loi_Cosmique` · `Thème_Narratif` · `Pratique_Ésotérique` · `Système_Social` · `Pathologie` · `Symbole_Récurrent` |
| Entité | `Grand_Ancien` · `Divinité_Mineure` · `Entité_Onirique` · `Possesseur` · `Fragment_Cosmique` · `Héraut` |
| Organisation | `Société_Secrète` · `Culte` · `Entreprise_Écran` · `Agence_Gouvernementale` · `Groupe_Informel` · `Institution_Académique` · `Réseau_Criminel` |
| Document interne | `Rapport_Officiel` · `Lettre_Personnelle` · `Journal_Intime` · `Archive_Classifiée` · `Note_Opérationnelle` · `Manifeste` · `Contrat` · `Dossier_Médical` · `Coupure_Presse` |
| Fragment lore | `Règle_Cosmique` · `Événement_Fondateur` · `Mythe_Interne` · `Chronologie` · `Géopolitique_Fictive` · `Backstory_Monde` · `Taxonomie_Univers` |

### `statut` (personnages)

```
Actif | Décédé | Disparu | Statut_Inconnu | Transformé
```

Combinaisons autorisées avec ` / ` : `Actif / Marqué` · `Décédé / Post-T1` · `Actif / Sous_Influence` · `Actif / Surveillé`

### `role_narratif` (personnages)

```
Protagoniste | Deutéragoniste | Antagoniste | Mentor | Allié | Catalyseur | Obstacle | Miroir | Victime | Enquêteur
```

### `tomes_actif`

Toujours une liste YAML, même pour un seul tome :
```yaml
tomes_actif:
  - T1
```
Pas de `tomes_actif: T1` (string) — ça casse le GROUP BY.

### `chapitre_apparition`

Format strict : `Ch_XX/XX` (numéro chapitre sur 2 chiffres, numéro scène sur 1 ou 2 chiffres).
Exemples valides : `Ch_05/4` · `Ch_13/1` · `Ch_20/12`

### Champs booléens

Valeurs autorisées uniquement : `true` · `false` (minuscules, sans guillemets).

### Champs à valeur `null`

Utiliser `null` (sans guillemets) pour les champs non applicables, pas une string vide.
```yaml
couple: null           # ✓
couple: ""             # ✗ — interprété comme string
couple:                # ✗ — ambigu
```

---

## IV. Convention Wikilinks

Toutes les références à d'autres fiches utilisent la syntaxe `[[UID]]` ou `[[Nom Affiché]]`.

Préférer `[[Nom Affiché]]` pour la lisibilité, mais s'assurer que le nom correspond exactement au titre de la fiche cible (Obsidian est case-sensitive sur les wikilinks).

Pour les champs YAML, les wikilinks s'écrivent entre guillemets :
```yaml
mentor: "[[Gwenaëlle Morvan]]"
```

---

## V. Données Non Arbitrées

Deux conventions selon le type de champ :

| Situation | Convention |
| :--- | :--- |
| Date partiellement connue | `"YYYY-##-##"` (## pour les parties inconnues) |
| Valeur textuelle non décidée | Laisser le champ commenté ou mettre `null` |
| Donnée à confirmer | Lister dans `donnees_a_confirmer` |

---

## VI. Schéma SQLite — Tables Principales

Le parser Python lit le YAML frontmatter et peuple les tables suivantes.

### Table `fiches`

```sql
CREATE TABLE fiches (
    uid             TEXT PRIMARY KEY,
    schema_version  TEXT NOT NULL,
    created         DATE,
    updated         DATE,
    type            TEXT NOT NULL,
    statut          TEXT,
    nom             TEXT,
    sort_non_arbitre INTEGER DEFAULT 0
);
```

### Table `tags`

```sql
CREATE TABLE tags (
    fiche_uid   TEXT REFERENCES fiches(uid),
    tag         TEXT NOT NULL,
    PRIMARY KEY (fiche_uid, tag)
);
```

### Table `apparitions`

```sql
CREATE TABLE apparitions (
    fiche_uid   TEXT REFERENCES fiches(uid),
    tome        TEXT,         -- T1 / T2 / T3
    chapitre    TEXT,         -- Ch_XX/XX
    PRIMARY KEY (fiche_uid, chapitre)
);
```

### Table `relations` (personnages)

```sql
CREATE TABLE relations (
    source_uid      TEXT REFERENCES fiches(uid),
    cible_uid       TEXT REFERENCES fiches(uid),
    type_relation   TEXT,   -- couple | mentor | meilleur_ami | antagoniste_de | etc.
    PRIMARY KEY (source_uid, cible_uid, type_relation)
);
```

### Table `personnages` (extension)

```sql
CREATE TABLE personnages (
    uid             TEXT PRIMARY KEY REFERENCES fiches(uid),
    prenom          TEXT,
    nom             TEXT,
    age             INTEGER,
    date_naissance  TEXT,
    origine         TEXT,       -- JSON array
    profession      TEXT,
    affiliation     TEXT,       -- JSON array de UIDs
    role_narratif   TEXT,
    arc_narratif    TEXT,
    note_auteur     TEXT        -- champ confidentiel
);
```

### Requêtes utiles

```sql
-- Tous les personnages actifs en T2
SELECT p.prenom, p.nom, f.statut
FROM personnages p
JOIN fiches f ON p.uid = f.uid
JOIN apparitions a ON f.uid = a.fiche_uid
WHERE a.tome = 'T2' AND f.statut LIKE 'Actif%';

-- Toutes les apparitions d'un chapitre donné
SELECT f.nom, f.type
FROM fiches f
JOIN apparitions a ON f.uid = a.fiche_uid
WHERE a.chapitre = 'Ch_18/4'
ORDER BY f.type;

-- Personnages avec sort non arbitré
SELECT nom FROM fiches
WHERE sort_non_arbitre = 1 AND type = 'Personnage';

-- Relations d'un personnage
SELECT f2.nom, r.type_relation
FROM relations r
JOIN fiches f2 ON r.cible_uid = f2.uid
WHERE r.source_uid = 'PERS-0001';
```

---

## VII. Notes de Maintenance

- Ne jamais modifier un `uid` après création.
- `updated` doit être mis à jour à chaque modification substantielle de la fiche.
- `note_auteur` n'est jamais exporté dans les listes de production. Le parser SQLite peut l'exclure via une vue.
- `donnees_a_confirmer` alimente un rapport de suivi généré trimestriellement.
- Les champs `##` dans les dates indiquent des données non arbitrées — ne pas remplacer par des valeurs inventées.

---

*Document Conservateur — v1.0*
