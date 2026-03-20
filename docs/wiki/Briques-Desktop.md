# Application Desktop — Tauri v2 + Runtime embarque

> *L'application desktop Apollia OS embarque le runtime complet dans un processus unique. Double-clic → fenetre → 10 vues temps reel couvrant 100% des capacites CLI.*

---

## 1. Architecture — Processus unique (ADR-027)

L'application desktop est une crate Tauri v2 (`apollia-desktop`) qui demarre le runtime Apollia en interne via `init_embedded()`. Un seul binaire distribue a la fois le runtime Rust et le frontend Svelte.

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
        ├── .invoke_handler(commands)  ← 29 commandes IPC
        ├── .plugin(dialog)            ← file picker natif
        ├── .plugin(notification)      ← notifications natives
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
│       ├── observability.rs   ← get_global_timeline, get_tool_audit_trail, get_llm_daily_costs
│       ├── config.rs          ← get_config, open_config_in_editor
│       └── onboarding.rs      ← check_onboarded, mark_onboarded, reset_onboarding, check_python, check_llm_configured, check_hello_agent_exists
└── ui/                        ← application Svelte 5
    ├── package.json
    ├── vite.config.ts
    └── src/
        ├── App.svelte
        ├── lib/
        │   ├── types.ts       ← 35+ interfaces TypeScript
        │   ├── stores/
        │   │   ├── sse.ts         ← SSE connection + 7 stores reactifs + 4 derives
        │   │   └── navigation.ts  ← currentRoute + showOnboarding
        │   └── components/ui/     ← Button, Card, Badge, Sheet, Separator (bits-ui)
        ├── components/
        │   ├── layout/        ← Sidebar.svelte, Main.svelte
        │   ├── agents/        ← AgentCard.svelte, AgentLogs.svelte
        │   ├── tasks/         ← TaskList.svelte, TaskDetail.svelte, TaskTimeline.svelte
        │   ├── hitl/          ← ApprovalCard.svelte, ApprovalHistory.svelte
        │   ├── llm/           ← LlmBackendCard.svelte, LlmStats.svelte
        │   ├── triggers/      ← TriggerRow, TriggerLogs, CreateTriggerDialog, EditTriggerDialog
        │   ├── pipelines/     ← PipelineRunCard, PipelineRunDetail, PipelineDefinitionCard, CreatePipelineDialog, EditPipelineDialog
        │   ├── memory/        ← NamespaceSelector.svelte, MemorySearch.svelte, MemoryTable.svelte
        │   ├── notifications/ ← NotificationChannelCard, NotificationLog, CreateChannelDialog, EditChannelDialog, GlobalEventsEditor
        │   ├── observability/ ← TimelineGlobal.svelte, LlmCostChart.svelte, AuditTrailTable.svelte
        │   └── onboarding/    ← StepEnvironment.svelte, StepFirstAgent.svelte, StepFirstTask.svelte
        └── routes/            ← 10 fichiers .svelte (un par route)
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

40 commandes exposees au frontend Svelte via `#[tauri::command]` (29 Sprint 15 + 11 Sprint 17) :

### Agents (3)

| Commande | Parametres | Retour |
|---|---|---|
| `list_agents` | — | `Vec<AgentInfo>` |
| `start_agent` | `path: String` | `Result<String, String>` (agent_id) |
| `stop_agent` | `agent_id: String` | `Result<(), String>` |

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
| `resume_task` | `task_id, approved, reason` | `Result<(), String>` |

### LLM (3)

| Commande | Parametres | Retour |
|---|---|---|
| `list_llm_backends` | — | `Vec<LlmBackendStatus>` |
| `ping_llm_backend` | `name: String` | `u64` (latency_ms) |
| `get_llm_cost_stats` | `days: Option<u32>` | `LlmCostStats` |

### Triggers (8 — 5 Sprint 15 + 3 Sprint 17)

| Commande | Parametres | Retour |
|---|---|---|
| `list_triggers` | — | `Vec<TriggerStatus>` |
| `list_trigger_definitions` | — | `Vec<TriggerDefinitionView>` |
| `get_trigger_definition` | `id: String` | `TriggerDefinitionView` |
| `create_trigger` | `definition: CreateTriggerRequest` | `TriggerDefinitionView` *(Sprint 17)* |
| `update_trigger` | `id: String, definition: UpdateTriggerRequest` | `TriggerDefinitionView` *(Sprint 17)* |
| `delete_trigger` | `id: String` | `()` *(Sprint 17)* |
| `set_trigger_enabled` | `id: String, enabled: bool` | `()` |
| `fire_trigger` | `id: String` | `String` (task_id) |
| `get_trigger_logs` | `id: String` | `Vec<TriggerLogEntry>` |

### Pipelines (8 — 5 Sprint 15 + 3 Sprint 17)

| Commande | Parametres | Retour |
|---|---|---|
| `list_pipelines` | — | `Vec<PipelineInfo>` |
| `list_pipeline_definitions` | — | `Vec<PipelineDefinitionView>` |
| `get_pipeline_definition` | `id: String` | `PipelineDefinitionView` |
| `create_pipeline` | `definition: CreatePipelineRequest` | `PipelineDefinitionView` *(Sprint 17)* |
| `update_pipeline` | `id: String, definition: UpdatePipelineRequest` | `PipelineDefinitionView` *(Sprint 17)* |
| `delete_pipeline` | `id: String` | `()` *(Sprint 17)* |
| `list_pipeline_runs` | `pipeline_id: String, limit: Option<usize>` | `Vec<PipelineRunSummary>` |
| `run_pipeline` | `pipeline_id: String, inputs: Option<Value>` | `RunPipelineResult` |
| `get_pipeline_run_detail` | `run_id: String` | `PipelineRunDetail` |

### Memory (4)

| Commande | Parametres | Retour |
|---|---|---|
| `list_memory_namespaces` | — | `Vec<String>` |
| `list_memory_entries` | `namespace, type?, limit?` | `Vec<MemoryEntry>` |
| `search_memory` | `namespace: String, query: String, limit?` | `Vec<MemorySearchResult>` |
| `delete_memory_entry` | `namespace: String, id: String` | `()` |

### Notifications (8 — 3 Sprint 15 + 5 Sprint 17)

| Commande | Parametres | Retour |
|---|---|---|
| `list_notification_channels` | — | `Vec<NotificationChannelView>` |
| `create_notification_channel` | `channel: CreateChannelRequest` | `NotificationChannelView` *(Sprint 17)* |
| `update_notification_channel` | `id: String, channel: UpdateChannelRequest` | `NotificationChannelView` *(Sprint 17)* |
| `delete_notification_channel` | `id: String` | `()` *(Sprint 17)* |
| `get_notification_events` | — | `Vec<String>` *(Sprint 17)* |
| `set_notification_events` | `events: Vec<String>` | `()` *(Sprint 17)* |
| `test_notification_channel` | `channel_id: String` | `ChannelTestResult` |
| `get_notification_logs` | `limit: Option<usize>` | `Vec<NotificationLogEntry>` |

### Observability (3)

| Commande | Parametres | Retour |
|---|---|---|
| `get_global_timeline` | `window_minutes: Option<u32>` | `Vec<GlobalTimelineEvent>` |
| `get_tool_audit_trail` | `limit: Option<usize>` | `Vec<AuditTrailEntry>` |
| `get_llm_daily_costs` | `days: Option<u32>` | `Vec<LlmDailyCostEntry>` |

### Configuration (2)

| Commande | Parametres | Retour |
|---|---|---|
| `get_config` | — | `ApollaConfigView` |
| `open_config_in_editor` | — | `()` |

### Onboarding (6 — commandes utilitaires)

| Commande | Parametres | Retour |
|---|---|---|
| `check_onboarded` | — | `bool` |
| `mark_onboarded` | — | `()` |
| `reset_onboarding` | — | `()` |
| `check_python` | — | `bool` |
| `check_llm_configured` | — | `bool` |
| `check_hello_agent_exists` | — | `Option<String>` (path) |

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

Store Svelte `currentRoute` avec 10 routes :

```typescript
type Route =
  | "agents"        // Gestion agents (Sprint 14)
  | "tasks"         // Liste et detail taches (Sprint 14)
  | "approvals"     // Approbations HITL (Sprint 14)
  | "llm"           // Backends LLM, ping, statistiques
  | "triggers"      // Triggers TOML, enable/disable, fire
  | "pipelines"     // Runs multi-agent, steps temps reel
  | "memory"        // Namespaces, recherche FTS5, suppression
  | "notifications" // Canaux, test, historique
  | "observability" // Timeline, audit trail, couts LLM
  | "settings";     // Configuration lecture seule (ADR-029)
```

Rendu conditionnel `{#if}` dans `Main.svelte`. Pas de router externe — routing par store client-side.

### 4.3 Sidebar

Navigation regroupee en 4 categories :

| Categorie | Routes |
|---|---|
| **Operations** | agents, tasks, approvals |
| **Infrastructure** | llm, triggers, pipelines |
| **Donnees** | memory, notifications, observability |
| **Settings** | settings (en bas, avant l'indicateur de connexion) |

Badge rouge sur `approvals` affichant le nombre d'approbations en attente.
Indicateur de connexion SSE en bas (pastille verte/rouge + label).
Attributs `data-testid` sur chaque element de navigation pour les tests e2e.

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

**Agents/Tasks/HITL (Sprint 14) :**
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

**Config :**
`ConfigEntry`, `ConfigSection`, `ApollaConfigView`

### 4.6 Vues

**Agents** — Liste temps reel avec badges d'etat (ACTIVE/vert, DEGRADED/orange, STOPPED/gris). File picker natif Tauri pour enregistrer un agent `.py`. Drawer avec les 20 dernieres taches de l'agent.

**Tasks** — Liste filtrable par onglets (All/Running/Completed/Failed/Pending). Detail avec input/output complets. Timeline interactive avec 8 types d'evenements (task_transition, step_started, step_completed, llm_call, tool_call, hitl_suspended, hitl_resolved, task_completed).

**Approvals** — Cartes d'approbation avec compteur live (Xm Ys), prompt complet, contexte JSON depliable, boutons Approuver/Rejeter avec dialogs de confirmation. Historique des 20 dernieres approbations resolues (7 jours).

**LLM** — Grille de backends avec cards : nom, type (embedded/api), modele, badge statut (Ready/Loading/Error), bouton Ping avec affichage latence. Section statistiques : cout USD, tokens, appels par backend sur 7 jours. Refresh 30s.

**Triggers** — Vue editeur CRUD (Sprint 17). Tableau avec ID, type badge (Cron/FileWatch/Webhook/Interval/Oneshot), cible agent, toggle enable/disable, compteur fires/skips. Boutons Fire et Logs. Dialogs `CreateTriggerDialog` et `EditTriggerDialog` avec champs dynamiques selon le type de source. Bouton Hot Reload. Suppression avec confirmation.

**Pipelines** — Vue editeur CRUD (Sprint 17). Onglet Definitions (liste des pipelines, creation/edition/suppression) + Onglet Runs. `CreatePipelineDialog` et `EditPipelineDialog` avec gestion dynamique des steps, validation DAG live, sections conditions depliables. `PipelineDefinitionCard` affiche la topologie. Dialog "Nouveau run" : selection pipeline + input JSON optionnel. Mise a jour temps reel via SSE canal `pipeline`.

**Memory** — Selecteur de namespace en dropdown. Recherche FTS5 debounced 300ms (minimum 3 caracteres), score BM25 affiche. Table expandable : type badge (episodic/semantic/procedural), cle, preview 100 chars, TTL, timestamp. Suppression par ligne avec dialog de confirmation.

**Notifications** — Vue editeur CRUD (Sprint 17). Onglet Canaux : creation/edition/suppression canaux (`CreateChannelDialog`, `EditChannelDialog`), type webhook config URL/headers. Onglet Evenements Globaux : checkboxes des evenements reconnus (`GlobalEventsEditor`). Bouton Tester avec resultat inline. Logs : 50 dernieres notifications envoyees.

**Observability** — 3 onglets :
- *Timeline* : evenements des N dernieres heures (slider 30min→24h), filtres par type (Task/Tool/LLM/Trigger/HITL), liste chronologique inversee avec icones + detail expandable
- *LLM Costs* : bar chart SVG natif Svelte (pas de lib externe), cout par jour 7j, barres colorees par backend
- *Audit Trail* : table expandable (args_json, stdout, stderr), filtres par outil + agent

**Settings** — Vue lecture seule nettoyee (ADR-029, Sprint 17). Affiche uniquement les sections structurelles TOML : [runtime], [llm], [budget], [memory], [tools]. Les sections operationnelles (triggers, pipelines, notifications) ont ete retirees — un bandeau info redirige vers les vues dediees. Bouton "Ouvrir dans l'editeur" appelle `open_config_in_editor()` via `open::that()`.

---

## 5. Onboarding wizard (premier lancement)

Wizard affiche au premier lancement si `~/.apollia/.onboarded` n'existe pas. Modal fullscreen avec stepper visible.

### Etape 1 — Verification environnement

Verifie automatiquement :
- Runtime Apollia : toujours ✓ (embarque)
- Python 3 : `check_python()` execute `python3 --version`
- LLM configure : `check_llm_configured()` verifie via `/api/v1/llm/status`

Indicateurs visuels ✓/✗ pour chaque verification. Bouton "Continuer" actif si Python OK.

### Etape 2 — Premier agent

File picker natif Tauri pour selectionner un fichier `.py`. Appel `start_agent(path)` → affichage etat de chargement → confirmation ACTIVE.

Raccourci : `check_hello_agent_exists()` verifie si `agents/hello_agent.py` existe et propose le chemin.

### Etape 3 — Premiere tache

Textarea pour saisir l'input. Appel `submit_task(agentId, input)` avec l'agent de l'etape 2. Affichage progression SSE → output → bouton "Terminer".

**Skip :** Bouton "Passer" sur chaque etape → appelle `mark_onboarded()` → redirige vers /agents.

---

## 6. System tray (STORY-151)

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

## 7. Build et packaging

### 7.1 Formats de sortie

| Plateforme | Format | Commande |
|---|---|---|
| macOS | `.dmg` + `.app` | `cargo tauri build` |
| Linux | `.AppImage` + `.deb` | `cargo tauri build` |

### 7.2 Configuration Tauri

- Fenetre : 1280×800 par defaut, minimum 900×600
- Plugins : `tauri-plugin-dialog` (file picker), `tauri-plugin-notification` (notifications natives)
- Build : Vite dev server sur port 5173, frontend dist dans `ui/dist`

### 7.3 CI

Le workflow `.github/workflows/build-desktop.yml` se declenche sur les tags `v*` et produit les artefacts pour macOS (macos-latest) et Linux (ubuntu-latest).

### 7.4 Installation

Voir la section "Installation application desktop" dans [INSTALL](./INSTALL.md).

---

## 8. Coexistence CLI + Desktop

Les deux modes d'acces (CLI et Desktop) partagent le meme runtime :

- **API REST** — TCP `localhost:7771` (Tauri commandes + SSE)
- **Unix socket** — `/tmp/apollia.sock` (CLI `apollia-os status`)
- **EventBus** — Broadcast Tokio partage (SSE dashboard + Tauri SSE stores)

Un seul processus, un seul Supervisor, un seul jeu d'acteurs Tokio. Pas de conflit de port ou de socket.

---

## 9. Attributs de test

Elements `data-testid` sur les composants principaux pour les tests e2e :

- Layout : `app-loading`, `app-main`, `sidebar`, `sidebar-logo`, `sidebar-nav`
- Navigation : `nav-agents`, `nav-tasks`, `nav-approvals`, `nav-llm`, `nav-triggers`, `nav-pipelines`, `nav-memory`, `nav-notifications`, `nav-observability`, `nav-settings`
- Groupes : `nav-group-operations`, `nav-group-infrastructure`, `nav-group-donnees`
- Badges : `approvals-badge`, `connection-status`, `connection-dot`
- Contenu : `agents-header`, `register-agent-btn`, `agents-grid`

---

## 10. Decisions architecturales

- **ADR-027** — Processus unique Tauri + runtime embarque
- **ADR-028** — Frontend Svelte : UX first, UI sprint dedie
- **ADR-029** — Settings lecture seule (round-trip TOML detruirait les commentaires)
- **ADR-033** — Config operateur SQLite : separation structurel (TOML) / operationnel (SQLite)
