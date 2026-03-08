# Changelog — Apollia OS

Toutes les modifications notables sont documentées ici.
Format : [Keep a Changelog](https://keepachangelog.com/fr/1.0.0/)
Versioning : [Semantic Versioning](https://semver.org/lang/fr/)

---

## [Unreleased]

### En cours
- Sprint 8 — planification à venir

---

## [0.1.0] — 2026-03 (en préparation)

Premier release public. Runtime local complet, agent Python fonctionnel, CLI opérationnelle.

### Ajouté

**AIP Bridge (PyO3)**
- Bridge Rust ↔ Python async via PyO3 + pyo3-async-runtimes
- Duck typing : tout objet Python avec `manifest()` et `async run()` est AIP-compatible
- `ToolProxy` : proxy Python pour l'invocation des outils Rust
- `MemoryInterface` : interface Python vers le Memory Engine SQLite
- `AIPBridge` : appels async Rust → Python (`call_run`, `call_on_start`, `call_on_stop`)
- `AgentLoader` trait : découplage runtime / PyO3 (ADR-019)

**Runtime Core**
- `EventBus` : broadcast Tokio pour les événements système
- `AgentRegistry` : acteur Tokio gérant le cycle de vie des agents
- `TaskRouter` : dispatch des tâches vers les `ExecutionCoordinator`
- `ExecutionCoordinator` : gestion du semaphore de concurrence par agent
- `Supervisor` : démarrage ordonné et watchdog des acteurs
- `ShutdownController` : graceful shutdown SIGTERM/SIGINT avec drain 30s
- `APIServer` : axum HTTP/REST sur Unix socket + TCP port 7771
- Routes REST : `POST /tasks`, `GET /tasks/:id`, `DELETE /tasks/:id`
- Routes REST : `GET /agents`, `POST /agents`, `GET /agents/:id`, `DELETE /agents/:id`
- Server-Sent Events : `GET /tasks/:id/stream` pour le streaming temps réel

**ORIA Engine**
- `Observer` : classification `Direct` / `Orchestrated` + `ContextBundle`
- `StepBudget` : enforcement tri-dimensionnel (steps, tool_calls, wall_clock)
- `ORIAEngine` : exécution Mode Direct avec `tokio::select!` pour le timeout
- `ResilienceLayer` : circuit breakers par outil + `RetryPolicy` backoff exponentiel avec jitter

**Tool Registry**
- `ToolRegistry` : acteur Tokio catalogue d'outils
- `ToolResolver` : validation des outils requis/optionnels au démarrage
- `AuditTrail` : SQLite WAL, fire-and-forget, sha2 pour l'input hash
- `BashExecutor` : Linux namespaces (`unshare`) + mode Dev macOS
- `PythonExecutor` : venv isolation par agent, fichier temp UUID
- `FileIo` : protection path traversal, glob matcher interne

**Memory Engine**
- `MemoryStore` : schéma SQLite + migrations automatiques
- `EpisodicMemory` : événements avec importance et timestamp
- `SemanticMemory` : faits structurés avec confiance
- `ProceduralMemory` : procédures pas à pas avec déclencheurs
- `MemorySearch` : FTS5 + BM25 cross-backend
- `MemoryManager` : namespace isolation, lazy store opening, access levels

**CLI**
- `apollia-os start/stop/status/run` — niveau 1 (opérations quotidiennes)
- `apollia-os agent list/start/stop/info` — gestion des agents
- `apollia-os task list/status/cancel` — gestion des tâches
- `apollia-os tools list/describe` — exploration du Tool Registry
- `apollia-os memory inspect` — inspection du Memory Engine
- `apollia-os audit list/stats` — audit trail
- `RuntimeClient` HTTP sur Unix socket pour tous les appels
- Exit codes POSIX : 0 succès, 1 usage, 2 runtime, 3 task failed, 4 timeout, 5 canceled
- `--json` global sur toutes les commandes

**Core**
- `AgentManifest`, `AgentSkill` — identité et capacités déclarées
- `AIPTask`, `AIPInput`, `AIPPart`, `AIPResult` — contrat de communication
- `ProcessState` — machine d'état processus agent (alignée ACP)
- `TaskStatus` — machine d'état tâche (alignée A2A)
- `StepBudgetConfig` — configuration des garde-fous

**Tests**
- 346 tests (342 sans python-tests) — unitaires + intégration + E2E
- Suite E2E : resilience, shutdown, budget, hello-agent
- CI GitHub Actions : Ubuntu, fmt + clippy + test + python-tests

### Agents d'exemple
- `agents/hello_agent.py` — agent minimal (manifest + run)
- `agents/devis_agent.py` — agent avec outils et mémoire

---

## [0.0.1] — 2026-01 (internal)

Fondations du workspace Rust. Non publié.

### Ajouté
- Workspace Cargo 7 crates
- Types fondamentaux `apollia-core`
- CI `cargo fmt + clippy + test`

---

[Unreleased]: https://github.com/nidal-z/apollia-os/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nidal-z/apollia-os/releases/tag/v0.1.0
[0.0.1]: https://github.com/nidal-z/apollia-os/releases/tag/v0.0.1
