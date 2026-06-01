---
name: apollia-adr
description: Créer et maintenir les Architecture Decision Records (ADR) pour le projet Apollia OS. Utilise ce skill quand l'utilisateur dit "crée un ADR", "documente cette décision", "on a décidé de...", "choix architectural", "pourquoi on utilise X plutôt que Y", ou quand une implémentation dévie de la spec initiale et nécessite une justification tracée. Ce skill garantit que toutes les décisions techniques significatives sont documentées avec leur contexte, alternatives, et conséquences - indispensable pour un projet open-source.
---

# Apollia OS - Skill de Création d'ADR

Produit des Architecture Decision Records cohérents, tracés dans `docs/Decisions-Log.md` et dans le répertoire `docs/adr/`.

## Quand créer un ADR

**Créer un ADR si la décision répond OUI à au moins une de ces questions :**
- Change un des 8 principes architecturaux ?
- Introduit une dépendance externe non prévue ?
- Modifie l'interface publique d'une brique (AIP, Tool Registry, Memory Engine) ?
- Contredit une décision documentée dans `docs/Decisions-Log.md` ?
- Serait difficile à inverser plus tard ?
- Surprendrait un contributeur externe qui lit le code ?

**Ne pas créer d'ADR pour :**
- Choix d'implémentation internes (nommage de variables, structure d'un module privé)
- Corrections de bugs sans impact architectural
- Ajout de tests

---

## Workflow de création

### Étape 1 - Identifier le numéro

Lire `docs/Decisions-Log.md`, trouver le dernier ADR-NNN, incrémenter.

### Étape 2 - Remplir le template

Utiliser le template ci-dessous. Remplir **toutes** les sections - une section vide = ADR incomplet.

### Étape 3 - Double enregistrement

Produire **deux fichiers** :
1. `docs/adr/ADR-NNN-titre-court.md` - ADR complet standalone
2. Mise à jour de `docs/Decisions-Log.md` - ajout de l'entrée résumée

### Étape 4 - Pointer depuis la story

Si l'ADR est créé pendant l'implémentation d'une story, ajouter dans la section "Liens" de la story :
```
- ADR associé : ADR-NNN
```

---

## Template ADR

```markdown
# ADR-NNN - Titre court et concret

**Date :** YYYY-MM-DD
**Statut :** Proposé | Accepté | Remplacé par ADR-NNN | Déprécié
**Décideur :** Nidal (solo)
**Sprint :** N (ou "Pré-implémentation")

---

## Contexte

[Décrire la situation qui nécessite une décision. Inclure :
- Le problème concret rencontré
- Les contraintes qui s'appliquent (principes architecturaux, stack, etc.)
- Pourquoi maintenant et pas plus tard]

## Décision

[La décision prise, formulée clairement. Commencer par "Nous utilisons X" ou "Nous adoptons Y".]

## Alternatives considérées

### Option A - [Nom] (rejetée)
**Pour :** [avantages]
**Contre :** [pourquoi rejetée]

### Option B - [Nom] (rejetée)
**Pour :** [avantages]
**Contre :** [pourquoi rejetée]

### Option retenue - [Nom]
**Pour :** [avantages qui l'ont emporté]
**Compromis acceptés :** [ce qu'on sacrifie]

## Conséquences

**Positives :**
- [conséquence positive 1]

**Négatives / Compromis :**
- [compromis accepté 1]

**Neutres / À surveiller :**
- [point à monitorer dans les sprints suivants]

## Principes architecturaux impactés

- Principe #N - [nom] : [comment cette décision s'aligne ou s'écarte]

## Liens

- Story associée : STORY-NNN
- ADR précédent sur le même sujet : ADR-NNN (si applicable)
- Documentation externe : [lien si pertinent]
```

---

## Format entrée dans Decisions-Log.md

Chaque ADR ajouté dans `docs/Decisions-Log.md` suit ce format condensé :

```markdown
## ADR-NNN - Titre court

**Date :** YYYY-MM-DD
**Statut :** Accepté

**Contexte :** [1-2 phrases]

**Décision :** [1 phrase claire]

**Alternatives considérées :** [liste inline : Option A (rejetée car X), Option B (rejetée car Y)]

**Conséquences :** [2-3 points clés]

**Principes impactés :** Principe #N - [nom]

[Détail complet → docs/adr/ADR-NNN-titre.md]
```

---

## ADRs existants (référence)

Les ADR-001 à ADR-010 sont documentés dans `docs/Decisions-Log.md`. Avant de créer un nouvel ADR, vérifier qu'il ne contredit pas un ADR existant. Si c'est le cas, le nouvel ADR doit explicitement mentionner qu'il **remplace** l'ADR précédent (statut → "Remplacé par ADR-NNN").

---

## Règles de style

- **Concret, pas abstrait** : "Nous utilisons SQLite avec FTS5" pas "Nous privilégions les solutions locales"
- **Les alternatives doivent être réelles** : Ne lister que des options qui ont été vraiment considérées
- **Les conséquences négatives sont obligatoires** : Un ADR sans compromis acceptés est incomplet
- **Temps présent** : "Nous utilisons X" pas "Nous avons décidé d'utiliser X"
