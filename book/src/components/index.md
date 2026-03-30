# Composants — Apollia OS

Chaque brique du runtime, documentée en détail.

- **[AIP Bridge (PyO3)](./aip-bridge.md)** — Le pont Rust↔Python qui exécute les agents
- **[Runtime Core](./runtime-core.md)** — Supervisor, EventBus, TaskRouter : le cœur du runtime
- **[Tool Registry](./tool-registry.md)** — Catalogue d'outils natifs et MCP, sandbox, audit trail
- **[Outils Natifs](./native-tools.md)** — Référence des 10 outils intégrés (bash, python, fichiers, réseau, mémoire)
- **[Memory Engine](./memory-engine.md)** — Persistance SQLite, FTS5, mémoire épisodique/sémantique/procédurale
- **[ORIA Engine](./oria-engine.md)** — Observer-Reasoner-Actor, StepBudget, ResilienceLayer
- **[LLM Backend](./llm-backend.md)** — Backends cloud et embarqués, routing, coûts
- **[Triggers](./triggers.md)** — Cron, Interval, FileWatch, Webhook avec HMAC-SHA256
- **[Pipelines](./pipelines.md)** — DAG multi-agents, fan-out/fan-in, fallback, HITL
- **[Desktop](./desktop.md)** — Application Tauri v2 + Svelte 5
- **[STT](./stt.md)** — Speech-to-Text embarqué avec whisper-cpp
- **[CLI](./cli.md)** — Binaire `apollia-os`, commandes et flags
