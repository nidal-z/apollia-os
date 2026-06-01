# ADR-003 - Duck typing pour l'Agent Interface Protocol (AIP)

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

L'AIP est le contrat entre le runtime Apollia OS et les agents Python. Le runtime doit être adoptable par les développeurs qui ont déjà des agents LangGraph, CrewAI, ou AutoGen existants - forcer l'héritage d'une classe `AIPAgent` créerait une friction à l'adoption et obligerait à modifier tous les agents existants.

Le principe #3 impose un "Contrat minimal" : deux méthodes suffisent.

## Décision

Nous utilisons le duck typing Python : tout objet Python exposant `manifest()` et `async run(task, ctx)` est AIP-compatible. La classe `AIPAgent` est optionnelle (aide-mémoire, non obligatoire). La validation se fait par `hasattr()` + inspection des signatures au chargement de l'agent, à l'état `INITIALIZING`.

## Alternatives considérées

### Option A - Classe de base obligatoire (rejetée)
**Pour :** Typage statique plus strict, meilleure autocomplétion IDE.
**Contre :** Oblige tous les agents existants à hériter d'une classe Apollia OS. Friction maximale à l'adoption. Crée une dépendance forte vers `apollia_os` dans chaque agent.

### Option B - `typing.Protocol` Python (rejetée)
**Pour :** Élégant, compatible mypy/pyright, zero friction runtime.
**Contre :** Nécessite que l'agent importe le `Protocol` depuis `apollia_os`. Pas de différence pratique avec la classe de base du point de vue de l'import obligatoire.

### Option C - Descripteur YAML/TOML séparé (rejetée)
**Pour :** Séparation configuration/code.
**Contre :** Source de vérité dupliquée : le manifest en YAML + le code de l'agent. Risque de désynchronisation. Plus lourd à maintenir.

### Option retenue - Duck typing + `hasattr` validation
**Pour :** Zero friction : un agent existant avec `manifest()` et `run()` fonctionne sans modification. Principe #3 respecté.
**Compromis acceptés :** Validation moins stricte au niveau du type checker statique. `AIPWrapper` nécessaire pour les cas edge (agents sans `manifest()`).

## Conséquences

**Positives :**
- Un agent LangGraph ou CrewAI existant peut être wrappé en une ligne avec `AIPWrapper`.
- Pas de dépendance obligatoire vers `apollia_os` dans le code agent.
- Fail fast au chargement : erreur claire si `manifest()` ou `run()` manquent (INITIALIZING).

**Négatives / Compromis :**
- Mypy/pyright ne peut pas vérifier statiquement la conformité AIP.
- `AIPWrapper` doit être maintenu pour couvrir les cas edge.
- Validation runtime uniquement - pas de feedback IDE avant l'exécution.

**Neutres / À surveiller :**
- Compléter `AIPWrapper` pour LangGraph et CrewAI (STORY-025).
- Documenter les cas d'erreur courants de validation duck typing.

## Principes architecturaux impactés

- Principe #3 - Contrat minimal : `manifest()` + `run()` async suffisent.
- Principe #4 - Fail fast : validation complète à INITIALIZING, pas à runtime.

## Liens

- Story associée : STORY-025 (Validation AIP duck typing)
- ADR précédent sur le même sujet : aucun
