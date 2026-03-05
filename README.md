# Apollia OS

> Runtime Rust pour l'exécution souveraine d'agents IA autonomes.

Un agent Python (LangGraph, CrewAI, custom) déposé dans Apollia OS s'exécute **isolé, outillé, et avec mémoire persistante** — sans dépendance cloud, sans Docker.

---

## Pourquoi

Les runtimes d'agents existants s'appuient sur des services cloud, imposent Docker, ou laissent les agents sans isolation ni mémoire structurée. Apollia OS est l'infrastructure locale souveraine qui manque à l'écosystème.

**Principes non négociables :**
- Zéro service externe obligatoire (pas de Redis, pas de Postgres, pas de Docker)
- Binaire unique auto-contenu
- Tout agent Python passe par un contrat minimal (`manifest()` + `run()`)

---

## Ce que ca fait

```bash
$ apollia-os start
$ apollia-os agent start ./agents/devis_agent.py
$ apollia-os run devis-agent "Devis Dupont SA, 5 jours à 850€/jour"
✔ Terminé en 3.1s — Devis #043 généré : 4 250 € HT
```

- **AIP Bridge (PyO3)** — N'importe quel agent Python s'exécute dans le runtime Rust via `pyo3-async-runtimes`
- **ORIA Engine** — Boucle d'exécution Observer-Reasoner-Actor, mode Direct (tâches simples) et mode Orchestré (tâches complexes)
- **Tool Registry** — Outils natifs (`bash_executor`, `python_executor`, `file_io`) avec sandbox Linux namespaces et audit trail SQLite
- **Memory Engine** — 4 types de mémoire (working, épisodique, sémantique, procédurale), SQLite + FTS5 `unicode61`, TTL configurable
- **Runtime Core** — Acteurs Tokio, supervision avec restart policy, graceful shutdown avec drain 30s
- **CLI** — `apollia-os agent start|stop|info|logs`, `apollia-os task list|cancel|retry`, `apollia-os memory inspect|search`

---

## Stack

| Couche | Technologie |
|---|---|
| Runtime | Rust + Tokio |
| Bridge agent | PyO3 + pyo3-async-runtimes |
| Persistance | SQLite (FTS5, WAL) |
| API locale | axum REST/JSON, Unix socket + TCP |
| CLI | clap derive |
| Sandbox | Linux user namespaces (`unshare`) |

---

## Structure du workspace

```
apollia-os/
├── crates/
│   ├── apollia-core/      # Types partagés (AgentManifest, AIPTask, RuntimeEvent…)
│   ├── apollia-runtime/   # Supervisor, EventBus, AgentRegistry, TaskRouter
│   ├── apollia-tools/     # Tool Registry, bash_executor, file_io, python_executor
│   ├── apollia-memory/    # Memory Engine, SQLite, FTS5
│   ├── apollia-aip/       # Bridge PyO3, AIPBridge, ToolProxy Python
│   ├── apollia-oria/      # ORIA Engine, modes Direct et Orchestré
│   └── apollia-cli/       # CLI clap
└── agents/
    └── devis_generator/   # Agent de démo PME
```

> Workspace Cargo en cours d'initialisation — Sprint 0

---

## Roadmap

| Sprint | Semaines | Livrable |
|---|---|---|
| 0 — Fondations | 1-2 | `cargo build` propre, types de base |
| 1 — EventBus + Registry | 3-4 | Acteurs Tokio testés |
| 2 — Tool Registry | 5-7 | `bash_executor` sandboxé, audit trail |
| 3 — Memory Engine | 8-10 | FTS5 search fonctionnel |
| **4 — Bridge PyO3 + ORIA** | **11-14** | **Agent Python s'exécute** ← sprint critique |
| 5 — API + CLI | 15-17 | Runtime opérable sans toucher le code |
| 6 — Hardening | 18-20 | Démo PME réelle, tout local |

---

## Documentation

La documentation complète est dans [`docs/`](./docs/Home.md).

| Section | Contenu |
|---|---|
| [Architecture](./docs/Architecture-Vue-Ensemble.md) | Vue d'ensemble, AIP, protocoles MCP/A2A/ACP |
| [Briques](./docs/Briques-ORIA-Engine.md) | ORIA, Tool Registry, Memory, Runtime, CLI |
| [Roadmap](./docs/Roadmap-Implementation.md) | 6 sprints détaillés avec livrables démo-ables |
| [ADR](./docs/Decisions-Log.md) | Log des décisions architecturales |

---

*Apollia — Nidal · mars 2026*
