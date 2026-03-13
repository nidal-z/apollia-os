# Décisions Architecturales (ADR)

> Registre de toutes les décisions significatives prises pendant le développement d'Apollia OS.
> Chaque ADR documente le contexte, la décision, les alternatives et les conséquences.

| ADR | Titre | Statut |
|---|---|---|
| ADR-001 | [ADR-001 — Rust comme langage principal du runtime](./adr-001-rust-comme-langage-runtime.md) | Accepté |
| ADR-002 | [ADR-002 — SQLite comme seul moteur de persistance](./adr-002-sqlite-moteur-persistance.md) | Accepté |
| ADR-003 | [ADR-003 — Duck typing pour l'Agent Interface Protocol (AIP)](./adr-003-duck-typing-aip.md) | Accepté |
| ADR-004 | [ADR-004 — Deux modes d'exécution ORIA (Direct + Orchestré)](./adr-004-deux-modes-execution-oria.md) | Accepté |
| ADR-005 | [ADR-005 — Sandbox sans Docker (Linux namespaces natifs)](./adr-005-sandbox-sans-docker.md) | Accepté |
| ADR-006 | [ADR-006 — REST JSON (pas gRPC) pour l'API locale](./adr-006-rest-json-api-locale.md) | Accepté |
| ADR-007 | [ADR-007 — Mémoire à l'initiative de l'agent](./adr-007-memoire-initiative-agent.md) | Accepté |
| ADR-008 | [ADR-008 — Pattern `noun verb` pour la CLI](./adr-008-pattern-noun-verb-cli.md) | Accepté |
| ADR-009 | [ADR-009 — Tokenizer FTS5 `unicode61` pour la recherche mémorielle](./adr-009-tokenizer-fts5-unicode61.md) | Accepté |
| ADR-010 | [ADR-010 — Pivot du SaaS Python vers le Runtime Rust open-source](./adr-010-pivot-saas-vers-runtime-rust-open-source.md) | Accepté |
| ADR-011 | [ADR-011 — AgentId et TaskId comme type aliases String dans apollia-core](./adr-011-agentid-taskid-string-aliases-dans-core.md) | Accepté |
| ADR-012 | [ADR-012 — Mode DevSandbox sur macOS : pas de sandbox réel en développement](./adr-012-sandbox-devmode-macos.md) | Accepté |
| ADR-013 | [ADR-013 — Configuration PyO3 Python sur macOS via PYO3_PYTHON](./adr-013-pyo3-python-config-macos.md) | Accepté |
| ADR-014 | [ADR-014 — Bridge AIP utilise spawn_blocking + asyncio.run() au lieu de into_future](./adr-014-bridge-spawn-blocking-asyncio-run.md) | Accepté |
| ADR-015 | [ADR-015 — Trait ToolExecutor pour abstraire l'execution des outils](./adr-015-tool-executor-trait-abstraction.md) | Accepté |
| ADR-016 | [ADR-016 — Trait AgentRunner pour decoupler ORIAEngine de AIPBridge](./adr-016-agent-runner-trait-abstraction.md) | Accepté |
| ADR-017 | [ADR-017 — hyper-util explicite pour Unix socket serving](./adr-017-hyper-util-unix-socket-serving.md) | Accepté |
| ADR-018 | [ADR-018 — CLI Bootstrap sans Supervisor](./adr-018-cli-bootstrap-sans-supervisor.md) | Accepté |
| ADR-019 | [ADR-019 — Trait AgentLoader pour decoupler apollia-runtime de PyO3](./adr-019-agent-loader-trait-decouplage-runtime-pyo3.md) | Accepté |
| ADR-020 | [ADR-020 — apollia-llm : moteur d'inférence embarqué, modèles fichiers externes, feature flags](./adr-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md) | Accepté |
| ADR-021 | [ADR-021 — apollia-triggers : TOML-only, HMAC-SHA256 webhooks, hot reload sans restart](./adr-021-apollia-triggers-toml-hmac-hot-reload.md) | Accepté |
| ADR-022 | [ADR-022 — ORIA Mode Orchestré : Option B (exécution directe outils) + hook on_plan_complete](./adr-022-oria-mode-orchestre-option-b.md) | Accepté |
| ADR-023 | [ADR-023 — HITL : AIPTask.is_resumed + InputResponse + tools_requiring_approval](./adr-023-hitl-is-resumed-input-response-tools-requiring-approval.md) | Accepté |
| ADR-024 | [ADR-024 — apollia-notifications : trait NotificationChannel, canaux, payload JSON fixe](./adr-024-apollia-notifications-trait-channel-json-fixe.md) | Accepté |
| ADR-025 | [ADR-025 — apollia-pipelines : TOML déclaratif, topologies natives, HITL intégré](./adr-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md) | Accepté |
| ADR-026 | [ADR-026 — Observabilité complète : persistance, timeline, troncature](./adr-026-observabilite-complete-persistance-timeline-troncature.md) | Accepté |
