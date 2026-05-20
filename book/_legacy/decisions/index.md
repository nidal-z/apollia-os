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
| ADR-014 | [ADR-014 — Bridge AIP utilise spawn_blocking + asyncio.run au lieu de into_future](./adr-014-bridge-spawn-blocking-asyncio-run.md) | Accepté |
| ADR-015 | [ADR-015 — Trait ToolExecutor pour abstraire l'execution des outils](./adr-015-tool-executor-trait-abstraction.md) | Accepté |
| ADR-016 | [ADR-016 — Trait AgentRunner pour decoupler ORIAEngine de AIPBridge](./adr-016-agent-runner-trait-abstraction.md) | Accepté |
| ADR-017 | [ADR-017 — hyper-util explicite pour Unix socket serving](./adr-017-hyper-util-unix-socket-serving.md) | Accepté |
| ADR-018 | [ADR-018 — CLI Bootstrap sans Supervisor](./adr-018-cli-bootstrap-sans-supervisor.md) | Accepté |
| ADR-019 | [ADR-019 — Trait AgentLoader pour decoupler apollia-runtime de PyO3](./adr-019-agent-loader-trait-decouplage-runtime-pyo3.md) | Accepté |
| ADR-020 | [ADR-020 — apollia-llm : moteur d'inférence embarqué, modèles fichiers externes, feature flags](./adr-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md) | Accepté |
| ADR-021 | [ADR-021 — apollia-triggers : TOML-only, HMAC-SHA256 webhooks, hot reload sans restart](./adr-021-apollia-triggers-toml-hmac-hot-reload.md) | Partiellement remplacé par ADR-033 |
| ADR-022 | [ADR-022 — ORIA Mode Orchestré : Option B (exécution directe outils) + hook on_plan_complete](./adr-022-oria-mode-orchestre-option-b.md) | Accepté |
| ADR-023 | [ADR-023 — HITL : AIPTask.is_resumed + InputResponse + tools_requiring_approval](./adr-023-hitl-is-resumed-input-response-tools-requiring-approval.md) | Accepté |
| ADR-024 | [ADR-024 — apollia-notifications : trait NotificationChannel, canaux, payload JSON fixe](./adr-024-apollia-notifications-trait-channel-json-fixe.md) | Accepté |
| ADR-025 | [ADR-025 — apollia-pipelines : TOML déclaratif, topologies natives, HITL intégré](./adr-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md) | Superseded (crate retirée) |
| ADR-026 | [ADR-026 — Observabilité complète : persistance, timeline, troncature](./adr-026-observabilite-complete-persistance-timeline-troncature.md) | Accepté |
| ADR-027 | [ADR-027 — apollia-desktop : processus unique Tauri + runtime embarqué](./adr-027-apollia-desktop-processus-unique-tauri-runtime-embarque.md) | Accepté |
| ADR-028 | [ADR-028 — Frontend Svelte : UX first, UI sprint dédié](./adr-028-frontend-svelte-ux-first-ui-sprint-dedie.md) | Accepté |
| ADR-029 | [ADR-029 — Settings lecture seule dans l'application desktop](./adr-029-settings-lecture-seule.md) | Accepté |
| ADR-030 | [ADR-030 — EventBus → Tauri events remplace le polling IPC](./adr-030-eventbus-tauri-events-remplace-polling.md) | Accepté |
| ADR-031 | [ADR-031 — Stratégie i18n : svelte-i18n avec fichiers JSON FR/EN](./adr-031-i18n-svelte-i18n-fr-en.md) | Accepté |
| ADR-032 | [ADR-032 — Agent Install & Persistence dans ~/.apollia/agents/](./adr-032-agent-install-persistence.md) | Accepté |
| ADR-033 | [ADR-033 — Config opérateur SQLite : séparation structurel / opérationnel](./adr-033-config-operateur-sqlite.md) | Accepté |
| ADR-034 | [ADR-034 — Chat hybride : sessions, streaming, HITL inline](./adr-034-chat-hybride-sessions-streaming-hitl-inline.md) | Accepté |
| ADR-035 | [ADR-035 — Per-step observation en mode Orchestré](./adr-035-per-step-observation-orchestrated.md) | Accepté |
| ADR-036 | [ADR-036 — Stratégie de cache de plans ORIA](./adr-036-plan-cache-strategy.md) | Accepté |
| ADR-037 | [ADR-037 — Packaging Python SDK](./adr-037-python-sdk-packaging.md) | Accepté |
| ADR-038 | [ADR-038 — Mémoire utilisateur globale](./adr-038-global-user-memory.md) | Accepté |
| ADR-039 | [ADR-039 — Conversation memory management](./adr-039-conversation-memory-management.md) | Accepté |
| ADR-040 | [ADR-040 — Onboarding comme agent conversationnel](./adr-040-onboarding-conversational-agent.md) | Accepté |
| ADR-041 | [ADR-041 — Moteur STT embarqué : whisper-rs, trait SttBackend](./adr-041-moteur-stt-embarque-whisper-rs-trait-stt-backend.md) | Accepté |
| ADR-042 | [ADR-042 — Remplacement de mistral.rs par llama.cpp statique](./adr-042-remplacement-mistralrs-par-llamacpp-statique.md) | Accepté |
| ADR-043 | [ADR-043 — Décomposition atomique des outils natifs](./adr-043-decomposition-atomique-outils.md) | Accepté |
| ADR-044 | [ADR-044 — Client MCP : architecture, transport, lifecycle](./adr-044-client-mcp.md) | Accepté |
| ADR-045 | [ADR-045 — Page Intégrations : wizard générique piloté par MCP Registry](./adr-045-page-integrations-wizard-generique.md) | Accepté |
| ADR-046 | [ADR-046 — Transport HTTP/SSE pour MCP](./adr-046-transport-http-sse-mcp.md) | Accepté |
| ADR-047 | [ADR-047 — Multi-LLM Backend Registry : SQLite + binding par agent](./adr-047-multi-llm-backend-registry.md) | Accepté |
| ADR-048 | [ADR-048 — Worker Agents : expertise de domaine compilée en Python](./adr-048-worker-agents-expertise-domaine.md) | Accepté |
| ADR-049 | [ADR-049 — Routing A2A inter-agents : discovery + invocation synchrone](./adr-049-a2a-routing-inter-agents.md) | Accepté |
| ADR-050 | [ADR-050 — Distribution Worker Agents : bundled vs communautaire](./adr-050-distribution-worker-agents.md) | Accepté |
| ADR-051 | [ADR-051 — Authentification API REST TCP : token statique + restriction loopback](./adr-051-api-auth.md) | Accepté |
| ADR-052 | [ADR-052 — Sandbox Windows : modèle Chromium 3 couches](./adr-052-windows-sandbox.md) | Accepté |
| ADR-053 | [ADR-053 — Pipeline fan-out et boucles conditionnelles](./adr-053-pipeline-fanout-loops.md) | Superseded (crate retirée) |
| ADR-054 | [ADR-054 — Consolidation mémoire épisodique : report justifié post-v1](./adr-054-memory-episodic-consolidation.md) | Accepté |
| ADR-055 | [ADR-055 — Community Registry : distribution Git-based peer-to-peer](./adr-055-community-registry.md) | Accepté |
| ADR-056 | [ADR-056 — Workspace Context Assembly : subprocess git, TTL, APOLLIA.md](./adr-056-workspace-context-assembly.md) | Accepté |
| ADR-057 | [ADR-057 — Prompt Caching Strategy : 3 breakpoints ephemeral, -80% coût](./adr-057-prompt-caching-strategy.md) | Accepté |
| ADR-058 | [ADR-058 — Context Window Management : auto-compact 80%, résumé LLM](./adr-058-context-window-management.md) | Accepté |
| ADR-059 | [ADR-059 — Concurrent Tool Execution : is_read_only + execute_batch + Semaphore](./adr-059-concurrent-tool-execution.md) | Accepté |
| ADR-060 | [ADR-060 — ContextProvider Trait : agnostique domaine, 3 niveaux d'extension](./adr-060-context-provider-trait.md) | Accepté |
| ADR-061 | [ADR-061 — Moteur de permissions 3 couches : SafeList + PrefixRuleEngine + InjectionDetector](./adr-061-permission-engine-3-layers.md) | Accepté |
| ADR-062 | [ADR-062 — MCP server mode : transport stdio, 9 outils natifs + submit_task](./adr-062-mcp-server-mode.md) | Accepté |
| ADR-063 | [ADR-063 — Binary feedback RLHF : deux plans parallèles, log SQLite du choix](./adr-063-binary-feedback-rlhf.md) | Accepté |
| ADR-064 | [ADR-064 — OAuth2 PKCE : Keyring Multi-Plateforme vs Fichier Chiffré](./adr-064-oauth2-pkce-keyring.md) | Accepté |
| ADR-065 | [ADR-065 — Auto-Updater : Binaire Direct + SHA256](./adr-065-auto-updater-distribution.md) | Accepté |
| ADR-066 | [ADR-066 — Memory Export/Import : Format JSON Gzip](./adr-066-memory-export-import-format.md) | Accepté |
| ADR-067 | [ADR-067 — AWS Bedrock : aws-sigv4 Natif vs SDK Complet](./adr-067-bedrock-sigv4-vs-sdk.md) | Accepté |
| ADR-068 | [ADR-068 — Google Vertex AI : ADC vs Clé de Service JSON](./adr-068-vertex-adc-vs-service-account.md) | Accepté |
| ADR-069 | [ADR-069 — Autonomie Filesystem : Friction Graduée + Journal Réversible](./adr-069-autonomie-filesystem-friction-graduee-journal-reversible.md) | Accepté |
| ADR-070 | [ADR-070 — Memory Namespace Project-Scoped](./adr-070-memory-namespace-project-scoped.md) | Accepté |
| ADR-071 | [ADR-071 — ContextBootstrap : Convention de Bootstrapping Agent](./adr-071-context-bootstrap-convention.md) | Accepté |
