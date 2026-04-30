# Sprint Summary — Apollia OS

> Vue consolidée de tous les sprints : ce qui a été livré, les primitives agent disponibles, et les écarts par rapport aux specs.
> Dernière mise à jour : 2026-04-15.

---

## Sprint 0 — Fondations

**Statut :** LIVRÉ ✅ | **Stories :** 5/5 | **Crates :** 7 squelettes + apollia-core peuplé

### Ce qui a été implémenté
- Workspace Cargo avec 7 crates (`apollia-core`, `apollia-runtime`, `apollia-oria`, `apollia-tools`, `apollia-memory`, `apollia-aip`, `apollia-cli`)
- Types fondamentaux dans `apollia-core` : `AgentManifest`, `AIPTask`, `AIPResult`, `ProcessState`, `TaskStatus`, `AIPError`, `StepBudgetConfig`, `SandboxProfile`
- CI GitHub Actions : `cargo fmt` + `clippy` + `test` chaînés avec `rust-cache`

### Primitives agent disponibles
Aucune — types définis mais pas encore de runtime.

### Écarts
- 6 crates vides (squelettes uniquement — normal pour Sprint 0)
- DT-001→005 : `Cargo.lock` non commité, CI Linux only, sprint-index non synchronisé automatiquement

---

## Sprint 1 — EventBus + AgentRegistry

**Statut :** LIVRÉ ✅ | **Stories :** 4/4 | **Tests :** 17

### Ce qui a été implémenté
- `EventBus` broadcast Tokio (`tokio::broadcast`) avec catalogue `RuntimeEvent`
- `AgentRegistry` acteur Tokio (Register / Unregister / UpdateState) avec transitions `ProcessState`
- `AgentRegistryHandle` API publique async (Clone + Send + Sync)
- Test d'intégration EventBus ↔ Registry

### Primitives agent disponibles
Aucune directement — infrastructure acteur interne.

### Écarts
- Spec prévoyait 7 `RuntimeEvent` au test d'intégration, réalité = 6 (pas de variant `AgentStopping`)
- `AgentRegistry` rendu `pub` (au lieu de `pub(crate)`) pour accès depuis tests d'intégration
- **ADR-011** (non planifié) : `AgentId`/`TaskId` comme type aliases `String` au lieu de newtypes

---

## Sprint 2 — Tool Registry + Outils natifs

**Statut :** LIVRÉ ✅ | **Stories :** 7/7 | **Tests :** 55

### Ce qui a été implémenté
- `ToolDescriptor`, `ToolKind`, `McpTransport` dans `apollia-tools`
- `ToolRegistry` acteur Tokio + `ToolRegistryHandle`
- `ToolResolver` : validation des outils requis/optionnels au démarrage agent
- 3 outils natifs : `bash_executor` (Linux namespaces via `unshare`, mode Dev macOS), `python_executor` (venv isolé par agent), `file_io` (protection path traversal + glob matcher) — *Note : `file_io` a été déprécié au Sprint 25 et remplacé par `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep` (ADR-043)*
- `AuditTrail` SQLite (acteur `std::thread` + `mpsc::sync_channel`, fire-and-forget, SHA-256)

### Primitives agent disponibles
- `bash_executor` : exécution shell sandboxée
- `python_executor` : exécution Python isolée (venv par agent)
- `file_io` : lecture/écriture fichiers avec protection traversal

### Écarts
- STORY-015 livrée avant STORY-013 (ordre inversé vs plan)
- `AgentManifest` étendu en cours de sprint avec `dangerous_tools_allowed: bool` (friction cross-crate ~30min)
- **ADR-012** (non planifié) : `SandboxMode::Dev` macOS via `#[cfg]` (sandbox-exec deprecated)
- DT-010/011 : cgroups / mount namespace hardening incomplet (suffisant pour MVP)

---

## Sprint 3 — Memory Engine

**Statut :** LIVRÉ ✅ | **Stories :** 7/7 | **Tests :** 56

### Ce qui a été implémenté
- `MemoryStore` schema SQLite + migrations versionnées
- 3 backends : `EpisodicMemory` (record/history/TTL), `SemanticMemory` (remember/recall/forget), `ProceduralMemory`
- `MemorySearch` FTS5 avec tokenizer `unicode61` + ranking BM25
- `MemoryManager` : isolation par namespace, lazy store opening, access levels ReadWrite/ReadOnly
- CLI preview : `apollia-os memory inspect`

### Primitives agent disponibles
- Mémoire épisodique : `record()`, `history()`, `purge_before()`
- Mémoire sémantique : `remember()`, `recall()`, `forget()`, `search()`
- Mémoire procédurale : stockage de patterns/procédures apprises
- Recherche full-text FTS5 cross-backends

### Écarts
- `semantic.rs` (545 lignes) et `search.rs` (477 lignes) dépassent la limite de 300 lignes (tests inline ~50%)
- DT-017 : `MemoryManager` non implémenté comme acteur Tokio (accès direct `Arc<Mutex>`)
- Pas de purge auto TTL (DT-016)

---

## Sprint 4 — Bridge PyO3 + ORIA Direct

**Statut :** LIVRÉ ✅ | **Stories :** 9/9 | **Tests :** 32 (apollia-aip) + 16 (apollia-oria)

### Ce qui a été implémenté
- Chargement module Python via PyO3 (`AIPLoader`)
- Validation duck typing : `manifest()` + `async run()` + callbacks optionnels
- Bridge Tokio ↔ asyncio via `spawn_blocking` + `asyncio.run()` (ADR-014)
- `ToolProxy` #[pyclass] : `call()`, `list_tools()`, `tool_call_count()`
- `MemoryInterface` #[pyclass] : `record()`, `remember()`, `recall()`, `search()`, `forget()`
- `Observer` + `ContextBundle` + `classify()` (4 heuristiques, `ExecutionMode` Direct/Orchestrated)
- `ORIAEngine` + `StepBudget` tri-dimensionnel (steps, tool_calls, wall_clock)
- `ExecutionCoordinator` avec semaphore Tokio + `TaskRouter` dispatch

### Primitives agent disponibles
- `ctx.tools.call(name, args)` → appel outil Rust depuis Python
- `ctx.tools.list_tools()` → liste des outils disponibles
- `ctx.memory.record(content, importance)` → mémoire épisodique
- `ctx.memory.remember(key, value)` → mémoire sémantique
- `ctx.memory.recall(key)` → rappel sémantique
- `ctx.memory.search(query)` → recherche FTS5
- `ctx.memory.forget(key)` → suppression

### Écarts
- `#[allow(clippy::useless_conversion)]` nécessaire (faux positif PyO3)
- `PYO3_PYTHON` obligatoire sur macOS → **ADR-013** (non planifié)
- `Arc<Mutex<MemoryManager>>` au lieu de pattern acteur (DT-023 — compromis single-agent)
- **ADR-014** (non planifié) : `spawn_blocking` + `asyncio.run()` au lieu de `into_future`
- **ADR-015** (non planifié) : trait `ToolExecutor` (dependency injection — réutilisé Sprints 6, 12, 20)
- **ADR-016** (non planifié) : trait `AgentRunner` (testabilité sans Python)

---

## Sprint 5 — APIServer + CLI complète

**Statut :** LIVRÉ ✅ | **Stories :** 8/8 | **Tests :** 73 (apollia-runtime) + 35 (apollia-cli)

### Ce qui a été implémenté
- `APIServer` axum dual TCP (port 7771) + Unix socket (`/tmp/apollia.sock`)
- Routes REST : tasks (POST/GET/DELETE), agents (GET/POST/DELETE), SSE streaming
- CLI niveau 1 : `start()`, `stop()`, `status`, `run()`
- CLI niveau 2 : `agent list|start|stop|info`, `task list|status|cancel`, `tools list|describe`, `audit list|stats`, `memory inspect`
- `Supervisor` démarrage ordonné séquentiel + watchdog + rollback
- `ShutdownController` graceful (SIGTERM/SIGINT, drain 30s, double Ctrl+C = force exit)

### Primitives agent disponibles
Pas de nouvelles primitives Python. API REST et CLI opérationnelles pour administrer les agents.

### Écarts
- axum 0.7.9 : path params `:id` (pas `{id}` qui est 0.8+)
- `manifest_from_path()` MVP sans chargement Python réel — DT-031 (résolu Sprint 6 via ADR-019)
- Fichiers longs : `shutdown.rs` (829 loc), `router.rs` (649 loc), `supervisor.rs` (623 loc)
- **ADR-017** (non planifié) : hyper-util explicite pour Unix socket (axum 0.7 ne supporte pas nativement)
- **ADR-018** (non planifié) : CLI bootstrap sans Supervisor

---

## Sprint 6 — Hardening + Agent démo

**Statut :** LIVRÉ ✅ | **Stories :** 6/6 | **Tests :** 336 workspace

### Ce qui a été implémenté
- `ResilienceLayer` : circuit breaker per-outil (Closed/Open/HalfOpen) + cooldown
- `RetryPolicy` : backoff exponentiel `base*2^(n-1)` + jitter ±25% (`rand` crate)
- `Reasoner` ORIA fonctionnel (prompt planner, `ExecutionPlan`, retry ×3 JSON invalide)
- Agent `devis-generator` et `hello-agent` opérationnels
- `AgentLoader` trait (ADR-019) → découplage runtime/PyO3
- Tests e2e : `test_resilience.rs`, `test_shutdown_e2e.rs`, `test_budget_e2e.rs`, `test_hello_agent.rs`

### Primitives agent disponibles
- Résilience automatique : retry + circuit breaker transparent pour les appels outils

### Écarts
- **STORY-043** (ORIA Mode Orchestré) reportée → soldée par STORY-061 dans Sprint 8
- DT-031 plus profond qu'anticipé → nécessité **ADR-019** (AgentLoader trait) + refactoring `AppState`, `Supervisor::start()`, `routes_agents`
- `apollia-cli` dépend maintenant de `apollia-aip` (couplage non prévu initialement)
- Tests `python-tests` ne tournent pas en CI Linux (DT-035)

---

## Sprint 7 — Hardening tests + CI verte

**Statut :** LIVRÉ ✅ | **Stories :** 4/4 | **Tests :** 340+ workspace

### Ce qui a été implémenté
- Tests regression MVP : `Working→Completed` EventBus, `find_by_name` AgentRegistry
- Test intégration chaîne complète : `start_agent → submit → Completed` (MockBackend)
- CI Ubuntu : `actions/setup-python@v5` + `PYO3_PYTHON` via `$GITHUB_ENV` + `python-tests` feature

### Primitives agent disponibles
Aucune nouvelle.

### Écarts
Aucun — sprint purement technique, 4/4 livré conforme.

---

## Sprint 8 — apollia-llm : moteur embarqué + ctx.llm

**Statut :** LIVRÉ ✅ | **Stories :** 14/14 | **Crate :** `apollia-llm` créée

### Ce qui a été implémenté
- Trait `CompletionModel` (async-trait) + types `CompletionRequest`/`CompletionResponse`/`LlmError`
- `EmbeddedBackend` : chargement `.gguf` via mistral-rs, inférence in-process (feature `local`)
- `OpenAICompatibleClient` : `async-openai` (feature `cloud`)
- `AnthropicClient` : `reqwest` direct, SSE streaming (feature `cloud`)
- `LlmRouter` : multi-backend, `from_config()`, default backend, `list()`, `get(name?)`
- `ToolCallHelper.run_tools()` : boucle ReAct, garde-fous `max_iterations` + `StepBudget`
- Observabilité : `LlmCallCompleted` EventBus, `cost_usd` cloud, `debug_log_prompt` TRACE
- CLI : `llm status|ping|chat`, `model list`
- STORY-043 soldée via STORY-061 : `Reasoner` fonctionnel avec `Arc<dyn CompletionModel>`

### Primitives agent disponibles
- **`ctx.llm`** — Proxy LLM complet :
  - `ctx.llm.chat(messages)` → réponse LLM
  - `ctx.llm.complete(messages)` → complétion
  - `ctx.llm.stream(messages)` → streaming token par token
  - `ctx.llm.run_tools(messages, tools)` → boucle ReAct avec appels outils
- `ctx.llm` est `None` si aucun backend configuré (agent démarre en `DEGRADED`)

### Écarts
- `EmbeddedBackend` : features `local-cuda` / `local-metal` déclarées mais deps GPU non encore activées (`objc2-metal ^0.3.2` absent crates.io au moment du sprint)
- Metal support ajouté post-sprint : `local-metal` feature + `MISTRALRS_METAL_PRECOMPILE=0`
- `serde(default)` sur `device` → `Cpu` si absent du TOML (choix pragmatique)

---

## Sprint 9 — apollia-triggers + Dashboard

**Statut :** LIVRÉ ✅ | **Stories :** 14/14 | **Crate :** `apollia-triggers` créée

### Ce qui a été implémenté
- `TriggerEngine` acteur Tokio (position 6 Supervisor) avec sources :
  - **Tier 1** : `CronTrigger` (crate `cron 0.12`), `IntervalTrigger`, `OneshotTrigger`
  - **Tier 2** : `FileWatchTrigger` (notify v6, bridge sync→async)
  - **Tier 3** : Webhook (`POST /webhooks/:id`, HMAC-SHA256 + `constant_time_eq`)
- Persistance SQLite : `trigger_history`, `trigger_state` (migration 003)
- Hot reload : `POST /api/v1/triggers/reload` sans restart runtime
- Dashboard HTMX 100% embarqué via `include_str!` + SSE 5 canaux nommés
- CLI : `trigger list|status|fire|enable|disable|logs|reload`
- 6 nouveaux `RuntimeEvent` : `TriggerFired/Skipped/Error/Enabled/Disabled/TriggersReloaded`

### Primitives agent disponibles
Les agents ne déclenchent pas directement les triggers — les triggers déclenchent les agents.

### Écarts
- **ADR-021** : TOML-only (pas SQLite pour les définitions — migré SQLite en Sprint 17), HMAC-SHA256 header `X-Apollia-Signature`, hot reload timeout 2s + abort forcé

---

## Sprint 10 — ORIA Mode Orchestré

**Statut :** LIVRÉ ✅ | **Stories :** 13/13

### Ce qui a été implémenté
- `Reasoner` : prompts planner/replanner, `parse_and_validate()`, retry ×3 JSON invalide
- `PlanRepository` SQLite (migration `004_execution_plans.sql` → `~/.apollia/plans.db`)
- `topological_sort()` (Kahn BFS) + détection cycle DFS
- `ActorLoop.execute()` : exécution topologique, `StepBudget`, `ResilienceLayer`, replan max 2
- 7 nouveaux `RuntimeEvent` : `PlanGenerated`, `StepStarted/Completed/Failed`, `PlanReplanning`, `PlanCompleted/Failed`
- `on_plan_complete()` hook PyO3 duck typing optionnel
- SSE étendu pour events plan/step
- CLI `run()` enrichi (plan + steps temps réel) + `task inspect` (lecture plans.db sans runtime)

### Primitives agent disponibles
- `execution_mode = "orchestrated"` dans manifest → ORIA planifie et exécute les outils directement
- `on_plan_complete(step_results: dict)` → hook optionnel appelé à la fin du plan
- `system_prompt` dans manifest → prompt injecté au Reasoner pour la planification

### Écarts
- **ADR-022** : Option B retenue — ORIA exécute les outils directement, `agent.run()` n'est PAS appelé pendant les steps (écart majeur vs option A qui aurait délégué à l'agent)
- `spawn_blocking` nécessaire pour SQLite `!Send` dans futures async

---

## Sprint 11 — HITL + Notifications

**Statut :** LIVRÉ ✅ | **Stories :** 15/15 | **Crate :** `apollia-notifications` créée

### Ce qui a été implémenté
- `AIPResult.input_required(prompt, context)` → suspension agent
- `tools_requiring_approval` dans `AgentManifest` → approbation par outil
- Migration `005_hitl_tables.sql` + `TaskRepository.save_input_required()/rebuild_for_resume()`
- `ResumeHandler` : `POST /api/v1/tasks/{id}/resume`
- ORIA HITL : Mode Direct (oneshot channel) + Mode Orchestré (mid-plan suspend)
- `TimeoutWatcher` : scan 60s, annulation `input_required` expirées
- `NotificationEngine` acteur (position 9 Supervisor) : trait `NotificationChannel`
- Canaux : `DesktopChannel` (notify-rust v4) + `WebhookChannel` (payload JSON fixe)
- CLI : `task list --pending-approval`, `task resume --approve/--reject`, `notify test|list|logs`

### Primitives agent disponibles
- `AIPResult.input_required(prompt, context)` → suspendre l'agent pour demander une approbation
- `task.is_resumed` → booléen indiquant si la tâche reprend après une approbation
- `tools_requiring_approval: ["tool_name"]` dans manifest → approbation automatique par outil
- `InputResponseData` injectée dans la tâche au resume

### Écarts
- **ADR-023** : `AIPTask.is_resumed` + `InputResponse` + `tools_requiring_approval` (conception validée)
- **ADR-024** : Notifications trait + payload JSON fixe (pas de template personnalisable)

---

## Sprint 12 — Orchestration multi-agent (Pipelines)

**Statut :** LIVRÉ ✅ | **Stories :** 18/18 | **Crate :** `apollia-pipelines` créée — ⚠️ **retirée du workspace v0.1.0** (composition multi-agent désormais via triggers + agents ReAct autonomes, ADR-066)

### Ce qui a été implémenté
- Types : `PipelineDefinition`, `PipelineRun`, `StepRun`, `PipelineStatus`, `StepRunStatus`
- `PipelineExecutor` : exécution topologique par layers (`FuturesUnordered` fan-out/fan-in)
- `evaluate_condition()` : 5 opérateurs (Contains/Equals/StartsWith/EndsWith/Regex)
- Fallback : `activate_fallback()` + recalcul graphe
- HITL pipelines : `PipelineSuspended` + `wait_for_resume()`
- `PipelineEngine` acteur Tokio (position 8 Supervisor) + reprise runs au boot
- `TemplateRenderer` : `{{steps.x.output}}` pour chaîner les outputs
- Triggers → Pipelines : champ `pipeline` dans `TriggerDefinition` (XOR `agent`)
- 9 nouveaux `RuntimeEvent` Pipeline
- API REST + CLI + Dashboard SSE

### Primitives agent disponibles
Les agents participent aux pipelines sans le savoir — le pipeline orchestre leurs exécutions en séquence/parallèle.

### Écarts
- **ADR-025** : Pipeline déclaratif TOML + topologies DAG natives + HITL intégré (conforme à la spec)
- Aucun écart notable — 18/18 stories livrées sans dette nouvelle

---

## Sprint 13 — Observabilité complète

**Statut :** LIVRÉ ✅ | **Stories :** 10/12 (2 abandonnées)

### Ce qui a été implémenté
- `TaskRecord` enrichi : `input_text`, `output_text`, `duration_ms`, `transitions_json`
- `StepRecord` enrichi : `input_rendered`, `output_text`, `tool_used`, `error_detail`, `duration_ms`
- `ToolCallRecord` enrichi : `args_json`, `stdout`, `stderr`
- `LlmCallRepository` NEW : table `llm_calls` dans `apollia-llm`
- `TriggerFireRecord` enrichi : `payload_json`, `dispatch_ms`
- HITL enrichi : `suspended_at`, `wait_duration_ms`
- `truncate_with_marker()` UTF-8 safe pour les champs longs
- Timeline API : `GET /api/v1/tasks/{id}/timeline` — agrège 5 sources SQLite, 9 types `TimelineEvent`

### Primitives agent disponibles
Aucune nouvelle primitive Python — l'observabilité est automatique et transparente pour les agents.

### Écarts
- **STORY-133** 🚫 abandonnée (Dashboard HTMX observabilité) — migration Tauri+Svelte prévue
- **STORY-134** 🚫 abandonnée (Tests e2e observabilité) — couverture unitaire jugée suffisante
- **ADR-026** : Timeline unifiée 5 sources SQLite (conforme)

---

## Sprint 14 — Application desktop native

**Statut :** LIVRÉ ✅ | **Stories :** 8/8 | **Crate :** `apollia-desktop` créée (Tauri v2)

### Ce qui a été implémenté
- Crate `apollia-desktop` : Tauri v2 + Svelte 5 + shadcn-svelte
- 9 Tauri commands IPC (agents, tasks, HITL)
- 3 routes Svelte : Agents, Tasks (timeline), Approvals
- SSE stores Svelte (agents, tasks, pendingApprovals)
- Build + packaging (.dmg / .AppImage)

### Primitives agent disponibles
Aucune — les agents ne sont pas conscients du desktop.

### Écarts
- **ADR-027** : Processus unique Tauri (runtime embarqué dans l'app, pas de daemon séparé)
- **ADR-028** : Frontend Svelte UX first, UI sprint dédié (séparer fonctionnel et polish)

---

## Sprint 15 — Svelte frontend complet

**Statut :** LIVRÉ ✅ | **Stories :** 13/13

### Ce qui a été implémenté
- 10 routes Svelte (Agents, Tasks, Approvals, LLM, Triggers, Pipelines, Memory, Notifications, Observabilité, Settings)
- 29 Tauri IPC commands (vs 9 Sprint 14)
- 7 SSE stores + 4 derived
- 35+ types TypeScript
- Sidebar restructurée 4 catégories (Operations / Infrastructure / Données / Settings)
- Onboarding wizard 3 étapes (environment check, first agent, first task)
- System tray : hide on close, native notifications, approval counter, graceful quit
- `data-testid` attributes pour e2e
- Plugins : `tauri-plugin-dialog`, `tauri-plugin-notification`

### Primitives agent disponibles
Aucune — frontend uniquement.

### Écarts
- **ADR-029** : Settings en lecture seule dans l'app desktop (pas de CRUD settings — choix délibéré)

---

## Sprint 17 — Config opérateur CRUD SQLite

**Statut :** LIVRÉ ✅ | **Stories :** 14/14

### Ce qui a été implémenté
- 3 repositories SQLite : `triggers_def.db`, `pipelines_def.db`, `notifications.db`
- `TriggerDefinitionRepository`, `PipelineDefinitionRepository`, `NotificationConfigRepository` — tous avec validation avant écriture
- 9 REST CRUD endpoints (triggers + pipelines + notifications)
- 11 Tauri commands CRUD
- Svelte éditeurs : `CreateTriggerDialog`, `EditTriggerDialog`, `CreatePipelineDialog`, `EditPipelineDialog`, `CreateChannelDialog`, `EditChannelDialog`, `GlobalEventsEditor`
- Migration de `[[triggers]]`, `[[pipelines]]`, `[notifications]` hors de `apollia.toml` → SQLite only
- Pattern : API handler → SQLite → `Engine.reload()` (hot reload)
- Settings vue nettoyée : sections opérationnelles retirées, bandeau info vers vues dédiées

### Primitives agent disponibles
Aucune — configuration opérateur uniquement.

### Écarts
- **ADR-033** : Séparation structurel (TOML reste pour `[runtime]`, `[memory]`, `[[llm.backends]]`) / opérationnel (SQLite pour triggers, pipelines, notifications). Choix Option A validé.
- `Arc<Mutex<Repository>>` dans `AppState` (mutations rares, opérateur humain — exception acceptée au pattern acteur)

---

## Sprint 18 — Chat hybride

**Statut :** LIVRÉ ✅ | **Stories :** 12/12

### Ce qui a été implémenté
- `ChatSessionManager` acteur Tokio : sessions persistées SQLite, historique complet
- 2 modes : **Chat Libre** (LLM + outils natifs, boucle ReAct Rust) et **Chat Agent** (agent Python installé)
- Streaming token-by-token via SSE (`ChatToken` events)
- HITL inline : Accept / Refuse / Always Accept pour les tool calls
- 7 API REST endpoints chat + 12 nouveaux `RuntimeEvent` chat
- Desktop : liste sessions, `NewChatDialog` (mode Libre/Agent), conversation streaming, tool call visualization
- Bouton Chat sur cartes agent + entrée sidebar

### Primitives agent disponibles
- Mode Chat Agent : l'agent Python reçoit les messages utilisateur via `run()` dans une session conversationnelle persistée

### Écarts
- **ADR-034** : Chat hybride sessions + streaming + HITL inline (conforme à la spec)
- Pas d'écart majeur — 12/12 livré conforme

---

## Sprint 20 — Système Agentique Amélioré

**Statut :** LIVRÉ ✅ | **Stories :** 18/18

### Ce qui a été implémenté
- `ToolRegistryHandle::describe(name)` → `ToolDescriptor` JSON schema
- `PlanStep.model_hint` + routing multi-modèle par step dans `ActorLoop`
- Per-step observation : `StepContext` + injection observation + memory update épisodique après chaque step
- `PlanCacheRepository` SQLite + calcul cache key → skip re-planification si plan identique
- Scoring pondéré `Observer` (remplace seuils heuristiques)
- `AgentMailbox` acteur Tokio : communication agent-to-agent
- Desktop : Tool Schema Viewer, Timeline enrichie, Plan Cache Dashboard, Agent Messages Panel

### Primitives agent disponibles
- **`ctx.tools.describe(name)`** → description JSON schema d'un outil
- **`ctx.send(agent_name, message)`** → envoyer un message à un autre agent
- **`ctx.receive()`** → recevoir les messages adressés à cet agent
- `model_hint` dans `PlanStep` → routing vers un backend LLM spécifique par step (mode orchestré)

### Écarts
- **ADR-035** : Per-step observation en mode Orchestré (non planifié initialement — enrichissement)
- **ADR-036** : Cache de plans (optimisation performance, non prévu dans les specs initiales)

---

## Sprint 21 — apollia-sdk : Bibliothèque Python

**Statut :** LIVRÉ ✅ | **Stories :** 15/15 | **Package :** `sdk/apollia/`

### Ce qui a été implémenté
- Package `apollia` pip-installable (`sdk/` avec `pyproject.toml`)
- Base classes : `ReactAgent`, `ConversationalAgent`, `OrchestratedAgent`
- Type stubs : `RuntimeContext`, `ToolProxy`, `LlmProxy`, `MemoryInterface`
- Parsing utilities : JSON, code blocks, XML
- Output formatting utilities
- Testing utilities : `MockContext`, `MockToolProxy`, `MockLlmProxy`, `MockMemory`, assertion helpers
- Scaffolding : `apollia new <name> --type react|conversational|orchestrated`
- Agent sample : `sdk-demo-agent`
- CLI intégrée : `apollia-os agent new`
- Desktop : dialog "Create from Template"

### Primitives agent disponibles
- **`from apollia.agents import ReactAgent`** → base class avec boucle ReAct
- **`from apollia.agents import ConversationalAgent`** → base class conversationnelle (override `converse()`)
- **`from apollia.agents import OrchestratedAgent`** → base class orchestrée
- **`from apollia.agents import AIPResult`** → résultat d'exécution (`completed()`, `failed()`, `input_required()`)
- **`from apollia.testing import MockContext`** → test d'agents sans runtime
- **`from apollia.parsing import extract_json, extract_code_block`** → utilitaires parsing LLM
- **`apollia new <name>`** → scaffolding d'un nouvel agent

### Écarts
- **ADR-037** : Packaging Python SDK (conforme — distribution via pip install local, pas PyPI pour l'instant)

---

## Sprint 22 — Chat Intelligent + Mémoire Utilisateur Globale

**Statut :** LIVRÉ ✅ (d'après implémentation constatée)

### Ce qui a été implémenté
- `UserMemoryRepository` SQLite (`user_memory.db`) sous namespace `__user__`
- 3 catégories : `preferences`, `habits`, `context` + 4 sources : `onboarding`, `chat_inference`, `user_explicit`, `agent_observation`
- Scores de confiance (0.0–1.0) : arbitrage des mises à jour
- API REST : `GET/PUT /api/v1/user/profile`, `GET /api/v1/user/memory`, `DELETE /api/v1/user/memory/:key`
- Injection dans le chat : `BuiltInChatAgent.build_system_prompt()` injecte `## User Context`
- Extraction LLM post-session : `extract_user_memory()` fire-and-forget (timeout 30s, min 4 messages)
- `UserMemoryExtractor` stateful : enrichissement passif (cooldown 1h, déduplication, respect confiance)
- `ConversationSummarizer` : résumé LLM (max 500 tokens) pour contexte window
- Cross-session recall : FTS5 sur résumés de sessions passées (max 3 résultats)
- `ctx.user_context` dans `RuntimeContext` Python (mode chat uniquement)
- Desktop : User Memory Dashboard, contexte injecté visible dans le chat

### Primitives agent disponibles
- **`ctx.user_context`** → `dict[str, list[tuple[str, str]]] | None` — contexte utilisateur (préférences, habitudes, contexte) en mode chat
- **`ctx.memory.remember(key, value, source, confidence)`** → persistance avec score de confiance

### Écarts
- **ADR-038** : Mémoire utilisateur globale (conforme)
- **ADR-039** : Conversation memory management — summarization + sliding window (conforme)

---

## Sprint 23 — Onboarding Utilisateur

**Statut :** EN COURS | **Stories :** 14 planifiées

### Ce qui a été implémenté
- **ADR-040** : Onboarding comme agent conversationnel (pas wizard déterministe)
- Agent `onboarding-agent` (`ConversationalAgent`) : 5 domaines (identity, preferences, tools, domain, agents), system prompt bilingue FR/EN, détection de langue automatique
- 17 clés mémoire préfixées `user.*` + scores de confiance (`REMEMBER` = 0.9, `INFER` = 0.5)
- Tags `[REMEMBER key=value]` / `[INFER key=value]` extraits du LLM puis retirés avant affichage
- Détection premier lancement → `OnboardingRequired` event
- CLI : `apollia-os onboard [--topic <topic>]` (5 topics valides)
- Desktop : écran d'accueil, `OnboardingConversation`, `TopicProgressBar` (polling 4s)
- Tauri IPC : `get_onboarding_status()`, `trigger_onboarding()`, `dismiss_onboarding()`
- Enrichissement passif continu + Feedback loop UI (valider/corriger/supprimer)

### Primitives agent disponibles
- Le `onboarding-agent` utilise les mêmes primitives que tout agent SDK : `ctx.llm.complete()`, `ctx.memory.remember()`, `ConversationalAgent.converse()`
- Pas de nouvelle primitive — démonstration que le SDK existant suffit pour l'onboarding

### Écarts
- Sprint-index marque le sprint 🔲 (À planifier) alors que l'implémentation est avancée
- Sprint 22 aussi marqué 🔲 dans l'index alors que le code existe (index non synchronisé)

---

## Sprint 24 — apollia-stt : moteur STT embarqué

**Statut :** LIVRÉ ✅ | **Stories :** 17/17 | **ADR :** ADR-041

### Ce qui a été implémenté
- Nouvelle crate `apollia-stt` avec trait `SttBackend` (object-safe, Send+Sync)
- Backend `WhisperCppBackend` via `whisper-rs` 0.16 (compilation statique, Metal natif)
- Pipeline audio : capture microphone, resample 16kHz, silence trim
- `SttRepository` SQLite pour l'historique des transcriptions
- `SttEngine` acteur Tokio (Phase 12 Supervisor, conditionnel `stt.enabled`)
- 5 `RuntimeEvent` STT + 5 endpoints REST (`/api/v1/stt/*`)
- Desktop : `HotkeyListener` (Ctrl+Shift+Space), `ClipboardManager`, overlay d'enregistrement
- 5 commandes Tauri IPC + Vue Transcriptions + Settings STT
- CLI `apollia-os stt transcribe/status/models` + téléchargement modèle

### Primitives agent disponibles
Pas de nouvelle primitive Python — STT est une feature desktop/CLI, pas une API agent.

---

## Sprint 25 — Surface outil complète : outils atomiques + HTTP + mémoire

**Statut :** LIVRÉ ✅ | **Stories :** 22/22 | **ADR :** ADR-043

### Ce qui a été implémenté
- Décomposition `file_io` en 4 outils atomiques : `file_read`, `file_write`, `file_edit`, `file_list`
- 2 outils de recherche : `file_glob`, `file_grep`
- `http_fetch` (requêtes HTTP GET/POST)
- `memory_search` (recherche FTS5+BM25 depuis un outil)
- Dépréciation `file_io` (warning, code conservé)
- Affichage mode-aware (operator/builder) dans le chat desktop

### Primitives agent disponibles
- `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep` (6 outils atomiques)
- `http_fetch` (requêtes HTTP)
- `memory_search` (recherche mémoire depuis un outil)

---

## Sprint 26 — Client MCP : intégration universelle des outils externes

**Statut :** LIVRÉ ✅ | **Stories :** 19/19 | **ADR :** ADR-044

### Ce qui a été implémenté
- Nouvelle crate `apollia-mcp` : implémentation native JSON-RPC 2.0 + MCP
- Transport stdio (spawn subprocess + stdin/stdout async)
- Configuration via `~/.apollia/mcp.toml` (secrets interpolés env vars)
- `McpClientManager` acteur Tokio + `McpSession` (handshake, tools/list, tools/call)
- Naming `mcp:{server}/{tool}` dans le ToolRegistry
- HITL à deux niveaux : serveur (`requires_approval`) et agent (`tools_requiring_approval`)
- Lazy start des sous-processus serveurs
- API REST `/mcp/*` + CLI `apollia-os mcp list/status/restart`

### Primitives agent disponibles
- `ctx.tools.call("mcp:notion/search", ...)` — tout outil MCP accessible via le même pattern

---

## Sprint 28 — Configuration Runtime Unifiée : SQLite-first

**Statut :** LIVRÉ ✅ | **Stories :** 12/12 | **ADR :** ADR-047

### Ce qui a été implémenté
- `LlmBackendRepository` SQLite + `AgentManifest.llm_backend` optionnel
- `LlmRouter` multi-backend avec routing `agent_id → backend_name`
- API REST `/api/v1/llm/backends` CRUD
- STT config → SQLite + API REST `/api/v1/stt/config`
- `McpServerRepository` SQLite (migration TOML → DB)
- Suppression `[agents] startup` et `reload_triggers` de `apollia.toml`
- CLI `memory list` + `memory clear`

### Primitives agent disponibles
- `manifest()["llm_backend"]` — binding agent → backend LLM spécifique

---

## Sprint 29 — Worker Agents V1 : excel-worker + csv-data-worker

**Statut :** LIVRÉ ✅ | **Stories :** 6/7 (STORY-392 différée Sprint 30) | **ADR :** ADR-048

### Ce qui a été implémenté
- Pattern Worker Agent : `WorkerAgent(BaseReActAgent)` dans le SDK Python
- `AgentManifest.packages` + `setup_venv` au `INITIALIZING`
- `excel-worker` : manipulation Excel via openpyxl (guardrail : jamais bash sur .xlsx)
- `csv-data-worker` : analyse CSV via pandas (guardrail : détection encodage + dtypes)
- Tests + benchmark Worker Agent vs generic-agent sur Llama 13B

### Primitives agent disponibles
- `WorkerAgent` base class SDK (héritage `BaseReActAgent`)
- `manifest()["packages"]` — déclaration dépendances pip
- `manifest()["supports_a2a"]` + `manifest()["skills"]` — déclaration skills A2A

---

## Sprint 30 — A2A Routing V1 + Benchmark Worker Agent Pattern

**Statut :** LIVRÉ ✅ | **Stories :** 9/9 | **ADR :** ADR-049

### Ce qui a été implémenté
- `SkillIndex` dans `AgentRegistry` : index inversé `skill_id → agent_name`
- `A2AInvoker` : invocation inter-agents par `skill_id` (timeout 120s configurable)
- Trust model A2A : user memory read-only pour les Workers invoqués
- `RuntimeContextConfig { user_memory_read_only: bool }`
- Endpoint REST `GET /api/v1/a2a/agents` + `/.well-known/agent.json`
- CLI `agent list --supports-a2a`
- Matrice de décision wiki (Worker vs MCP vs Pipeline)

### Primitives agent disponibles
- `ctx.delegate(skill_id, payload, timeout_secs=120)` — délégation A2A
- `ctx.a2a_invoke(skill_id, payload)` — alias d'invocation

---

## Sprint 31 — Worker Agents V2 : pdf-worker + code-worker + A2A chat libre

**Statut :** LIVRÉ ✅ | **Stories :** 6/6 | **ADR :** —

### Ce qui a été implémenté
- `pdf-worker` : extraction texte/tableaux PDF via pdfplumber (guardrail : chunking > 50 pages)
- `code-worker` : génération/refactoring/revue code Python+Rust (guardrail : file_read avant file_write)
- `CompositeToolInvoker` : A2A intégré dans le chat libre (`BuiltInChatAgent`)
- Template `apollia new --type worker` : scaffolding Worker Agent complet
- Documentation Worker Agent Pattern builders

### Primitives agent disponibles
- 4 Worker Agents built-in opérationnels (`excel-worker`, `csv-data-worker`, `pdf-worker`, `code-worker`)

---

## Sprint 32 — A2A complet + Distribution locale + Worker Agents communautaires

**Statut :** LIVRÉ ✅ | **Stories :** 8/8 | **ADR :** ADR-050

### Ce qui a été implémenté
- ADR-050 : stratégie distribution bundled vs communautaire formalisée
- `sql-worker` : interrogation SQLite (guardrail : SELECT-only, paramétrage `?` anti-injection)
- `git-worker` : opérations Git (guardrail : bloque push --force, reset --hard, etc.)
- `agents/bundled/manifest.json` + auto-installation au premier boot (4 agents)
- `agents/community/` : structure + README + sql-worker + git-worker
- `A2AConfig` : `max_depth`, `invocation_timeout_secs`, `chain_timeout_secs`
- 3 garde-fous A2A runtime : `MaxDepthExceeded`, `SelfInvocation`, `ChainTimeoutExceeded`
- `RuntimeEvent::A2AGuardTriggered` émis sur EventBus
- `A2AToolsProvider` : injection dynamique des skills A2A comme outils virtuels `a2a:{skill_id}` dans ORIA
- `apollia-os agent install <path> [--skip-tests]` avec validation communautaire
- Tests E2E distribution + A2A guards

### Primitives agent disponibles
- 6 Worker Agents total (4 bundled + 2 communautaires)
- `ctx.tools.call("a2a:read-excel", ...)` — invocation A2A transparente via outil ORIA
- Garde-fous A2A appliqués par le runtime (non contournables depuis Python)

---

## Sprint 33 — Onboarding interactif multi-phases

**Statut :** LIVRÉ ✅ | **Stories :** 13/13

### Ce qui a été implémenté
- i18n complète des 8 composants onboarding/companion (zéro string FR/EN hardcodée)
- ~70 clés traduction `onboarding_v2.*` et `companion.*` dans `fr.json` + `en.json`
- 2 pages wiki créées : `Onboarding-System.md`, `Onboarding-Tour-Steps.md`
- `Briques-Desktop.md` mis à jour (sections 5-6)
- 2 chapitres book : `first-launch.md`, `onboarding-tour.md`

---

## Sprint 34 — Beta Hardening: Technical Debt, Security & Robustness

**Statut :** LIVRÉ ✅ | **Stories :** 24/25 (1 🚫 Windows sandbox reporté) | **ADRs :** ADR-051→055

### Ce qui a été implémenté
- Auth API REST TCP `:7771` (token + loopback) — ADR-051
- HMAC-SHA256 sur webhooks sortants
- `cargo audit` + `cargo deny` en CI
- 23 constantes hardcodées → `apollia.toml`
- Pipeline fan-out (ADR-053) + step timeout + cancellation + HITL audit
- Pricing LLM robuste (table lookup)
- 3 agents communautaires : browser-worker, email-worker, slack-worker
- Registry communautaire distant (Git-based) — `apollia agent install <git-url>` — ADR-055
- Tests E2E Tauri automatisés (5 tests)
- apollia-stt tests renforcés (22 → ~40)
- CUDA compile check CI
- 🚫 STORY-451 (Windows sandbox) reporté post-v1

### Primitives agent disponibles
- 9 Worker Agents total (6 précédents + browser + email + slack)
- `apollia-os agent install <git-url>` — installation depuis un repo Git

---

## Sprint 35 — Workspace Intelligence & Execution Performance

**Statut :** LIVRÉ ✅ | **Stories :** 13/13 | **ADRs :** ADR-056→060

### Ce qui a été implémenté
- Prompt caching `cache_control: ephemeral` — ADR-057 (-80% coût LLM sessions longues)
- `TokenBudget` accumulé + affichage CLI + TTFT
- `truncate_middle()` pour output > 30KB
- Outils read-only concurrents (`is_read_only` + `execute_batch()`) — ADR-059
- Retry exponentiel partagé tous backends + `CancellationToken` abort
- Nouvelle crate `apollia-workspace` — ADR-056 : WorkspaceAssembler, GitContextCollector, ApolliamdFinder
- Injection APOLLIA.md dans ORIA + AIP bridge (`ctx.workspace`)
- `StyleDetector` — détection conventions de code
- Auto-compact fenêtre de contexte — ADR-058 (seuil 80%, résumé LLM)
- Session recovery — `apollia chat --resume <id>` après Ctrl+C
- `persistent_bash` — shell persistant entre steps (état CWD conservé)
- Sidechain logging A2A
- `ContextProvider` trait — ADR-060

### Primitives agent disponibles
- `ctx.workspace` — contexte projet injecté (APOLLIA.md, git, style)
- `apollia workspace status` — branche git + APOLLIA.md path
- Shell persistant (`cd /tmp` conservé entre steps)

---

## Sprint 36 — Permissions, MCP Server & Intelligence UX

**Statut :** LIVRÉ ✅ | **Stories :** 16/16 | **ADRs :** ADR-061→063

### Ce qui a été implémenté
- `apollia-permissions` : moteur 3 couches (SafeList + PrefixRuleEngine + InjectionDetector) — ADR-061
- Validation syntaxe bash + `BANNED_COMMANDS` + AST parser (remplace regex)
- MCP server mode — Apollia client ET serveur (stdio, 9 outils + `submit_task`) — ADR-062
- Routing LLM par niveau de précision (Precise/Fast/Embedding)
- Extraction file paths post-bash (non-bloquant, `FilePathExtractor`)
- Binary feedback — deux plans alternatifs (RLHF SQLite logging) — ADR-063
- 6 types UI HITL desktop (Bash, FileEdit, FileWrite, Filesystem, MCP, Generic + PermissionDispatcher)
- Widget coût LLM temps réel desktop (`TokenBudgetWidget`)
- Notification inactivité + canal terminal iTerm2/GNOME
- `apollia workspace status/init`
- `FileTimestampCache` — invalidation mémoire fichiers modifiés
- Alerte seuil coût LLM (notifications desktop et OS)
- `--allowedTools` / `--disallowedTools` + REPL history
- Conversation forking (`ChatSession::fork`, `/fork` REPL)
- Slash commands depuis `APOLLIA_COMMANDS` (`CommandRegistry`)

### Primitives agent disponibles
- Permissions 3 couches appliquées par le runtime
- `/fork` — forking de conversation
- Slash commands custom depuis `APOLLIA_COMMANDS/`

---

## Sprint 37 — Parité complète TypeScript

**Statut :** LIVRÉ ✅ | **Stories :** 15/15 | **ADRs :** ADR-064→068

### Ce qui a été implémenté
- `apollia-auth` : OAuth2 PKCE complet (keyring, providers, callback localhost) — ADR-064
- Auto-updater `apollia update` (GitHub Releases, SHA256, atomic replace) — ADR-065
- Code review agent `apollia-review` (Python AIP, route REST, CLI)
- MCP discovery mDNS (`_apollia-mcp._tcp.local.`)
- Hot reload serveurs MCP (disconnect → update config → reconnect)
- HITL MCP finalisé (`apollia mcp set-approval`, SQLite persistence, TTL)
- `apollia memory export/import` (JSON gzip, merge/replace) — ADR-066
- Purge configurable par type mémoire (episodic/semantic/procedural, auto_purge)
- `OnBusyPolicy::Queue` pour triggers (file bornée, TriggerQueueFull event)
- Filtrage notifications par sévérité par canal (Debug→Critical, min_severity)
- Templates pipeline communautaires + `apollia pipeline install` (registry Git, LlmPrompt step)
- `apollia-stt` CUDA compile check CI (feature matrix cpu/metal/cuda)
- AWS Bedrock backend (SigV4, credentials chain) — ADR-067
- Google Vertex AI backend (ADC OAuth2, token cache) — ADR-068
- Notebook tool — lecture et édition Jupyter `.ipynb` (NotebookRead + NotebookEdit)

### Primitives agent disponibles
- `apollia auth login <provider>` — OAuth2 PKCE
- `apollia update` — auto-updater SHA256
- `apollia mcp list --discover` — mDNS discovery
- Vertex AI + Bedrock comme backends LLM enterprise
- `NotebookRead` + `NotebookEdit` pour agents data science

---

## Sprint 38 — Autonomie filesystem

**Statut :** LIVRÉ ✅ | **Stories :** 5/5 | **ADR :** ADR-069

### Ce qui a été implémenté
- Refactor `NativeChatToolInvoker` : workspace_path par session
- Extension `RiskClassifier` aux opérations filesystem (4 niveaux de risque)
- Journal réversible filesystem + CLI `apollia rollback` — ADR-069
- UI HITL filesystem — modal diff/preview pour opérations sensibles
- File picker natif pour création de projet

### Primitives agent disponibles
- Agents autonomes sur le filesystem, régulés par friction graduée HITL
- `apollia rollback <session-id>` — restauration post-hoc du disque

---

## Sprint 39 — Agents qui travaillent : Restructuration & Premiers Assistants Réels

**Statut :** LIVRÉ ✅ | **Stories :** 7/7 | **ADR :** ADR-070

### Ce qui a été implémenté
- Memory namespace project-scoped (`project_id:namespace`) — ADR-070
- Restructuration `agents/` (workers/ assistants/ system/ examples/)
- `spec-assistant` — assistant conception et specs (TaskSpec, project rules, mémoire)
- `dev-assistant` — assistant implémentation (pre-task contract, A2A, guardrails)
- `review-assistant` — assistant vérification (complétude, conformance, tests)
- `document-assistant` — traitement documents tous profils (routing A2A vers workers)
- Smoke tests des 4 assistants

### Primitives agent disponibles
- 4 assistants opérationnels installables et démo-ables
- Pipeline dev complet : spec → implémentation → vérification
- Isolation mémoire par projet (transparent pour le code Python)

---

## Sprint 40 — Context Bootstrapping & SDK 0.3.0

**Statut :** LIVRÉ ✅ | **Stories :** 6/6 | **ADRs :** ADR-070, ADR-071

### Ce qui a été implémenté
- `recall_entry()` + `recall_all()` exposés en Python (métadonnées complètes)
- SDK 0.3.0 : `AgentManifestDict` v2 (4 champs AIP), `ConversationalAgent` stub importable sans runtime
- `ContextBootstrap` : protocole SDK (classe abstraite, 2 méthodes) — ADR-071
- `ProjectContextBootstrap` : base partagée agents dev (commit hash, workspace rules, tech stack)
- Adoption bootstrap dans les 4 assistants (spec/dev/review/document)
- Tests d'intégration bootstrap + smoke tests mis à jour

### Primitives agent disponibles
- `ctx.memory.recall_entry(key)` — métadonnées complètes d'une entrée sémantique
- `ctx.memory.recall_all(limit=N)` — lister toutes les entrées du namespace
- `ContextBootstrap` — persistance cross-session du contexte projet
- `from apollia import ConversationalAgent` — importable sans runtime Rust

---

## Sprints non livrés

### Sprint 16 — MVP Demo-Ready UI/UX bimodale
**Statut :** 🔲 À planifier | **Stories :** 27 planifiées
Objectif : UI bimodale (Builder + Opérateur), agent install/persistence SQLite, dark mode, i18n, glassmorphism.

### Sprint 19 — Refonte UI/UX 8 pages restantes
**Statut :** 🔲 À faire | **Stories :** 14 planifiées
Objectif : Aligner les 8 pages desktop sur le Design System "Warm Glass".

---

## Récapitulatif — Primitives agent par sprint

| Sprint | Primitives ajoutées |
|---|---|
| 0-1 | *(infrastructure uniquement)* |
| 2 | `bash_executor`, `python_executor`, `file_io` (outils natifs) |
| 3 | `ctx.memory.*` (record, remember, recall, search, forget) |
| 4 | `ctx.tools.call()`, `ctx.tools.list_tools()`, `ctx.memory.*` via Python |
| 8 | **`ctx.llm.*`** (chat, complete, stream, run_tools) |
| 10 | `execution_mode = "orchestrated"`, `on_plan_complete()`, `system_prompt` |
| 11 | `AIPResult.input_required()`, `task.is_resumed`, `tools_requiring_approval` |
| 20 | `ctx.tools.describe()`, `ctx.send()`, `ctx.receive()`, `model_hint` |
| 21 | SDK classes (`ReactAgent`, `ConversationalAgent`, `OrchestratedAgent`), `MockContext`, `apollia new` |
| 22 | `ctx.user_context`, `ctx.memory.remember(..., confidence=)` |
| 25 | `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep`, `http_fetch`, `memory_search` |
| 26 | `ctx.tools.call("mcp:{server}/{tool}", ...)` — outils MCP externes |
| 28 | `manifest()["llm_backend"]` — binding agent → backend LLM |
| 29 | `WorkerAgent` base class, `manifest()["packages"]`, `manifest()["supports_a2a"]` |
| 30 | `ctx.delegate(skill_id, payload)`, `ctx.a2a_invoke(skill_id, payload)` |
| 32 | `ctx.tools.call("a2a:{skill_id}", ...)` — invocation A2A via outils ORIA |

---

## Récapitulatif — ADRs non planifiés

| ADR | Sprint | Cause |
|---|---|---|
| ADR-011 | 1 | AgentId/TaskId String aliases (pragmatisme vs newtypes) |
| ADR-012 | 2 | macOS sandbox-exec deprecated → mode Dev compile-time |
| ADR-013 | 4 | PYO3_PYTHON config macOS (friction dev) |
| ADR-014 | 4 | spawn_blocking + asyncio.run (event loop init trop complexe) |
| ADR-015 | 4 | Trait ToolExecutor (testabilité — réutilisé 3 sprints) |
| ADR-016 | 4 | Trait AgentRunner (même pattern que ADR-015) |
| ADR-017 | 5 | hyper-util Unix socket (limitation axum 0.7) |
| ADR-018 | 5 | CLI bootstrap sans Supervisor (simplifié) |
| ADR-019 | 6 | AgentLoader trait (résolution DT-031) |
| ADR-035 | 20 | Per-step observation (enrichissement non prévu) |
| ADR-036 | 20 | Cache de plans (optimisation non prévue) |

---

## Dettes techniques ouvertes

| ID | Sévérité | Description | Depuis |
|---|---|---|---|
| DT-010 | Moyenne | cgroups hardening incomplet | Sprint 2 |
| DT-011 | Moyenne | mount namespace tmpfs non implémenté | Sprint 2 |
| DT-023 | Moyenne | `Arc<Mutex<MemoryManager>>` viole pattern acteur | Sprint 4 |
| DT-030 | Basse | `main.rs` apollia-cli monolithique (539+ loc) | Sprint 5 |
| DT-034 | Basse | Socket path Unix hardcodé | Sprint 5 |
| DT-035 | Moyenne | Tests `python-tests` pas en CI Linux | Sprint 6 |
| DT-037 | Basse | `apollia-cli` couplé à `apollia-aip` | Sprint 6 |
