# ADR-037 - Packaging Python SDK

**Date :** 2026-03-23
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 21

---

## Contexte

Les développeurs d'agents écrivent actuellement leurs agents Python en important depuis un fichier unique `apollia_base.py` déposé dans le répertoire `agents/`. Il n'y a pas de type hints, pas d'utilitaires de test, pas d'autocomplete IDE, pas de packaging propre.

Nous avons besoin d'un SDK Python que les développeurs d'agents peuvent installer via pip avec support IDE complet, tout en maintenant zéro couplage avec le runtime Rust au moment de l'installation. Le SDK doit rester un package Python pur - les classes PyO3 sont exposées au runtime, pas au développement.

## Décision

Nous créons `apollia-sdk` comme package Python séparé dans `sdk/` à la racine du workspace. Installable via `pip install -e ./sdk`. Zéro dépendance Rust à l'installation - les type stubs sont du pur Python (PEP 561 `py.typed`). Le SDK contient les base classes (`BaseReActAgent`, `ConversationalAgent`, `OrchestratedAgent`), les utilitaires (parsing, formatting), les mocks de test (`MockContext`), et le scaffolding CLI (`apollia new`). L'ancien `agents/apollia_base.py` devient un wrapper de compatibilité qui importe depuis le SDK.

## Alternatives considérées

### Option A - Bundle SDK in the Rust binary via PyO3 (rejetée)
**Pour :** Distribution unique, un seul artefact à gérer.
**Contre :** Requiert un build Rust pour le développement Python, complexifie le packaging, va à l'encontre de l'objectif de faciliter l'écriture d'agents.

### Option B - Keep apollia_base.py as single file (rejetée)
**Pour :** Zéro setup, un seul fichier à copier.
**Contre :** Pas de type hints, pas de support IDE, pas d'utilitaires de test, ne scale pas avec l'ajout de base classes et d'utilitaires.

### Option retenue - Separate pip-installable package
**Pour :** Tooling Python standard, autocomplete IDE via type stubs PEP 561, testable avec pytest, zéro dépendance Rust à l'installation.
**Compromis acceptés :** Deux étapes d'installation (runtime + SDK), les stubs peuvent diverger de la réalité PyO3 (synchronisation manuelle nécessaire).

## Conséquences

**Positives :**
- Les développeurs d'agents bénéficient de l'autocomplete, de la validation mypy, de `MockContext` pour les tests, et du scaffolding CLI
- Packaging Python standard (`pyproject.toml`, `pip install -e`)
- Testabilité des agents sans runtime Rust (mocks purs Python)

**Négatives / Compromis :**
- Les stubs doivent être maintenus en synchronisation avec les classes PyO3 manuellement
- Deux packages à maintenir (runtime Rust + SDK Python)

**Neutres / À surveiller :**
- Considérer la génération automatique des stubs depuis PyO3 à terme
- Évaluer si le SDK doit être publié sur PyPI ou rester en installation locale uniquement
- Surveiller que `MockContext` reste fidèle au comportement réel de `RuntimeContext`

## Principes architecturaux impactés
- Principe #3 - Contrat minimal : le SDK expose uniquement ce dont les agents ont besoin, pas les internals du runtime
- Principe #2 - Zéro dépendance externe : le SDK a zéro dépendance runtime (Python pur)

## Liens
- Story associée : STORY-238, STORY-239, STORY-240, STORY-241
