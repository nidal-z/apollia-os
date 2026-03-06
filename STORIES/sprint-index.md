# Sprint Index — Apollia OS

> État courant de tous les sprints. Mettre à jour après chaque story créée ou terminée.

---

## Sprint 0 — Fondations (semaines 1-2) — LIVRE ✅

**Objectif :** Workspace Rust qui compile, tous les types de base définis.
**Livrable démo-able :** `cargo build --workspace` sans erreur + CI verte.

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-001 | Init workspace Cargo avec 7 crates | S | ✅ |
| STORY-002 | Types fondamentaux `apollia-core` (AgentManifest, AIPTask, AIPResult) | M | ✅ |
| STORY-003 | Types ProcessState, TaskStatus, AIPError avec serde | S | ✅ |
| STORY-004 | StepBudgetConfig et SandboxProfile | S | ✅ |
| STORY-005 | CI : cargo fmt + clippy + test | S | ✅ |

[Détail → sprint-0/index.md](sprint-0/index.md) | [Bilan → sprint-0/bilan.md](sprint-0/bilan.md)

---

## Sprint 1 — EventBus + AgentRegistry (semaines 3-4) — LIVRE ✅

**Objectif :** Deux premiers acteurs Tokio fonctionnels.
**Livrable démo-able :** Test d'intégration EventBus + Registry avec transitions ProcessState.

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-006 | EventBus broadcast Tokio + RuntimeEvent catalogue | M | ✅ |
| STORY-007 | AgentRegistry acteur Tokio (Register/Unregister/UpdateState) | M | ✅ |
| STORY-008 | AgentRegistryHandle API publique async | S | ✅ |
| STORY-009 | Test d'intégration EventBus ↔ AgentRegistry | M | ✅ |

[Détail → sprint-1/index.md](sprint-1/index.md) | [Plan → sprint-1/plan.md](sprint-1/plan.md) | [Bilan → sprint-1/bilan.md](sprint-1/bilan.md)

---

## Sprint 2 — Tool Registry + Outils natifs (semaines 5-7) — LIVRE ✅

**Objectif :** bash_executor sandboxé avec audit trail.
**Livrable démo-able :** `bash_executor.run("echo hello")` → stdout tracé dans SQLite.

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-010 | ToolDescriptor, ToolKind types dans apollia-tools | S | ✅ |
| STORY-011 | ToolRegistry catalogue en mémoire | M | ✅ |
| STORY-012 | ToolResolver validation à INITIALIZING | M | ✅ |
| STORY-013 | bash_executor avec Linux namespaces (unshare) | L | ✅ |
| STORY-014 | python_executor avec virtualenv isolé | L | ✅ |
| STORY-015 | file_io avec validation path traversal | M | ✅ |
| STORY-016 | Audit trail SQLite (tool_invocations) | M | ✅ |

[Plan → sprint-2/plan.md](sprint-2/plan.md) | [Bilan → sprint-2/bilan.md](sprint-2/bilan.md)

---

## Sprint 3 — Memory Engine (semaines 8-10) — LIVRE ✅

**Objectif :** Persistance souveraine FTS5 fonctionnelle.
**Livrable démo-able :** `memory.search("devis Dupont")` retourne 3 résultats classés BM25.

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-017 | Schema SQLite complet + migrations versionnées | M | ✅ |
| STORY-018 | EpisodicMemory backend (record/history/TTL) | M | ✅ |
| STORY-019 | SemanticMemory backend (remember/recall/forget) | M | ✅ |
| STORY-020 | FTS5 search avec tokenizer unicode61 + BM25 | M | ✅ |
| STORY-021 | MemoryManager namespace isolation | M | ✅ |
| STORY-022 | ProceduralMemory backend | S | ✅ |
| STORY-023 | CLI `apollia-os memory inspect` preview | S | ✅ |

[Plan → sprint-3/plan.md](sprint-3/plan.md) | [Bilan → sprint-3/bilan.md](sprint-3/bilan.md)

---

## Sprint 4 — Bridge PyO3 + ORIA Direct (semaines 11-14) — LIVRE ✅

**Objectif :** Agent Python s'exécute dans le runtime Rust.
**Livrable démo-able :** `apollia-os run hello-agent "Bonjour"` → résultat affiché.

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-024 | Chargement module Python via PyO3 | L | ✅ |
| STORY-025 | Validation AIP duck typing (manifest + run async) | M | ✅ |
| STORY-026 | Bridge Tokio ↔ asyncio via pyo3-async-runtimes | L | ✅ |
| STORY-027 | ToolProxy Python → outils Rust | M | ✅ |
| STORY-028 | MemoryInterface Python → apollia-memory | M | ✅ |
| STORY-029 | Observer + ContextBundle + classify() | M | ✅ |
| STORY-030 | ORIA Mode Direct + StepBudget enforcement | L | ✅ |
| STORY-031 | ExecutionCoordinator + sémaphore concurrence | M | ✅ |
| STORY-032 | TaskRouter dispatch | M | ✅ |

[Plan → sprint-4/plan.md](sprint-4/plan.md) | [Bilan → sprint-4/bilan.md](sprint-4/bilan.md)

---

## Sprint 5 — APIServer + CLI complète (semaines 15-17) — LIVRE ✅

**Objectif :** Runtime opérable sans modifier le code.
**Livrable démo-able :** start/stop/status/run/audit fonctionnels.

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-033 | APIServer axum Unix socket + TCP | L | ✅ |
| STORY-034 | Routes REST tasks (POST/GET/DELETE) | M | ✅ |
| STORY-035 | Routes REST agents (POST/GET/DELETE) | M | ✅ |
| STORY-036 | SSE streaming pour tâches | M | ✅ |
| STORY-037 | CLI commandes niveau 1 (start/stop/status/run) | L | ✅ |
| STORY-038 | CLI commandes niveau 2 (agent/task/tools/memory/audit) | L | ✅ |
| STORY-039 | Supervisor démarrage ordonné + watchdog | L | ✅ |
| STORY-040 | Graceful shutdown SIGTERM/drain 30s | M | ✅ |

[Détail → sprint-5/index.md](sprint-5/index.md) | [Plan → sprint-5/plan.md](sprint-5/plan.md) | [Bilan → sprint-5/bilan.md](sprint-5/bilan.md)

---

## Sprint 6 — Hardening + Agent de démo (semaines 18-19) — EN COURS

**Objectif :** Démo client réelle. Agent devis-generator opérationnel.
**Livrable démo-able :** Démo PME live, tout local, zéro cloud.

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-041 | ResilienceLayer circuit breaker par outil | L | ✅ |
| STORY-042 | Retry policy avec backoff exponentiel + jitter | M | ✅ |
| STORY-043 | ORIA Mode Orchestré + Reasoner LLM | XL | 🚫 Reportée Sprint 7 |
| STORY-044 | Agent devis-generator complet | L | ✅ |
| STORY-045 | Tests d'intégration end-to-end | L | ✅ |
| STORY-046 | README + documentation installation | M | 🔲 |

[Plan → sprint-6/plan.md](sprint-6/plan.md)

---

## Sprint 7 — ORIA Orchestré + extensions (à planifier)

| ID | Story | Taille | Statut |
|---|---|---|---|
| STORY-043 | ORIA Mode Orchestré + Reasoner LLM | XL | 🔲 (reportée Sprint 6) |

---

## Légende

| Symbole | Signification |
|---|---|
| 🔲 | À faire |
| 🔄 | En cours |
| ✅ | Terminée |
| ⏸️ | Bloquée (attente dépendance) |
| 🚫 | Annulée / reportée |
