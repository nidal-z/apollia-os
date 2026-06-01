# ADR-001 - Rust comme langage principal du runtime

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

Le runtime Apollia OS doit être distribué comme binaire unique sans dépendances système (Principe #2). Il doit superviser des acteurs asynchrones en parallèle (gestion de sandbox, bridge Python, EventBus) et être sûr pour manipuler des namespaces Linux. Les agents Python existent déjà et doivent continuer à fonctionner - le runtime ne les remplace pas, il les héberge.

## Décision

Nous utilisons Rust + Tokio pour l'intégralité du runtime. Python est réservé aux agents via le bridge PyO3. Aucune partie du runtime (supervision, routing, mémoire, API) n'est écrite en Python.

## Alternatives considérées

### Option A - Go (rejetée)
**Pour :** Binaires uniques natifs, bonne concurrence, build simple.
**Contre :** Pas d'équivalent PyO3 pour intégrer l'interpréteur Python in-process. Nécessite subprocess pour les agents, ce qui interdit l'injection de `RuntimeContext` directement dans l'agent.

### Option B - Python (rejetée)
**Pour :** Même écosystème que les agents, rapidité de développement.
**Contre :** GIL limite la concurrence réelle. Packaging en binaire unique complexe (PyInstaller fragile). Performances insuffisantes pour la supervision d'acteurs. Violerait Principe #2.

### Option C - Node.js (rejetée)
**Pour :** Async natif, écosystème riche.
**Contre :** Pas de vrai binaire unique. Performances inférieures pour l'isolation sandbox. Pas d'intégration native Python en-process.

### Option retenue - Rust + Tokio
**Pour :** Binaire statique unique (Principe #2). Sécurité mémoire garantie par le compilateur. PyO3 permet d'intégrer CPython in-process sans subprocess. Tokio fournit le modèle acteur avec `mpsc::channel` natif.
**Compromis acceptés :** Courbe d'apprentissage plus élevée pour les contributeurs. Temps de compilation plus long.

## Conséquences

**Positives :**
- Binaire statique `apollia-os` fonctionne sur tout Linux sans rien installer.
- Sécurité mémoire garantie par le compilateur - pas de segfaults ni de data races.
- Tokio offre un vrai modèle acteur sans `Arc<Mutex<T>>` cross-acteurs.
- PyO3 permet d'appeler `agent.run()` directement dans le même processus.

**Négatives / Compromis :**
- Courbe d'apprentissage Rust pour les contributeurs externes.
- Temps de compilation longs (compensés par `cargo check` + build incrémental).
- Débogage plus complexe que Python.

**Neutres / À surveiller :**
- La maturité de pyo3-async-runtimes pour l'interopérabilité Tokio ↔ asyncio (STORY-026).

## Principes architecturaux impactés

- Principe #2 - Zéro dépendance externe : Rust permet le binaire statique.
- Principe #5 - Un acteur, une responsabilité : Tokio `mpsc::channel` est le standard.

## Liens

- Story associée : STORY-001 (Init workspace Cargo)
- ADR précédent sur le même sujet : aucun (décision fondatrice)
