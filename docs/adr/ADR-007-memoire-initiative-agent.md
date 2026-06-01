# ADR-007 - Mémoire à l'initiative de l'agent

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

Les systèmes de mémoire automatique (auto-memory) des frameworks IA modernes injectent du contexte mémoriel dans chaque appel LLM sans contrôle explicite. Ce pattern génère des coûts imprévisibles (appels LLM cachés), du bruit dans le contexte (mémoires non pertinentes), et une opacité dans le comportement de l'agent. La cible PME nécessite un comportement prédictible et des coûts maîtrisés.

## Décision

Nous n'injectons jamais automatiquement de contexte mémoriel dans les appels agents. L'agent appelle explicitement les méthodes de mémoire via `RuntimeContext.memory` : `search()`, `recall()`, `record()`, etc. Le runtime ne touche pas à la mémoire sans instruction explicite de l'agent.

## Alternatives considérées

### Option A - Injection automatique des épisodes récents (rejetée)
**Pour :** Comportement plus "intelligent" sans effort de l'agent.
**Contre :** Coût imprévisible (combien d'épisodes ? quelle troncature ?). Bruit dans le contexte LLM. L'agent perd le contrôle de ce qu'il voit. Difficile à déboguer.

### Option B - Injection après retrieval intelligent (rejetée)
**Pour :** Sélection pertinente des épisodes via scoring.
**Contre :** Appel LLM supplémentaire caché pour le scoring. Double coût. Opacité accrue. L'agent ne sait pas pourquoi certaines mémoires sont injectées.

### Option retenue - Mémoire explicite à l'initiative de l'agent
**Pour :** Comportement 100% prédictible. Coûts maîtrisés. L'agent décide quand et quoi récupérer.
**Compromis acceptés :** Plus de code côté agent pour gérer la mémoire explicitement. Moins d'"automatisme".

## Conséquences

**Positives :**
- Comportement de l'agent entièrement déterministe et debuggable.
- Zéro appel LLM caché généré par le runtime.
- L'agent contrôle la pertinence de ce qu'il récupère.
- Principle #6 - Mémoire à initiative agent : respecté strictement.

**Négatives / Compromis :**
- Les agents doivent explicitement appeler `ctx.memory.search()` pour bénéficier de la mémoire.
- Plus verbose pour les agents simples qui n'ont pas besoin de mémoire avancée.

**Neutres / À surveiller :**
- Patterns d'usage mémoire courants dans les agents PME (identifier les helpers à fournir).
- Documentation des bonnes pratiques mémoire pour les développeurs d'agents.

## Principes architecturaux impactés

- Principe #6 - Mémoire à initiative de l'agent : décision directement issue de ce principe.
- Principe #1 - Local-first : La mémoire reste locale et sous contrôle de l'agent.

## Liens

- Story associée : STORY-028 (MemoryInterface Python → apollia-memory)
- ADR précédent sur le même sujet : aucun
