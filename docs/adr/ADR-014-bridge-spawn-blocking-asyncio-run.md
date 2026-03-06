# ADR-014 — Bridge AIP utilise spawn_blocking + asyncio.run() au lieu de into_future

**Date :** 2026-03-06
**Statut :** Accepté
**Decideur :** Nidal (solo)
**Sprint :** 4

---

## Contexte

STORY-026 specifie l'utilisation de `pyo3_async_runtimes::tokio::into_future` pour convertir les awaitables Python (coroutines `run()`, `on_start()`, `on_stop()`) en Futures Rust, permettant un `.await` non-bloquant depuis Tokio.

En pratique, `into_future` necessite un event loop asyncio actif en arriere-plan. Ce loop doit etre initialise et maintenu par le runtime appelant. La crate `pyo3-async-runtimes` fournit un module `testing` avec un custom test harness pour resoudre ce probleme dans les tests, mais son integration :
- Impose un harness de test custom (incompatible avec `#[tokio::test]` standard)
- Requiert une initialisation globale de l'event loop asyncio
- Ajoute une complexite disproportionnee pour le Sprint 4

Sans cette initialisation, les tests avec `into_future` deadlock : le Future Rust attend que l'event loop Python drive la coroutine, mais aucun event loop ne tourne.

## Decision

Nous utilisons `tokio::task::spawn_blocking` + `asyncio.run()` pour executer les coroutines Python depuis Rust async.

Le pattern est :
1. Cloner les references Python (`Py<PyAny>`) necessaires (Send-safe)
2. `tokio::task::spawn_blocking(move || { ... })` — deplace l'execution vers le blocking thread pool
3. Dans le closure : `Python::with_gil` → appeler la methode → obtenir la coroutine → `asyncio.run(coroutine)` → extraire le resultat
4. `.await` le `JoinHandle` depuis le contexte async Rust

Ce pattern garantit :
- Le GIL n'est jamais tenu sur un worker Tokio (non-bloquant pour les taches async)
- La coroutine Python est drivee par `asyncio.run()` (event loop cree et detruit automatiquement)
- Les tests fonctionnent avec `#[tokio::test]` standard

## Alternatives considerees

### Option A — `into_future` avec custom test harness (rejetee)

**Pour :** Integration native pyo3-async-runtimes, zero overhead thread pool, partage du meme event loop entre appels
**Contre :** Necessite un custom test harness incompatible avec `#[tokio::test]`, initialisation globale complexe, complexite disproportionnee pour Sprint 4. Migration possible plus tard quand le runtime initialisera l'event loop asyncio au demarrage.

### Option B — `asyncio.run()` synchrone dans `Python::with_gil` sans `spawn_blocking` (rejetee)

**Pour :** Simple, pas de thread supplementaire
**Contre :** Bloque le worker Tokio pendant toute l'execution Python. Viole AC-6 (GIL relache pendant l'execution) et bloque les autres taches async du runtime.

### Option retenue — `spawn_blocking` + `asyncio.run()`

**Pour :** Non-bloquant pour Tokio, fonctionne avec `#[tokio::test]`, zero initialisation complexe, GIL isole sur le blocking thread pool
**Compromis acceptes :** Un thread du blocking pool est monopolise par appel Python. `asyncio.run()` cree un nouvel event loop a chaque appel (overhead negligeable pour des appels agent).

## Consequences

**Positives :**
- Tests fonctionnent avec `#[tokio::test]` standard
- Zero initialisation globale requise
- GIL jamais tenu sur un worker Tokio
- Compatible avec les agents Python existants sans modification

**Negatives / Compromis :**
- Un thread blocking par appel agent concurent (Tokio gere un pool de 512 threads par defaut)
- `asyncio.run()` cree un event loop fresh a chaque appel (pas de reutilisation)

**Neutres / A surveiller :**
- Migration vers `into_future` envisageable quand le runtime Apollia initialisera l'event loop asyncio au demarrage (potentiellement Sprint 6+)
- Si le nombre d'agents concurrents depasse ~50, surveiller la pression sur le blocking thread pool

## Principes architecturaux impactes

- Principe #5 — Un acteur, une responsabilite : Respecte — le bridge ne gere que l'appel Python, l'event loop est delegue a `asyncio.run()`

## Liens

- Story associee : STORY-026
- pyo3-async-runtimes testing : https://docs.rs/pyo3-async-runtimes/latest/pyo3_async_runtimes/testing/
- ADR precedent sur PyO3 : ADR-013
