# Application Desktop — Tauri v2 + Runtime embarque

> *L'application desktop Apollia OS embarque le runtime complet dans un processus unique. Double-clic → fenetre → 10 vues temps reel couvrant 100% des capacites CLI.*

---

## 1. Architecture — Processus unique (ADR-027)

L'application desktop est une crate Tauri v2 (`apollia-desktop`) qui demarre le runtime Apollia en interne via `init_embedded`. Un seul binaire distribue a la fois le runtime Rust et le frontend Svelte.

```
main() Tauri
  │
  ├── init_embedded(config) → RuntimeHandle
  │     └── thread "apollia-runtime"
  │           └── Supervisor.start() → AllReady
  │
  ├── setup_tray() → SystemTray (icone + menu contextuel)
  │
  └── tauri::Builder
        ├── .manage(RuntimeHandle)     ← etat partage
        ├── .invoke_handler(commands)  ← commandes IPC
        ├── .plugin(dialog)            ← file picker natif
        ├── .plugin(notification)      ← notifications natives
        ├── .plugin(updater)           ← mises a jour in-app (tauri-plugin-updater)
        └── .run()                     ← ouvre la WebView
```

**Communication frontend ↔ runtime :**

| Type | Mecanisme | Exemples |
|---|---|---|
| Mutations ponctuelles | Commandes Tauri `#[tauri::command]` | `start_agent`, `submit_task`, `fire_trigger` |
| Flux temps reel | SSE EventBus (`localhost:7771/api/v1/dashboard/stream`) | Agents, tasks, LLM, triggers, pipelines, approvals |

Le CLI reste fonctionnel via le socket Unix existant (`/tmp/apollia.sock`).

---

## 2. Crate `apollia-desktop`

### 2.1 Structure

```
crates/apollia-desktop/
├── Cargo.toml
├── tauri.conf.json            ← configuration Tauri v2
├── build.rs                   ← tauri_build::build()
├── capabilities/default.json  ← permissions Tauri v2
├── icons/logov2.png
├── src/
│   ├── main.rs                ← entree Tauri + init_embedded() + tray
│   ├── tray.rs                ← system tray (menu, tooltip, tray-update listener)
│   └── commands/
│       ├── mod.rs             ← helpers HTTP (http_get_json, http_post_json)
│       ├── agents.rs          ← list_agents, start_agent, stop_agent
│       ├── tasks.rs           ← list_tasks, submit_task, get_task_timeline
│       ├── hitl.rs            ← list_pending_approvals, list_resolved_approvals, resume_task
│       ├── llm.rs             ← list_llm_backends, ping_llm_backend, get_llm_cost_stats
│       ├── triggers.rs        ← list_triggers, set_trigger_enabled, fire_trigger, get_trigger_logs, reload_triggers
│       ├── pipelines.rs       ← list_pipelines, list_pipeline_runs, list_all_pipeline_runs, run_pipeline, get_pipeline_run_detail
│       ├── memory.rs          ← list_memory_namespaces, list_memory_entries, search_memory, delete_memory_entry
│       ├── notifications.rs   ← list_notification_channels, test_notification_channel, get_notification_logs
│       ├── tools.rs           ← list_tools, describe_tool
│       ├── tool_governance.rs ← governance_list_tools, governance_set_tool_enabled, governance_get/set_tool_config, governance_*_credential, governance_list_permission_rules, governance_revoke_permission_rule, governance_revoke_all_rules, governance_list_audit
│       ├── observability.rs   ← get_global_timeline, get_tool_audit_trail, get_llm_daily_costs, get_plan_cache_stats, clear_plan_cache
│       ├── config.rs          ← get_config, open_config_in_editor
│       ├── onboarding.rs      ← check_onboarded, mark_onboarded, reset_onboarding, check_python, check_llm_configured, check_hello_agent_exists
│       └── stt.rs             ← get_stt_status, list_transcriptions, delete_transcription, transcribe_file, list_stt_models
│   ├── stt/                   ← module STT desktop
│   │   ├── flow.rs            ← SttFlow (hotkey → capture → transcribe → clipboard)
│   │   ├── hotkey.rs          ← HotkeyListener (tauri-plugin-global-shortcut)
│   │   ├── clipboard.rs       ← ClipboardManager (arboard + enigo)
│   │   └── overlay.rs         ← RecordingOverlay (fenêtre Tauri secondaire)
└── ui/                        ← application Svelte 5
    ├── package.json
    ├── vite.config.ts
    └── src/
        ├── App.svelte
        ├── lib/
        │   ├── types.ts       ← 45+ interfaces TypeScript (dont SttStatus, TranscriptRow, SttModelInfo)
        │   ├── stores/
        │   │   ├── sse.ts             ← SSE connection + 7 stores reactifs + 4 derives
        │   │   ├── navigation.ts      ← currentRoute + showOnboarding
        │   │   ├── settings.ts        ← SettingsSubRoute (12 valeurs) + SETTINGS_SUB_ROUTES
        │   │   ├── toolGovernance.ts  ← ToolStatusDto, CredentialEntryDto, CredentialTestResultDto + 7 fonctions IPC (loadTools, toggleTool, getToolConfig, updateToolConfig, setCredential, deleteCredential, testCredential)
        │   │   └── permissions.ts     ← PermissionRuleDto, AuditEntryDto, PermissionRuleFilter + 7 fonctions IPC (loadRules, revokeRule, revokeAll, countRulesForScope, loadAudit, setScopeFilter, setToolFilter)
        │   └── components/ui/     ← Button, Card, Badge, Sheet, Separator (bits-ui)
        ├── components/
        │   ├── layout/        ← Sidebar.svelte, Main.svelte
        │   ├── agents/        ← AgentCard.svelte, AgentLogs.svelte, AgentDetail.svelte, AgentMessagesPanel.svelte, CreateFromTemplateDialog.svelte
        │   ├── tasks/         ← TaskList.svelte, TaskDetail.svelte, TaskTimeline.svelte
        │   ├── hitl/          ← ApprovalCard.svelte, ApprovalHistory.svelte
        │   ├── llm/           ← LlmBackendCard.svelte, LlmStats.svelte
        │   ├── triggers/      ← TriggerRow, TriggerLogs, CreateTriggerDialog, EditTriggerDialog
        │   ├── pipelines/     ← PipelineRunCard, PipelineRunDetail, PipelineDefinitionCard, CreatePipelineDialog, EditPipelineDialog
        │   ├── memory/        ← NamespaceSelector.svelte, MemorySearch.svelte, MemoryTable.svelte, ToolSchemaPanel.svelte
        │   ├── notifications/ ← NotificationChannelCard, NotificationLog, CreateChannelDialog, EditChannelDialog, GlobalEventsEditor
        │   ├── observability/ ← TimelineGlobal.svelte, LlmCostChart.svelte, AuditTrailTable.svelte, PlanCacheStats.svelte
        │   ├── settings/      ← SettingsNav.svelte, ToolCard.svelte, ToolConfigDrawer.svelte, CredentialField.svelte, PermissionRuleCard.svelte
        │   ├── stt/           ← TranscriptCard.svelte, TranscribeFileDialog.svelte, RecordingOverlay.svelte
        │   └── onboarding/    ← StepEnvironment.svelte, StepFirstAgent.svelte, StepFirstTask.svelte
        └── routes/            ← 15 fichiers .svelte (un par route : Agents, Tasks, Approvals, Chat, Transcriptions, Integrations, Llm, Triggers, Pipelines, Memory, Notifications, Observability, Settings, Dashboard, Onboarding)
```

### 2.2 `RuntimeHandle` (apollia-runtime)

```rust
pub struct RuntimeHandle {
    pub event_sender: EventBusSender,
    pub registry_handle: AgentRegistryHandle,
    pub tool_registry_handle: ToolRegistryHandle,
    pub router_handle: TaskRouterHandle<DynBackend>,
    pub api_handle: APIServerHandle,
    pub api_port: u16,
    pub llm_router: Option<Arc<LlmRouter>>,
    pub trigger_engine: Option<TriggerEngineHandle>,
    pub pipeline_engine: Option<PipelineEngineHandle>,
    pub audit_trail: Option<AuditTrailHandle>,
    pub task_repository: Option<Arc<TaskRepository>>,
    pub pending_approvals: Option<Arc<Mutex<PendingApprovals>>>,
    pub notification_engine: Option<NotificationEngineHandle>,
}
```

### 2.3 `EmbeddedConfig`

```rust
pub struct EmbeddedConfig {
    pub tcp_port: u16,               // defaut: 7771
    pub socket_path: PathBuf,        // defaut: /tmp/apollia.sock
    pub data_dir: PathBuf,           // defaut: ~/.apollia/
    pub startup_timeout_secs: u64,   // defaut: 30
}
```

### 2.4 `EmbeddedError`

```rust
pub enum EmbeddedError {
    SupervisorFailed(SupervisorError),
    StartupTimeout(u64),
    RuntimeThreadPanicked,
}
```

---

## 3. Commandes Tauri IPC

Commandes exposees au frontend Svelte via `#[tauri::command]` (source de vérité : `invoke_handler` dans `src/main.rs`) :

### Agents (6)

| Commande | Parametres | Retour |
|---|---|---|
| `list_agents` | — | `Vec<AgentInfo>` |
| `start_agent` | `path: String` | `Result<String, String>` (agent_id) |
| `stop_agent` | `agent_id: String` | `Result<, String>` |
| `create_agent_from_template` | `name: String, template_type: String` | `Result<CreateAgentResult, String>` |
| `check_sdk_available` | — | `Result<bool, String>` |
| `check_agent_name_available` | `name: String` | `Result<bool, String>` |

### Tasks (3)

| Commande | Parametres | Retour |
|---|---|---|
| `list_tasks` | `filter: Option<TaskFilter>` | `Result<Vec<TaskSummary>, String>` |
| `submit_task` | `agent_id: String, input: String` | `Result<String, String>` (task_id) |
| `get_task_timeline` | `task_id: String` | `Result<Vec<Value>, String>` |

### HITL (3)

| Commande | Parametres | Retour |
|---|---|---|
| `list_pending_approvals` | — | `Result<Vec<PendingApproval>, String>` |
| `list_resolved_approvals` | `limit: Option<usize>, days: Option<u64>` | `Result<Vec<ResolvedApproval>, String>` |
| `resume_task` | `task_id, approved, reason` | `Result<, String>` |

### LLM (3)

| Commande | Parametres | Retour |
|---|---|---|
| `list_llm_backends` | — | `Vec<LlmBackendStatus>` |
| `ping_llm_backend` | `name: String` | `u64` (latency_ms) |
| `get_llm_cost_stats` | `days: Option<u32>` | `LlmCostStats` |

### Triggers (9 — 5 + 3 + 1)

| Commande | Parametres | Retour |
|---|---|---|
| `list_triggers` | — | `Vec<TriggerStatus>` |
| `list_trigger_definitions` | — | `Vec<TriggerDefinitionView>` |
| `get_trigger_definition` | `id: String` | `TriggerDefinitionView` |
| `create_trigger` | `definition: CreateTriggerRequest` | `TriggerDefinitionView` |
| `update_trigger` | `id: String, definition: UpdateTriggerRequest` | `TriggerDefinitionView` |
| `delete_trigger` | `id: String` | `` |
| `set_trigger_enabled` | `id: String, enabled: bool` | `` |
| `fire_trigger` | `id: String` | `String` (task_id) |
| `get_trigger_logs` | `id: String` | `Vec<TriggerLogEntry>` |

### Pipelines (9 — 5 + 3 + 1)

| Commande | Parametres | Retour |
|---|---|---|
| `list_pipelines` | — | `Vec<PipelineInfo>` |
| `list_pipeline_definitions` | — | `Vec<PipelineDefinitionView>` |
| `get_pipeline_definition` | `id: String` | `PipelineDefinitionView` |
| `create_pipeline` | `definition: CreatePipelineRequest` | `PipelineDefinitionView` |
| `update_pipeline` | `id: String, definition: UpdatePipelineRequest` | `PipelineDefinitionView` |
| `delete_pipeline` | `id: String` | `` |
| `list_pipeline_runs` | `pipeline_id: String, limit: Option<usize>` | `Vec<PipelineRunSummary>` |
| `run_pipeline` | `pipeline_id: String, inputs: Option<Value>` | `RunPipelineResult` |
| `get_pipeline_run_detail` | `run_id: String` | `PipelineRunDetail` |

### Memory (4)

| Commande | Parametres | Retour |
|---|---|---|
| `list_memory_namespaces` | — | `Vec<String>` |
| `list_memory_entries` | `namespace, type?, limit?` | `Vec<MemoryEntry>` |
| `search_memory` | `namespace: String, query: String, limit?` | `Vec<MemorySearchResult>` |
| `delete_memory_entry` | `namespace: String, id: String` | `` |

### Notifications (8 — 3 + 5)

| Commande | Parametres | Retour |
|---|---|---|
| `list_notification_channels` | — | `Vec<NotificationChannelView>` |
| `create_notification_channel` | `channel: CreateChannelRequest` | `NotificationChannelView` |
| `update_notification_channel` | `id: String, channel: UpdateChannelRequest` | `NotificationChannelView` |
| `delete_notification_channel` | `id: String` | `` |
| `get_notification_events` | — | `Vec<String>` |
| `set_notification_events` | `events: Vec<String>` | `` |
| `test_notification_channel` | `channel_id: String` | `ChannelTestResult` |
| `get_notification_logs` | `limit: Option<usize>` | `Vec<NotificationLogEntry>` |

### Observability (3)

| Commande | Parametres | Retour |
|---|---|---|
| `get_global_timeline` | `window_minutes: Option<u32>` | `Vec<GlobalTimelineEvent>` |
| `get_tool_audit_trail` | `limit: Option<usize>` | `Vec<AuditTrailEntry>` |
| `get_llm_daily_costs` | `days: Option<u32>` | `Vec<LlmDailyCostEntry>` |

### Tool Governance (12)

Commandes pilotant `NativeToolRegistry`, `ToolCredentialStore`, `PrefixRuleEngine` et `PermissionAuditLog` via `governance.db`. Seuls les scopes `project` et `global` sont exposés au frontend — ils sont persistés dans `governance.db`.

| Commande | Parametres | Retour |
|---|---|---|
| `governance_list_tools` | — | `Vec<ToolStatusDto>` |
| `governance_set_tool_enabled` | `tool_name: String, enabled: bool` | `` |
| `governance_get_tool_config` | `tool_name: String` | `Option<Value>` |
| `governance_set_tool_config` | `tool_name: String, config: Value` | `` |
| `governance_list_credentials` | `tool_name: Option<String>` | `Vec<CredentialEntryDto>` |
| `governance_set_credential` | `tool_name: String, key_name: String, value: String` | `` |
| `governance_delete_credential` | `tool_name: String, key_name: String` | `` |
| `governance_test_credential` | `tool_name: String, key_name: String` | `CredentialTestResultDto` |
| `governance_list_permission_rules` | `filter: PermissionRuleFilter` | `Vec<PermissionRuleDto>` |
| `governance_revoke_permission_rule` | `id: i64` | `` |
| `governance_revoke_all_rules` | `scope: String, project_path: Option<String>` | `u32` |
| `governance_list_audit` | `tool_name: Option<String>, limit: Option<u32>, offset: Option<u32>` | `Vec<AuditEntryDto>` |

DTOs définis dans `commands/tool_governance.rs` : `ToolStatusDto`, `CredentialEntryDto`, `CredentialTestResultDto`, `PermissionRuleFilter`, `PermissionRuleDto`, `AuditEntryDto`.

### Configuration (2)

| Commande | Parametres | Retour |
|---|---|---|
| `get_config` | — | `ApollaConfigView` |
| `open_config_in_editor` | — | `` |

### Chat (10)

| Commande | Parametres | Retour |
|---|---|---|
| `create_chat_session` | `request: CreateSessionRequest` | `Result<ChatSessionSummary, String>` |
| `list_chat_sessions` | `status: Option<String>` | `Result<Vec<ChatSessionSummary>, String>` |
| `get_chat_session` | `session_id: String` | `Result<ChatSessionDetail, String>` |
| `close_chat_session` | `session_id: String` | `Result<(), String>` |
| `delete_chat_session` | `session_id: String` | `Result<(), String>` |
| `rename_chat_session` | `session_id: String, title: String` | `Result<(), String>` |
| `update_chat_session` | `session_id: String, request: UpdateSessionRequest` | `Result<(), String>` |
| `generate_chat_session_name` | `session_id: String, first_message: String` | `Result<String, String>` (titre genere) |
| `send_chat_message` | `session_id: String, content: String` | `Result<String, String>` (message_id) |
| `authorize_chat_tool` | `session_id, message_id, tool_name, decision` | `Result<(), String>` |

### STT (5)

| Commande | Parametres | Retour |
|---|---|---|
| `get_stt_status` | — | `Result<SttStatus, String>` |
| `list_transcriptions` | `limit: Option<u32>` | `Result<Vec<TranscriptRow>, String>` |
| `delete_transcription` | `id: String` | `Result<, String>` |
| `transcribe_file` | `file_path: String` | `Result<TranscriptRow, String>` |
| `list_stt_models` | — | `Result<Vec<SttModelInfo>, String>` |

### Onboarding (6 — commandes utilitaires)

| Commande | Parametres | Retour |
|---|---|---|
| `check_onboarded` | — | `bool` |
| `mark_onboarded` | — | `` |
| `reset_onboarding` | — | `` |
| `check_python` | — | `bool` |
| `check_llm_configured` | — | `bool` |
| `check_hello_agent_exists` | — | `Option<String>` (path) |

### Mises a jour in-app (2)

| Commande | Parametres | Retour | Description |
|---|---|---|---|
| `check_for_update` | — | `Result<UpdateCheckResult, String>` | Interroge l'endpoint GitHub Releases configure dans `tauri.conf.json`. Retourne `available: true` + `new_version` si une version plus recente existe. |
| `install_update` | — | `Result<(), String>` | Telecharge et installe la mise a jour disponible, puis redémarre l'application. Doit etre appele apres `check_for_update`. |

```rust
pub struct UpdateCheckResult {
    pub available: bool,
    pub current_version: String,
    pub new_version: Option<String>,
    pub release_notes: Option<String>,
}
```

---

## 4. Frontend Svelte (ADR-028)

### 4.1 Stack

- **Svelte 5** (runes `$state`, `$effect`) + **Vite 6** — framework reactif leger
- **bits-ui** — composants headless accessibles (Button, Card, Badge, Sheet)
- **Tailwind CSS 3.4** — utilitaires CSS + design tokens
- **@tauri-apps/api** — bridge IPC Tauri
- **@tauri-apps/plugin-dialog** — file picker natif
- **@tauri-apps/plugin-notification** — notifications natives OS

### 4.2 Navigation

Store Svelte `currentRoute` (source de vérité : `ui/src/lib/stores/navigation.ts`) :

```typescript
type Route =
  | "dashboard"      // Vue d'accueil (route par defaut au demarrage)
  | "agents"         // Gestion agents
  | "tasks"          // Liste et detail taches
  | "approvals"      // Approbations HITL
  | "chat"           // Sessions de chat interactif
  | "transcriptions" // Historique STT + transcription fichier
  | "integrations"   // Connexions MCP (operator) / MCP Servers (builder)
  | "llm"            // Backends LLM, ping, statistiques
  | "triggers"       // Triggers TOML, enable/disable, fire
  | "pipelines"      // Runs multi-agent, steps temps reel
  | "memory"         // Namespaces, recherche FTS5, suppression
  | "notifications"  // Canaux, test, historique
  | "observability"  // Timeline, audit trail, couts LLM
  | "settings";      // Configuration lecture seule (ADR-029)
```

Rendu conditionnel `{#if}` dans `Main.svelte`. Pas de router externe — routing par store client-side. L'onboarding est gere séparément par `App.svelte` via le store `onboardingStore.showOnboarding` (overlay fullscreen, pas une route).

### 4.3 Sidebar

Rail d'icones 56px permanent (V4, `data-state="rail"`). Pas de mode expand/collapse ni de categories — liste plate de 7 destinations + Settings en pied.

| Route | Icone | Badge |
|---|---|---|
| `dashboard` | Home | — |
| `chat()` | MessageSquare | compteur sessions actives |
| `agents` | Bot | — |
| `projects` | FolderOpen | — |
| `tasks` | CheckSquare | compteur taches in-flight (pulse animee) |
| `inbox` | Inbox | compteur approbations en attente (`pendingCount + pendingChatApprovalCount`) |
| `integrations` | Plug | — |
| `settings` | Settings | — (pied de sidebar) |

Comportement :
- Chaque bouton affiche un **tooltip** au survol (label en francais)
- La route active est materalisee par une **barre verticale** (`active-bar`) a gauche du bouton
- Sur mobile, la sidebar bascule en **drawer** (overlay semi-transparent, focus trap, fermeture Echap)
- L'avatar utilisateur (initiales) est affiche en dernier element du rail

Attributs `data-testid` : `nav-<route>` sur chaque bouton (ex. `data-testid="nav-inbox"`).

### 4.4 SSE et stores reactifs

Le store `sse.ts` etablit une connexion SSE vers `localhost:7771/api/v1/dashboard/stream` avec reconnexion automatique (backoff exponentiel 1s → 30s max).

7 stores reactifs de base :

| Store | Type | Source SSE |
|---|---|---|
| `agents` | `AgentStatus[]` | channel `agents` |
| `tasks` | `TaskSummary[]` | channel `tasks` |
| `pendingApprovals` | `PendingApproval[]` | channel `approvals` |
| `llmBackends` | `LlmBackendStatus[]` | channel `llm` |
| `triggers` | `TriggerStatus[]` | channel `triggers` |
| `pipelineRuns` | `PipelineRunSummary[]` | channel `pipeline` |
| `connectionStatus` | `ConnectionStatus` | etat connexion SSE |
| `sttStatus` | `SttStatus \| null` | hydrate via IPC `get_stt_status` |
| `transcriptions` | `TranscriptRow[]` | hydrate via IPC `list_transcriptions` |
| `isRecording` | `boolean` | evenements `stt-recording-started/stopped` |
| `chatSessions` | `ChatSessionSummary[]` | evenement `chat-changed` |
| `currentSession` | `ChatSessionDetail \| null` | — |
| `chatTokenBuffer` | `string` | evenement `chat-token` (fast path) |

4 stores derives :

| Store derive | Calcul |
|---|---|
| `pendingCount` | nombre d'approbations en attente |
| `llmBackendCount` | nombre total de backends LLM |
| `readyLlmBackends` | backends avec statut `ready` |
| `errorLlmBackends` | backends avec statut `error` |

Pattern de rafraichissement : evenement SSE → appel IPC Tauri → mise a jour du store → re-render Svelte.

Traitement HITL specifique : `TaskInputRequired` → ajout dans `pendingApprovals` + notification native Tauri + emission evenement `tray-update`.

### 4.5 Types TypeScript

35+ interfaces definies dans `lib/types.ts` :

**Agents/Tasks/HITL :**
`AgentStatus`, `TaskSummary`, `PendingApproval`, `ResolvedApproval`, `TimelineEvent` (union discriminee par type)

**LLM :**
`LlmBackendStatus` (name, backend_type, model, status, latency_ms), `LlmPingResult`, `LlmCostStatsRow`

**Triggers :**
`TriggerStatus` (id, agent, source_kind, enabled, fire_count, skip_count, last_fired), `TriggerLogEntry`, `TriggerFireResult`

**Pipelines :**
`PipelineInfo`, `PipelineRunSummary`, `PipelineStepSummary`, `PipelineRunDetail`, `RunPipelineResult`

**Memory :**
`MemoryEntry` (episodic|semantic|procedural), `MemorySearchResult`

**Notifications :**
`NotificationChannel` (desktop|webhook|sse), `ChannelTestResult`, `NotificationLogEntry`

**Observability :**
`GlobalTimelineEvent`, `AuditTrailEntry`, `LlmDailyCostEntry`

**Chat :**
`ChatSessionSummary`, `ChatSessionDetail`, `ChatMessageView`, `ToolCallView`, `CreateSessionRequest`, `SendMessageRequest`, `ToolAuthorizationRequest`

**STT :**
`SttStatus`, `SttModelInfo`, `TranscriptRow`

**Config :**
`ConfigEntry`, `ConfigSection`, `ApollaConfigView`

### 4.6 Vues

**Agents** — Liste temps reel avec badges d'etat (ACTIVE/vert, DEGRADED/orange, STOPPED/gris). File picker natif Tauri pour enregistrer un agent `.py`. Drawer avec les 20 dernieres taches de l'agent.

**Tasks** — Liste filtrable par onglets (All/Running/Completed/Failed/Pending). Detail avec input/output complets. Timeline interactive avec 8 types d'evenements (task_transition, step_started, step_completed, llm_call, tool_call, hitl_suspended, hitl_resolved, task_completed).

**Approvals** — Cartes d'approbation avec compteur live (Xm Ys), prompt complet, contexte JSON depliable, boutons Approuver/Rejeter avec dialogs de confirmation. Historique des 20 dernieres approbations resolues (7 jours).

**LLM** — Grille de backends avec cards : nom, type (embedded/api), modele, badge statut (Ready/Loading/Error), bouton Ping avec affichage latence. Section statistiques : cout USD, tokens, appels par backend sur 7 jours. Refresh 30s.

**Triggers** — Vue editeur CRUD. Tableau avec ID, type badge (Cron/FileWatch/Webhook/Interval/Oneshot), cible agent, toggle enable/disable, compteur fires/skips. Boutons Fire et Logs. Dialogs `CreateTriggerDialog` et `EditTriggerDialog` avec champs dynamiques selon le type de source. Bouton Hot Reload. Suppression avec confirmation.

**Pipelines** — Vue editeur CRUD. Onglet Definitions (liste des pipelines, creation/edition/suppression) + Onglet Runs. `CreatePipelineDialog` et `EditPipelineDialog` avec gestion dynamique des steps, validation DAG live, sections conditions depliables. `PipelineDefinitionCard` affiche la topologie. Dialog "Nouveau run" : selection pipeline + input JSON optionnel. Mise a jour temps reel via SSE canal `pipeline`.

**Memory** — Selecteur de namespace en dropdown. Recherche FTS5 debounced 300ms (minimum 3 caracteres), score BM25 affiche. Table expandable : type badge (episodic/semantic/procedural), cle, preview 100 chars, TTL, timestamp. Suppression par ligne avec dialog de confirmation.

**Notifications** — Vue editeur CRUD. Onglet Canaux : creation/edition/suppression canaux (`CreateChannelDialog`, `EditChannelDialog`), type webhook config URL/headers. Onglet Evenements Globaux : checkboxes des evenements reconnus (`GlobalEventsEditor`). Bouton Tester avec resultat inline. Logs : 50 dernieres notifications envoyees.

**Observability** — 3 onglets :
- *Timeline* : evenements des N dernieres heures (slider 30min→24h), filtres par type (Task/Tool/LLM/Trigger/HITL), liste chronologique inversee avec icones + detail expandable
- *LLM Costs* : bar chart SVG natif Svelte (pas de lib externe), cout par jour 7j, barres colorees par backend
- *Audit Trail* : table expandable (args_json, stdout, stderr), filtres par outil + agent

**Integrations** — Route `/integrations`, catégorie "Infrastructure". Le rendu change selon le mode actif : mode **Operator** affiche "Connexions" (OperatorConnectionCard + OperatorCatalogue + ConnectorWizard 5 étapes + OperatorServerManage) ; mode **Builder** affiche "MCP Servers" (BuilderServerRow + BuilderServerDetail + BuilderRegistryBrowser). Le catalogue est alimenté par le `RegistryClient` qui interroge `registry.modelcontextprotocol.io` avec cache local JSON (`~/.apollia/cache/mcp-registry.json`). Les secrets saisis dans le wizard sont stockés dans l'OS Keychain via le `SecretStore` (crate `keyring`). Le disclaimer de sécurité MCP s'affiche une seule fois (persisté dans `localStorage`). Les cartes affichent `TrustBadge` (Official / Verified / Community / Custom) et `ConnectionStatusIndicator`. i18n complet EN + FR (clés `integrations.*`). Voir [Guide Intégrations](./Integrations-Guide) pour la documentation utilisateur.

**Transcriptions** — Route `/transcriptions`, catégorie "Données", icône micro. Bandeau statut STT (enabled/disabled, modèle chargé, Metal/CUDA). Liste des transcriptions en ordre chronologique inversé avec `TranscriptCard` (texte, langue, source icône 🎙️/📁/🔌, durée, timestamp). Boutons Copy et Delete par carte. `TranscribeFileDialog` : file picker natif filtré (.wav,.mp3,.ogg,.m4a), spinner pendant la transcription. Badge "Enregistrement" animé quand `isRecording = true`. Empty state avec icône Mic. Section STT dans Settings (lecture seule — ADR-029) : enabled, hotkey, clipboard mode, modèle actif, langue, lien vers doc `apollia.toml`.

**Settings** — Vue multi-onglets. Navigation gauche (`SettingsNav.svelte`) regroupée en sections.

- *Configuration* — lecture seule nettoyee (ADR-029). Affiche uniquement les sections structurelles TOML : [runtime], [llm], [budget], [memory], [tools], [stt]. Bouton "Ouvrir dans l'editeur" appelle `open_config_in_editor` via `open::that`.
- *Outils* (`/settings/tools`, `Tools.svelte`) — gouvernance des outils natifs. Liste les outils (`governance_list_tools`) avec toggle enable/disable (`ToolCard.svelte`). Bouton "Configurer" ouvre `ToolConfigDrawer.svelte` (panel latéral Sheet) pour les outils exposant une config : `web_search` (backend Auto/DDG/Brave, timeouts, résultats max, `require_configured`) et `web_read` (timeout, taille max, garde SSRF). `CredentialField.svelte` gère les credentials (Brave API key) : saisie masquée, enregistrement via `governance_set_credential`, suppression via `governance_delete_credential`, test live via `governance_test_credential`. Store réactif `toolGovernance.ts` applique un état optimiste pour les toggles avec rollback automatique si l'IPC échoue.
- Les autres onglets (LLM backends, Permissions, Mémoires, Raccourcis, Danger, etc.) sont gérés par leurs routes respectives dans `routes/settings/`.

---

## 5. Onboarding multi-phases

Onboarding interactif affiche au premier lancement si `get_onboarding_state` retourne `phase != "done"`. Ecrans fullscreen séquentiels avec machine a etats persistee.

### 5.1 Machine a etats — 7 phases

```
welcome → llm_setup → ai_setup → acquaintance → guided_tour → graduation → done
```

Chaque transition est persisted via `advance_onboarding_phase(phase)`. L'interruption est possible a tout moment — la barre de reprise `OnboardingResumeBar` s'affiche a la prochaine ouverture.

### 5.2 Composants Svelte (onboarding/)

| Composant | Phase | Description |
|---|---|---|
| `OnboardingWelcome` | `welcome` | Accroche + selection profil (Operator / Builder) |
| `OnboardingLlmSetup` | `llm_setup` | File picker `.gguf` ou "configurer plus tard" |
| `OnboardingAiSetup` | `ai_setup` | Scan auto LLM + STT, selection avec badge Recommande |
| `OnboardingAcquaintance` | `acquaintance` | Chat embarque avec agent d'onboarding |
| `OnboardingGuidedTour` | `guided_tour` | Tour interactif (voir §5.3) |
| `OnboardingGraduation` | `graduation` | Stats parcours + quick-cards + toggle companion |
| `OnboardingResumeBar` | toutes | Bandeau de reprise post-interruption |

### 5.3 Tour guide

Le `OnboardingGuidedTour` orchestre :
- `TourSpotlight` — overlay SVG avec decoupage de zone
- `TourStepCard` — carte flottante positionnee via `calculateCardPosition`
- `TourProgressRail` — barre de progression verticale fixe a gauche
- `VoiceIndicator` — indicateur STT push-to-talk

**Etapes :** 8 etapes pour Operator, 10 pour Builder. La sequence est chargee via `get_tour_steps(profile)`.

**Interactions :** navigation clavier (→ / ← / Echap), commandes vocales STT (suivant / precedent / passer / question libre), actions interactives per-etape avec timeout auto-skip (30s).

Voir [Onboarding-Tour-Steps](./Onboarding-Tour-Steps) pour les tables completes des etapes.

### 5.4 IPC Onboarding (17 commandes)

Voir [Onboarding-System](./Onboarding-System) pour la spec IPC complete, les types TypeScript, les cles UserMemory et les RuntimeEvents.

---

## 6. Companion Apollia

Panneau flottant draggable/resizable disponible pendant et apres l'onboarding.

### 6.1 Composants

| Composant | Description |
|---|---|
| `CompanionPanel` | Panneau principal (drag, resize, minimise, ferme) |
| `CompanionToggle` | Bouton d'ouverture/fermeture avec label i18n |
| `CompanionContextProvider` | Injecte le contexte route dans le store |

### 6.2 Etats du companion

| Etat | Description |
|---|---|
| `hidden` | Masque completement (post-graduation si desactive) |
| `minimized` | Bouton "Restaurer" en bas a droite |
| `visible` | Panneau complet — session chat active |

### 6.3 Sessions chat

Le companion utilise une session de chat ordinaire (`create_chat_session` avec `mode: "free"`). La session est creee a la demande lors de la premiere ouverture. Les messages s'echangent via `send_chat_message`.

---

## 7. System tray

### Menu contextuel

3 items :
1. **"Ouvrir Apollia OS"** — affiche/focus la fenetre principale
2. **Compteur approbations** — desactive si 0, affiche "N approbations en attente" si > 0
3. **"Quitter"** — arret graceful via `POST /api/v1/shutdown` puis `exit(0)`

### Comportement fenetre

- **Clic gauche sur l'icone tray** → toggle visibilite de la fenetre
- **Fermeture fenetre** → masque la fenetre (intercepte `CloseRequested`, `prevent_close`). Le runtime continue en arriere-plan
- **"Quitter" via tray** → arret graceful complet

### Mise a jour dynamique

Le frontend emet un evenement `tray-update` avec `{ active_agents, pending_approvals }` :
- Tooltip formate en francais : "Apollia OS — 3 agents actifs, 2 approbations en attente" (singulier/pluriel)
- Menu item approbations : texte et etat `enabled` mis a jour

### Notifications natives

Declenchees quand la fenetre est masquee + `TaskInputRequired` recu via SSE :
- Titre : "Action requise — Apollia OS"
- Corps : "Tache XXX attend votre approbation"
- Clic → affiche la fenetre + navigue vers /approvals
- Utilise `@tauri-apps/plugin-notification` (permission demandee au premier usage)

---

## 8. Build et packaging

### 8.1 Formats de sortie

| Plateforme | Format | Commande |
|---|---|---|
| macOS | `.dmg` + `.app` | `cargo tauri build` |
| Linux | `.AppImage` + `.deb` | `cargo tauri build` |

### 8.2 Configuration Tauri

- Fenetre : 1280×800 par defaut, minimum 900×600
- Plugins : `tauri-plugin-dialog` (file picker), `tauri-plugin-notification` (notifications natives), `tauri-plugin-updater` (mises a jour in-app via GitHub Releases)
- Endpoint updater configure dans `tauri.conf.json` → section `plugins.updater.endpoints`
- Build : Vite dev server sur port 5173, frontend dist dans `ui/dist`

### 8.3 CI

Le workflow `.github/workflows/build-desktop.yml` se declenche sur les tags `v*` et produit les artefacts pour macOS (macos-latest) et Linux (ubuntu-latest).

### 8.4 Installation

Voir la section "Installation application desktop" dans [INSTALL](./INSTALL.md).

---

## 9. Coexistence CLI + Desktop

Les deux modes d'acces (CLI et Desktop) partagent le meme runtime :

- **API REST** — TCP `localhost:7771` (Tauri commandes + SSE)
- **Unix socket** — `/tmp/apollia.sock` (CLI `apollia-os status`)
- **EventBus** — Broadcast Tokio partage (SSE dashboard + Tauri SSE stores)

Un seul processus, un seul Supervisor, un seul jeu d'acteurs Tokio. Pas de conflit de port ou de socket.

---

## 10. Attributs de test

Elements `data-testid` sur les composants principaux pour les tests e2e :

- Layout : `app-loading`, `app-main`, `sidebar`
- Navigation (rail) : `nav-dashboard`, `nav-chat`, `nav-agents`, `nav-projects`, `nav-tasks`, `nav-inbox`, `nav-integrations`, `nav-settings`
- Contenu : `agents-header`, `register-agent-btn`, `agents-grid`

---

## 11. Decisions architecturales

- **ADR-027** — Processus unique Tauri + runtime embarque
- **ADR-028** — Frontend Svelte : UX first, UI sprint dedie
- **ADR-029** — Settings lecture seule (round-trip TOML detruirait les commentaires)
- **ADR-033** — Config operateur SQLite : separation structurel (TOML) / operationnel (SQLite)
- **ADR-041** — Moteur STT embarqué : whisper-rs V1, trait SttBackend

---

## 12. Nouveaux composants

### 6 composants HITL spécialisés

`PermissionDispatcher.svelte` route vers le bon composant selon `permission_type` :

| Composant | Type d'outil | Affichage spécifique |
|---|---|---|
| `BashPermissionView.svelte` | `bash` | Commande colorisée, working_dir, 3 boutons |
| `FileEditPermissionView.svelte` | `file_edit` | Diff coloré ligne par ligne (+/-) |
| `FileWritePermissionView.svelte` | `file_write` | Badge vert "Créer" ou orange "Écraser" |
| `FilesystemPermissionView.svelte` | `filesystem` | Opération (delete/move/mkdir) + paths |
| `McpPermissionView.svelte` | `mcp` | Serveur + outil + arguments JSON indenté |
| `GenericPermissionView.svelte` | autres | JSON brut (fallback) |

**Bouton "Toujours autoriser"** — crée une `PrefixRule` via `add_permission_prefix_rule` Tauri IPC (intégration avec `apollia-permissions`).

```typescript
async function alwaysAllow(scope: 'project' | 'global', projectPath?: string) {
  await invoke('add_permission_prefix_rule', {
    toolName: permission.tool_name,
    argPrefix: extractArgPrefix(permission),
    action: 'allow',
    scope,
    projectPath: scope === 'project' ? projectPath : undefined,
  });
  await approve();
}
```

Fichiers :
```
crates/apollia-desktop/src/lib/components/permissions/
├── BashPermissionView.svelte
├── FileEditPermissionView.svelte
├── FileWritePermissionView.svelte
├── FilesystemPermissionView.svelte
├── McpPermissionView.svelte
├── GenericPermissionView.svelte
└── PermissionDispatcher.svelte
```

### `TokenBudgetWidget.svelte`

Widget dans le header du desktop affichant le coût LLM de la session en temps réel.

- Affiche `$X.XXX` — mis à jour < 500ms après chaque appel LLM
- Passe à **orange** à 80% du seuil configuré
- Passe à **rouge** + badge `!` si `threshold_exceeded = true`
- Alimenté par `RuntimeEvent::TokenBudgetUpdated` via SSE

### `PlanAlternativesView.svelte`

Composant affichant deux plans alternatifs ORIA et permettant à l'opérateur de choisir.

- Deux cartes plan (A et B) avec les étapes listées
- Boutons "Choisir Plan A" / "Choisir Plan B"
- Appel IPC `choose_plan` → persistance dans `plan_choices` SQLite

### Commande Tauri IPC ajoutée

```rust
#[tauri::command]
pub async fn get_cost_alert_threshold(
    state: tauri::State<'_, AppState>,
) -> Result<Option<f64>, String> {
    Ok(state.config.llm.cost_alert_threshold_usd)
}
```
