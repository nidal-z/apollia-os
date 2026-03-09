# Summary

[Introduction](introduction.md)

---

# Démarrage rapide

- [Installation](quickstart/install.md)
- [Premier agent](quickstart/hello-agent.md)

# Guides Agents

- [Vue d'ensemble](agents/index.md)
  - [Tutoriel Hello Agent](agents/tutorial.md)
  - [RuntimeContext (ctx.*)](agents/runtime-context.md)
  - [Adapter LangGraph / CrewAI](agents/adapters.md)
  - [Mode Orchestré (ORIA)](agents/mode-orchestre.md)
  - [Bonnes pratiques](agents/best-practices.md)
  - [Troubleshooting](agents/troubleshooting.md)

# Architecture

- [Vue d'ensemble](architecture/index.md)
  - [Présentation générale](architecture/overview.md)
  - [8 Principes](architecture/principles.md)
  - [Modèle Acteur Tokio](architecture/actor-model.md)
  - [Machines d'état](architecture/state-machines.md)
  - [Standards MCP · A2A · ACP](architecture/protocols.md)
  - [Diagrammes](architecture/diagrams/index.md)

# Composants

- [Vue d'ensemble](components/index.md)
  - [AIP Bridge (PyO3)](components/aip-spec.md)
  - [Runtime Core](components/runtime-core.md)
  - [Tool Registry](components/tool-registry.md)
  - [Memory Engine](components/memory-engine.md)
  - [ORIA Engine](components/oria-engine.md)
  - [LLM Backend](components/llm-backend.md)
  - [Triggers Engine](components/triggers.md)
  - [Notifications Engine](components/notifications.md)
  - [CLI](components/cli.md)

# API & Intégration

- [Vue d'ensemble](api/index.md)
  - [API HTTP REST](api/http-reference.md)
  - [MCP Integration](api/mcp.md)
  - [A2A / ACP](api/a2a-acp.md)

# Sécurité

- [Vue d'ensemble](security/index.md)
  - [Local-First](security/local-first.md)
  - [Sandbox Isolation](security/sandbox.md)
  - [Guardrails](security/guardrails.md)

# Opérations

- [Vue d'ensemble](ops/index.md)
  - [Installation complète](ops/install.md)
  - [Production Linux](ops/production.md)
  - [Configuration apollia.toml](ops/config.md)
  - [Exploitation & Debug](ops/debug.md)
  - [Dashboard Observabilité](ops/dashboard.md)

# Décisions Architecturales

- [Index des ADRs](decisions/index.md)
  - [ADR-001 — ADR-001 — Rust comme langage principal du runtime](decisions/adr-001-rust-comme-langage-runtime.md)
  - [ADR-002 — ADR-002 — SQLite comme seul moteur de persistance](decisions/adr-002-sqlite-moteur-persistance.md)
  - [ADR-003 — ADR-003 — Duck typing pour l'Agent Interface Protocol (AIP)](decisions/adr-003-duck-typing-aip.md)
  - [ADR-004 — ADR-004 — Deux modes d'exécution ORIA (Direct + Orchestré)](decisions/adr-004-deux-modes-execution-oria.md)
  - [ADR-005 — ADR-005 — Sandbox sans Docker (Linux namespaces natifs)](decisions/adr-005-sandbox-sans-docker.md)
  - [ADR-006 — ADR-006 — REST JSON (pas gRPC) pour l'API locale](decisions/adr-006-rest-json-api-locale.md)
  - [ADR-007 — ADR-007 — Mémoire à l'initiative de l'agent](decisions/adr-007-memoire-initiative-agent.md)
  - [ADR-008 — ADR-008 — Pattern `noun verb` pour la CLI](decisions/adr-008-pattern-noun-verb-cli.md)
  - [ADR-009 — ADR-009 — Tokenizer FTS5 `unicode61` pour la recherche mémorielle](decisions/adr-009-tokenizer-fts5-unicode61.md)
  - [ADR-010 — ADR-010 — Pivot du SaaS Python vers le Runtime Rust open-source](decisions/adr-010-pivot-saas-vers-runtime-rust-open-source.md)
  - [ADR-011 — ADR-011 — AgentId et TaskId comme type aliases String dans apollia-core](decisions/adr-011-agentid-taskid-string-aliases-dans-core.md)
  - [ADR-012 — ADR-012 — Mode DevSandbox sur macOS : pas de sandbox réel en développement](decisions/adr-012-sandbox-devmode-macos.md)
  - [ADR-013 — ADR-013 — Configuration PyO3 Python sur macOS via PYO3_PYTHON](decisions/adr-013-pyo3-python-config-macos.md)
  - [ADR-014 — ADR-014 — Bridge AIP utilise spawn_blocking + asyncio.run() au lieu de into_future](decisions/adr-014-bridge-spawn-blocking-asyncio-run.md)
  - [ADR-015 — ADR-015 — Trait ToolExecutor pour abstraire l'execution des outils](decisions/adr-015-tool-executor-trait-abstraction.md)
  - [ADR-016 — ADR-016 — Trait AgentRunner pour decoupler ORIAEngine de AIPBridge](decisions/adr-016-agent-runner-trait-abstraction.md)
  - [ADR-017 — ADR-017 — hyper-util explicite pour Unix socket serving](decisions/adr-017-hyper-util-unix-socket-serving.md)
  - [ADR-018 — ADR-018 — CLI Bootstrap sans Supervisor](decisions/adr-018-cli-bootstrap-sans-supervisor.md)
  - [ADR-019 — ADR-019 — Trait AgentLoader pour decoupler apollia-runtime de PyO3](decisions/adr-019-agent-loader-trait-decouplage-runtime-pyo3.md)
  - [ADR-020 — ADR-020 — apollia-llm : moteur d'inférence embarqué, modèles fichiers externes, feature flags](decisions/adr-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md)
  - [ADR-021 — apollia-triggers : TOML-only, HMAC-SHA256 webhooks, hot reload sans restart](decisions/adr-021-apollia-triggers-toml-hmac-hot-reload.md)
  - [ADR-022 — ORIA Mode Orchestré : Option B (exécution directe outils) + hook on_plan_complete](decisions/adr-022-oria-mode-orchestre-option-b.md)
  - [ADR-023 — HITL : AIPTask.is_resumed + InputResponse + tools_requiring_approval](decisions/adr-023-hitl-is-resumed-input-response-tools-requiring-approval.md)
  - [ADR-024 — apollia-notifications : trait NotificationChannel, canaux, payload JSON fixe](decisions/adr-024-apollia-notifications-trait-channel-json-fixe.md)

---

[Roadmap](roadmap.md)
