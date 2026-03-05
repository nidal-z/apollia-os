---
name: apollia-sprint
description: Planifier, démarrer, clôturer et faire le bilan d'un sprint pour le projet Apollia OS. Utilise ce skill quand l'utilisateur dit "on commence le sprint N", "qu'est-ce qu'on fait ce sprint", "bilan du sprint", "démarre le sprint", "prépare le sprint", "qu'est-ce qui reste à faire", ou demande à voir l'état d'avancement du projet. Ce skill maintient la cohérence entre les stories, vérifie les dépendances, et produit les artefacts de sprint (plan, bilan, décisions).
---

# Apollia OS — Skill de Planification de Sprint

Gère le cycle de vie complet d'un sprint Apollia OS : préparation, démarrage, suivi, et clôture.

## Contexte

- **Cadence** : ~2 semaines par sprint, 8-10h de développement effectif
- **Solo developer** : pas de cérémonies d'équipe, focus sur la valeur livrée
- **Référence** : `references/sprint-index.md` = source de vérité pour l'état des stories

---

## Commandes disponibles

### 1. `prépare sprint N`

Produit le plan détaillé du sprint avant de commencer.

**Actions :**
1. Lire `references/sprint-index.md` pour récupérer les stories du sprint
2. Vérifier que les dépendances des stories sont toutes ✅ dans les sprints précédents
3. Calculer la charge totale (S=2h, M=3h, L=6h) et comparer au budget (8-10h/semaine × 2 = 16-20h)
4. Identifier le **Sprint Goal** — le livrable démo-able unique qui valide le sprint
5. Produire `STORIES/sprint-N/plan.md`

**Format du plan :**
```markdown
# Sprint N — Plan

**Sprint Goal :** [livrable démo-able en 1 phrase]
**Durée estimée :** Xh / budget 16-20h
**Dates :** semaine W → semaine W+1

## Stories du sprint (ordre d'implémentation)

| Priorité | ID | Story | Taille | Dépend de |
|---|---|---|---|---|
| 1 | STORY-NNN | ... | M | STORY-NNN ✅ |

## Dépendances vérifiées
[liste des dépendances inter-stories]

## Risques identifiés
[points techniques incertains]

## Definition of Done du sprint
- [ ] Sprint Goal atteint et démo-able
- [ ] `cargo test --workspace` passe
- [ ] `cargo clippy --workspace -- -D warnings` propre
- [ ] `sprint-index.md` mis à jour
- [ ] Bilan sprint rédigé
```

### 2. `démarre sprint N`

Initialise les artefacts du sprint et la première story.

**Actions :**
1. Créer `STORIES/sprint-N/` si inexistant
2. Copier le template de story pour la première story du plan
3. Mettre à jour `sprint-index.md` : sprint → 🔄 En cours, première story → 🔄
4. Afficher le focus immédiat : "Commence par STORY-NNN : [titre]"

### 3. `statut sprint`

Vue d'ensemble de l'avancement en cours.

**Format de sortie :**
```
Sprint N — En cours (jour X/14)

STORIES
  ✅ STORY-006  EventBus broadcast Tokio              M  2.5h
  ✅ STORY-007  AgentRegistry acteur Tokio             M  3h
  🔄 STORY-008  AgentRegistryHandle API publique       S  [en cours]
  🔲 STORY-009  Test intégration EventBus ↔ Registry  M

Charge : 5.5h / ~18h budget  ████░░░░░░ 30%
Sprint Goal : Test intégration qui passe ← STORY-009 manquante

PROCHAINE ACTION : Terminer STORY-008, puis STORY-009
```

### 4. `clôture sprint N`

Valide le sprint terminé et prépare le bilan.

**Checklist de clôture :**
- [ ] Toutes les stories prévues sont ✅ (sinon documenter pourquoi)
- [ ] Sprint Goal atteint (démo réalisable)
- [ ] `cargo test --workspace` passe
- [ ] `sprint-index.md` mis à jour
- [ ] `STORIES/sprint-N/bilan.md` rédigé

**Format du bilan :**
```markdown
# Sprint N — Bilan

**Sprint Goal :** [atteint ✅ | partiellement ⚠️ | non atteint ❌]
**Démo :** [description de ce qui peut être démontré]

## Stories livrées

| ID | Story | Taille estimée | Temps réel | Dérive |
|---|---|---|---|---|

## Ce qui a bien marché

## Ce qui a posé problème

## Stories reportées (si applicable)

| ID | Story | Raison | Sprint cible |
|---|---|---|---|

## Décisions architecturales prises
[pointer vers les ADR créés si applicable]

## Dette technique identifiée

## Focus sprint suivant
```

### 5. `prochaine story`

Identifie la prochaine story à implémenter selon l'ordre de priorité et les dépendances.

**Logique :**
1. Lire l'état du sprint courant dans `sprint-index.md`
2. Trouver la première story 🔲 dont toutes les dépendances sont ✅
3. Afficher : "Prochaine story : STORY-NNN — [titre]. Commencer par [fichier cible]."

---

## Règles de gestion de sprint

### Scope creep
Si une story estimée M s'avère XL en cours d'implémentation :
1. Stopper l'implémentation
2. Découper en 2-3 stories M
3. Mettre à jour `sprint-index.md`
4. Reprendre avec la première sous-story

### Stories non terminées en fin de sprint
Ne jamais "forcer" une story à ✅ si elle ne passe pas la DoD.
→ Reporter au sprint suivant avec une note sur ce qui bloque.
→ Ajuster l'estimation pour le sprint suivant.

### Décisions pendant l'implémentation
Si une décision architecturale significative est prise pendant un sprint :
→ Créer immédiatement un ADR via le skill `apollia-adr`
→ Pointer depuis la story concernée

---

## Fichiers produits par ce skill

```
STORIES/
├── sprint-N/
│   ├── plan.md              ← produit par "prépare sprint N"
│   ├── bilan.md             ← produit par "clôture sprint N"
│   ├── story-NNN-titre.md   ← produit par apollia-story
│   └── index.md             ← liste des stories du sprint
└── sprint-index.md          ← mis à jour à chaque action
```
