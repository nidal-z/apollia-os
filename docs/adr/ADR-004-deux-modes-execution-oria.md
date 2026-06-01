# ADR-004 - Deux modes d'exécution ORIA (Direct + Orchestré)

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

La recherche RP-ReAct (2025) démontre que le mode Reasoner-Planner complet est optimal pour les tâches multi-step complexes mais introduit un overhead (appel LLM supplémentaire + génération de plan) sur les tâches atomiques. L'analyse du cas d'usage PME révèle que ~80% des tâches sont atomiques (une seule action, résultat direct). Facturer un appel LLM de planning pour "liste les fichiers" est inacceptable.

## Décision

Nous implémentons deux modes d'exécution dans ORIA, avec classification automatique à l'entrée de chaque tâche :
- **Mode Direct** : tâche atomique, appel direct à `agent.run()` avec supervision `StepBudget`. Critères : ≤ 4 outils requis, ≤ 15 steps estimés.
- **Mode Orchestré** : tâche complexe multi-step, `Reasoner` génère un `ExecutionPlan`, `ActorLoop` l'exécute step by step, avec jusqu'à 2 replans en cas d'échec.

La classification est automatique (l'agent ne choisit pas son mode).

## Alternatives considérées

### Option A - Mode unique Orchestré (rejetée)
**Pour :** Un seul chemin de code, plus simple à maintenir.
**Contre :** Appel LLM de planning pour chaque tâche, même les plus simples. Coût prohibitif et latence inutile pour 80% des cas PME.

### Option B - Mode unique Direct (rejetée)
**Pour :** Simplicité maximale, pas de LLM de planning.
**Contre :** Impossible pour les tâches genuinement multi-step (ex: "génère un devis complet avec 5 étapes"). L'agent devrait tout gérer lui-même sans guidage.

### Option C - Choix laissé à l'agent (rejetée)
**Pour :** L'agent connaît le mieux la complexité de ses tâches.
**Contre :** Trop de configuration pour la cible PME. Comportement non déterministe selon l'agent. Viole Principe #3 (contrat minimal).

### Option retenue - Classification automatique bimodale
**Pour :** Optimal pour chaque type de tâche. Transparent pour l'agent. Conforme au contrat minimal.
**Compromis acceptés :** Deux chemins de code à maintenir. Algorithme de classification doit être fiable.

## Conséquences

**Positives :**
- Latence minimale pour les tâches simples (pas d'appel LLM de planning).
- Support natif des tâches complexes multi-step en Mode Orchestré.
- Transparent pour l'agent : même interface `run()` dans les deux modes.

**Négatives / Compromis :**
- Deux chemins de code à tester et maintenir (Direct + Orchestré).
- Algorithme de classification peut se tromper sur des cas ambigus.
- Mode Orchestré implémenté plus tard (STORY-043, Sprint 6).

**Neutres / À surveiller :**
- Précision de la classification automatique sur les cas réels PME.
- Impact sur le StepBudget selon le mode (budget différent Direct vs Orchestré ?).

## Principes architecturaux impactés

- Principe #3 - Contrat minimal : L'agent ne gère pas le choix de mode.
- Principe #7 - Garde-fous non-négociables : StepBudget appliqué dans les deux modes.

## Liens

- Story associée : STORY-029 (Observer + classify), STORY-030 (ORIA Mode Direct), STORY-043 (Mode Orchestré)
- ADR précédent sur le même sujet : aucun
