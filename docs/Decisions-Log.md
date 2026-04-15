# Decisions Log — Apollia OS

> Journal des décisions architecturales significatives.
> Chaque entrée pointe vers le fichier ADR détaillé dans `docs/adr/`.

---

| Date | ID | Titre | Lien |
|---|---|---|---|
| 2025-XX-XX | ADR-001 | Rust comme langage du runtime | [docs/adr/ADR-001-rust-comme-langage-runtime.md](adr/ADR-001-rust-comme-langage-runtime.md) |
| 2025-XX-XX | ADR-002 | SQLite comme moteur de persistance | [docs/adr/ADR-002-sqlite-moteur-persistance.md](adr/ADR-002-sqlite-moteur-persistance.md) |
| 2025-XX-XX | ADR-003 | Duck typing AIP (manifest + run async) | [docs/adr/ADR-003-duck-typing-aip.md](adr/ADR-003-duck-typing-aip.md) |
| 2025-XX-XX | ADR-004 | Deux modes d'exécution ORIA (Direct / Orchestré) | [docs/adr/ADR-004-deux-modes-execution-oria.md](adr/ADR-004-deux-modes-execution-oria.md) |
| 2025-XX-XX | ADR-005 | Sandbox sans Docker (bubblewrap Linux) | [docs/adr/ADR-005-sandbox-sans-docker.md](adr/ADR-005-sandbox-sans-docker.md) |
| 2025-XX-XX | ADR-006 | REST JSON API locale (axum, Unix socket + TCP) | [docs/adr/ADR-006-rest-json-api-locale.md](adr/ADR-006-rest-json-api-locale.md) |
| 2025-XX-XX | ADR-007 | Mémoire à initiative de l'agent | [docs/adr/ADR-007-memoire-initiative-agent.md](adr/ADR-007-memoire-initiative-agent.md) |
| 2025-XX-XX | ADR-008 | Pattern noun-verb CLI | [docs/adr/ADR-008-pattern-noun-verb-cli.md](adr/ADR-008-pattern-noun-verb-cli.md) |
| 2025-XX-XX | ADR-009 | Tokenizer FTS5 unicode61 | [docs/adr/ADR-009-tokenizer-fts5-unicode61.md](adr/ADR-009-tokenizer-fts5-unicode61.md) |
| 2025-XX-XX | ADR-010 | Pivot SaaS vers runtime Rust open-source | [docs/adr/ADR-010-pivot-saas-vers-runtime-rust-open-source.md](adr/ADR-010-pivot-saas-vers-runtime-rust-open-source.md) |
| 2025-XX-XX | ADR-011 | AgentId / TaskId : string aliases dans core | [docs/adr/ADR-011-agentid-taskid-string-aliases-dans-core.md](adr/ADR-011-agentid-taskid-string-aliases-dans-core.md) |
| 2025-XX-XX | ADR-012 | Sandbox devmode macOS | [docs/adr/ADR-012-sandbox-devmode-macos.md](adr/ADR-012-sandbox-devmode-macos.md) |
| 2025-XX-XX | ADR-013 | PyO3 Python config macOS | [docs/adr/ADR-013-pyo3-python-config-macos.md](adr/ADR-013-pyo3-python-config-macos.md) |
| 2025-XX-XX | ADR-014 | Bridge spawn_blocking / asyncio.run | [docs/adr/ADR-014-bridge-spawn-blocking-asyncio-run.md](adr/ADR-014-bridge-spawn-blocking-asyncio-run.md) |
| 2025-XX-XX | ADR-015 | ToolExecutor trait abstraction | [docs/adr/ADR-015-tool-executor-trait-abstraction.md](adr/ADR-015-tool-executor-trait-abstraction.md) |
| 2025-XX-XX | ADR-016 | AgentRunner trait abstraction | [docs/adr/ADR-016-agent-runner-trait-abstraction.md](adr/ADR-016-agent-runner-trait-abstraction.md) |
| 2025-XX-XX | ADR-017 | hyper-util Unix socket serving | [docs/adr/ADR-017-hyper-util-unix-socket-serving.md](adr/ADR-017-hyper-util-unix-socket-serving.md) |
| 2025-XX-XX | ADR-018 | CLI bootstrap sans Supervisor | [docs/adr/ADR-018-cli-bootstrap-sans-supervisor.md](adr/ADR-018-cli-bootstrap-sans-supervisor.md) |
| 2025-XX-XX | ADR-019 | AgentLoader trait — découplage runtime PyO3 | [docs/adr/ADR-019-agent-loader-trait-decouplage-runtime-pyo3.md](adr/ADR-019-agent-loader-trait-decouplage-runtime-pyo3.md) |
| 2025-XX-XX | ADR-020 | LLM moteur embarqué — modèles externes + feature flags | [docs/adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md](adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md) |
| 2025-XX-XX | ADR-021 | Triggers TOML + HMAC + hot reload | [docs/adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md](adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md) |
| 2025-XX-XX | ADR-022 | ORIA Mode Orchestré (Option B) | [docs/adr/ADR-022-oria-mode-orchestre-option-b.md](adr/ADR-022-oria-mode-orchestre-option-b.md) |
| 2025-XX-XX | ADR-023 | HITL — is_resumed, input, response, tools requiring approval | [docs/adr/ADR-023-hitl-is-resumed-input-response-tools-requiring-approval.md](adr/ADR-023-hitl-is-resumed-input-response-tools-requiring-approval.md) |
| 2025-XX-XX | ADR-024 | Notifications — trait Channel, JSON fixe | [docs/adr/ADR-024-apollia-notifications-trait-channel-json-fixe.md](adr/ADR-024-apollia-notifications-trait-channel-json-fixe.md) |
| 2025-XX-XX | ADR-025 | Pipelines TOML déclaratif — topologies natives + HITL intégré | [docs/adr/ADR-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md](adr/ADR-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md) |
| 2025-XX-XX | ADR-026 | Observabilité complète — persistance timeline + troncature | [docs/adr/ADR-026-observabilite-complete-persistance-timeline-troncature.md](adr/ADR-026-observabilite-complete-persistance-timeline-troncature.md) |
| 2025-XX-XX | ADR-027 | Desktop — processus unique Tauri + runtime embarqué | [docs/adr/ADR-027-apollia-desktop-processus-unique-tauri-runtime-embarque.md](adr/ADR-027-apollia-desktop-processus-unique-tauri-runtime-embarque.md) |
| 2025-XX-XX | ADR-028 | Frontend Svelte — UX-first, UI sprint dédié | [docs/adr/ADR-028-frontend-svelte-ux-first-ui-sprint-dedie.md](adr/ADR-028-frontend-svelte-ux-first-ui-sprint-dedie.md) |
| 2025-XX-XX | ADR-029 | Settings lecture seule | [docs/adr/ADR-029-settings-lecture-seule.md](adr/ADR-029-settings-lecture-seule.md) |
| 2025-XX-XX | ADR-030 | EventBus Tauri events — remplace polling | [docs/adr/ADR-030-eventbus-tauri-events-remplace-polling.md](adr/ADR-030-eventbus-tauri-events-remplace-polling.md) |
| 2025-XX-XX | ADR-031 | i18n svelte-i18n FR+EN | [docs/adr/ADR-031-i18n-svelte-i18n-fr-en.md](adr/ADR-031-i18n-svelte-i18n-fr-en.md) |
| 2025-XX-XX | ADR-032 | Agent install — persistance | [docs/adr/ADR-032-agent-install-persistence.md](adr/ADR-032-agent-install-persistence.md) |
| 2025-XX-XX | ADR-033 | Config opérateur SQLite | [docs/adr/ADR-033-config-operateur-sqlite.md](adr/ADR-033-config-operateur-sqlite.md) |
| 2025-XX-XX | ADR-034 | Chat hybride — sessions, streaming, HITL inline | [docs/adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md](adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md) |
| 2025-XX-XX | ADR-035 | Per-step observation en mode Orchestré | [docs/adr/ADR-035-per-step-observation-orchestrated.md](adr/ADR-035-per-step-observation-orchestrated.md) |
| 2025-XX-XX | ADR-036 | Stratégie de cache de plans | [docs/adr/ADR-036-plan-cache-strategy.md](adr/ADR-036-plan-cache-strategy.md) |
| 2025-XX-XX | ADR-037 | Packaging Python SDK | [docs/adr/ADR-037-python-sdk-packaging.md](adr/ADR-037-python-sdk-packaging.md) |
| 2025-XX-XX | ADR-038 | Mémoire utilisateur globale | [docs/adr/ADR-038-global-user-memory.md](adr/ADR-038-global-user-memory.md) |
| 2025-XX-XX | ADR-039 | Conversation memory management | [docs/adr/ADR-039-conversation-memory-management.md](adr/ADR-039-conversation-memory-management.md) |
| 2025-XX-XX | ADR-040 | Onboarding conversationnel non-déterministe | [docs/adr/ADR-040-onboarding-conversational-agent.md](adr/ADR-040-onboarding-conversational-agent.md) |
| 2025-XX-XX | ADR-041 | Moteur STT embarqué — whisper-rs + trait SttBackend | [docs/adr/ADR-041-moteur-stt-embarque-whisper-rs-trait-stt-backend.md](adr/ADR-041-moteur-stt-embarque-whisper-rs-trait-stt-backend.md) |
| 2025-XX-XX | ADR-042 | Remplacement mistral-rs par llama.cpp statique | [docs/adr/ADR-042-remplacement-mistralrs-par-llamacpp-statique.md](adr/ADR-042-remplacement-mistralrs-par-llamacpp-statique.md) |
| 2025-XX-XX | ADR-043 | Décomposition atomique des outils | [docs/adr/ADR-043-decomposition-atomique-outils.md](adr/ADR-043-decomposition-atomique-outils.md) |
| 2025-XX-XX | ADR-044 | Client MCP natif (JSON-RPC 2.0) | [docs/adr/ADR-044-client-mcp.md](adr/ADR-044-client-mcp.md) |
| 2025-XX-XX | ADR-045 | Page Integrations — wizard générique | [docs/adr/ADR-045-page-integrations-wizard-generique.md](adr/ADR-045-page-integrations-wizard-generique.md) |
| 2025-XX-XX | ADR-046 | Transport HTTP/SSE MCP | [docs/adr/ADR-046-transport-http-sse-mcp.md](adr/ADR-046-transport-http-sse-mcp.md) |
| 2025-XX-XX | ADR-047 | Multi-LLM Backend Registry (SQLite) | [docs/adr/ADR-047-multi-llm-backend-registry.md](adr/ADR-047-multi-llm-backend-registry.md) |
| 2025-XX-XX | ADR-048 | Worker Agents — expertise de domaine compilée | [docs/adr/ADR-048-worker-agents-expertise-domaine.md](adr/ADR-048-worker-agents-expertise-domaine.md) |
| 2025-XX-XX | ADR-049 | Routing A2A inter-agents : discovery + invocation | [docs/adr/ADR-049-a2a-routing-inter-agents.md](adr/ADR-049-a2a-routing-inter-agents.md) |
| 2026-04-01 | ADR-050 | Distribution Worker Agents : bundled vs communautaire, registre local et Git | [docs/adr/ADR-050-distribution-worker-agents.md](adr/ADR-050-distribution-worker-agents.md) |
| 2026-04-03 | ADR-051 | Authentification API REST : token statique + restriction loopback | [docs/adr/ADR-051-api-auth.md](adr/ADR-051-api-auth.md) |
| 2026-04-03 | ADR-052 | Sandbox Windows : modèle Chromium 3 couches | [docs/adr/ADR-052-windows-sandbox.md](adr/ADR-052-windows-sandbox.md) |
| 2026-04-03 | ADR-053 | Pipeline fan-out et boucles conditionnelles | [docs/adr/ADR-053-pipeline-fanout-loops.md](adr/ADR-053-pipeline-fanout-loops.md) |
| 2026-04-03 | ADR-054 | Consolidation mémoire épisodique : report justifié post-v1 | [docs/adr/ADR-054-memory-episodic-consolidation.md](adr/ADR-054-memory-episodic-consolidation.md) |
| 2026-04-03 | ADR-055 | Community Registry : distribution Git-based peer-to-peer | [docs/adr/ADR-055-community-registry.md](adr/ADR-055-community-registry.md) |
| 2026-04-04 | ADR-056 | Workspace Context Assembly : subprocess git, TTL, APOLLIA.md | [docs/adr/ADR-056-workspace-context-assembly.md](adr/ADR-056-workspace-context-assembly.md) |
| 2026-04-04 | ADR-057 | Prompt Caching Strategy : 3 breakpoints ephemeral, −80% coût | [docs/adr/ADR-057-prompt-caching-strategy.md](adr/ADR-057-prompt-caching-strategy.md) |
| 2026-04-04 | ADR-058 | Context Window Management : auto-compact 80%, résumé LLM | [docs/adr/ADR-058-context-window-management.md](adr/ADR-058-context-window-management.md) |
| 2026-04-04 | ADR-059 | Concurrent Tool Execution : is_read_only + execute_batch + Semaphore(10) | [docs/adr/ADR-059-concurrent-tool-execution.md](adr/ADR-059-concurrent-tool-execution.md) |
| 2026-04-04 | ADR-060 | ContextProvider Trait : agnostique domaine, 3 niveaux d'extension | [docs/adr/ADR-060-context-provider-trait.md](adr/ADR-060-context-provider-trait.md) |
| 2026-04-04 | ADR-061 | Permission Engine 3 Couches : SafeList + RiskClassifier + InjectionDetector | [docs/adr/ADR-061-permission-engine-3-layers.md](adr/ADR-061-permission-engine-3-layers.md) |
| 2026-04-04 | ADR-062 | MCP Server Mode : StdioServerTransport, 9 outils natifs + submit_task | [docs/adr/ADR-062-mcp-server-mode.md](adr/ADR-062-mcp-server-mode.md) |
| 2026-04-04 | ADR-063 | Binary Feedback RLHF : 2 plans parallèles tokio::join!, log SQLite | [docs/adr/ADR-063-binary-feedback-rlhf.md](adr/ADR-063-binary-feedback-rlhf.md) |
| 2026-04-04 | ADR-064 | OAuth2 PKCE : keyring multi-plateforme vs fichier chiffré | [docs/adr/ADR-064-oauth2-pkce-keyring.md](adr/ADR-064-oauth2-pkce-keyring.md) |
| 2026-04-04 | ADR-065 | Auto-Updater : binaire direct + SHA256, lock file | [docs/adr/ADR-065-auto-updater-distribution.md](adr/ADR-065-auto-updater-distribution.md) |
| 2026-04-04 | ADR-066 | Memory Export/Import : format JSONL gzip, migration de schéma versionnée | [docs/adr/ADR-066-memory-export-import-format.md](adr/ADR-066-memory-export-import-format.md) |
| 2026-04-04 | ADR-067 | AWS Bedrock : aws-sigv4 natif vs SDK complet | [docs/adr/ADR-067-bedrock-sigv4-vs-sdk.md](adr/ADR-067-bedrock-sigv4-vs-sdk.md) |
| 2026-04-04 | ADR-068 | Google Vertex AI : ADC vs clé de service JSON | [docs/adr/ADR-068-vertex-adc-vs-service-account.md](adr/ADR-068-vertex-adc-vs-service-account.md) |
| 2026-04-10 | ADR-069 | Autonomie filesystem : friction graduée + journal réversible (4 couches, généralise ADR-061) | [docs/adr/ADR-069-autonomie-filesystem-friction-graduee-journal-reversible.md](adr/ADR-069-autonomie-filesystem-friction-graduee-journal-reversible.md) |
| 2026-04-15 | ADR-070 | Memory namespace project-scoped : préfixage `project_id:namespace` pour isolation mémoire inter-projets | [docs/adr/ADR-070-memory-namespace-project-scoped.md](adr/ADR-070-memory-namespace-project-scoped.md) |
| 2026-04-15 | ADR-071 | ContextBootstrap : convention de bootstrapping de contexte agent (2 méthodes abstraites, SDK 0.2.0+) | [docs/adr/ADR-071-context-bootstrap-convention.md](adr/ADR-071-context-bootstrap-convention.md) |
