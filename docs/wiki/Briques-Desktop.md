# Application Desktop — Tauri v2 + Runtime embarque

> *L'application desktop Apollia OS embarque le runtime complet dans un processus unique. Double-clic → fenetre → agents, tasks, HITL approvals temps reel.*

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
  └── tauri::Builder
        ├── .manage(RuntimeHandle)     ← etat partage
        ├── .invoke_handler(commands)  ← 9 commandes IPC
        └── .run()                     ← ouvre la WebView
```

**Communication frontend ↔ runtime :**

| Type | Mecanisme | Exemples |
|---|---|---|
| Mutations ponctuelles | Commandes Tauri `#[tauri::command]` | `start_agent`, `submit_task`, `resume_task` |
| Flux temps reel | SSE EventBus (`localhost:7771/api/v1/dashboard/stream`) | Agents state, tasks status, HITL pending |

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
│   ├── main.rs                ← entree Tauri + init_embedded()
│   └── commands/
│       ├── mod.rs             ← helpers HTTP (http_get_json, http_post_json)
│       ├── agents.rs          ← list_agents, start_agent, stop_agent
│       ├── tasks.rs           ← list_tasks, submit_task, get_task_timeline
│       └── hitl.rs            ← list_pending_approvals, list_resolved_approvals, resume_task
└── ui/                        ← application Svelte 5
    ├── package.json
    ├── vite.config.ts
    └── src/
        ├── App.svelte
        ├── lib/
        │   ├── types.ts       ← AgentStatus, TaskSummary, PendingApproval, TimelineEvent
        │   └── stores/        ← sse.ts, agents.ts, tasks.ts, hitl.ts, navigation.ts
        ├── components/
        │   ├── layout/        ← Sidebar.svelte, Main.svelte
        │   ├── agents/        ← AgentCard.svelte, AgentLogs.svelte
        │   ├── tasks/         ← TaskList.svelte, TaskDetail.svelte, TaskTimeline.svelte
        │   ├── hitl/          ← ApprovalCard.svelte, ApprovalHistory.svelte
        │   └── ui/            ← Button, Card, Badge, Sheet, Separator (bits-ui wrapped)
        └── routes/            ← Agents.svelte, Tasks.svelte, Approvals.svelte
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
    // Champs optionnels selon la configuration
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

9 commandes exposees au frontend Svelte via `#[tauri::command]` :

### Agents

| Commande | Parametres | Retour |
|---|---|---|
| `list_agents` | — | `Vec<AgentInfo>` |
| `start_agent` | `path: String` | `Result<String, String>` (agent_id) |
| `stop_agent` | `agent_id: String` | `Result<(), String>` |

### Tasks

| Commande | Parametres | Retour |
|---|---|---|
| `list_tasks` | `filter: Option<TaskFilter>` | `Result<Vec<TaskSummary>, String>` |
| `submit_task` | `agent_id: String, input: String` | `Result<String, String>` (task_id) |
| `get_task_timeline` | `task_id: String` | `Result<Vec<Value>, String>` |

### HITL

| Commande | Parametres | Retour |
|---|---|---|
| `list_pending_approvals` | — | `Result<Vec<PendingApproval>, String>` |
| `list_resolved_approvals` | `limit: Option<usize>, days: Option<u64>` | `Result<Vec<ResolvedApproval>, String>` |
| `resume_task` | `task_id, approved, reason` | `Result<(), String>` |

---

## 4. Frontend Svelte (ADR-028)

### 4.1 Stack

- **Svelte 5** (runes) + **Vite 6** — framework reactif leger
- **bits-ui** — composants headless accessibles (Button, Card, Badge, Sheet)
- **Tailwind CSS 3.4** — utilitaires CSS + design tokens
- **@tauri-apps/api** — bridge IPC Tauri

### 4.2 Navigation

Store Svelte `currentRoute` (`writable<'agents' | 'tasks' | 'approvals'>`). Rendu conditionnel `{#if}` dans `App.svelte`. Pas de router externe.

### 4.3 SSE et stores reactifs

Le store `sse.ts` etablit une connexion SSE vers `localhost:7771/api/v1/dashboard/stream` avec reconnexion automatique (backoff exponentiel 1s → 30s max).

5 stores reactifs mis a jour en temps reel :

| Store | Type | Source |
|---|---|---|
| `agents` | `writable<AgentStatus[]>` | SSE channel `agents` |
| `tasks` | `writable<TaskSummary[]>` | SSE channel `tasks` |
| `pendingApprovals` | `writable<PendingApproval[]>` | SSE channel `hitl` |
| `connectionStatus` | `writable<ConnectionStatus>` | SSE connection state |
| `currentRoute` | `writable<Route>` | Navigation user |

3 stores derives : `activeAgentCount`, `runningTasks`, `pendingCount`.

### 4.4 Vues

**Agents** — Liste temps reel avec badges d'etat (ACTIVE/vert, DEGRADED/orange, STOPPED/gris). File picker natif Tauri pour enregistrer un agent `.py`. Drawer avec les 20 dernieres taches de l'agent.

**Tasks** — Liste filtrable par onglets (All/Running/Completed/Failed/Pending). Detail avec input/output complets. Timeline interactive avec 5 types d'evenements (Transition, Tool, LLM, HITL, Done) — repose sur l'API Timeline Sprint 13.

**Approvals** — Cartes d'approbation avec compteur live (Xm Ys), prompt complet, contexte JSON depliable, boutons Approuver/Rejeter avec dialogs de confirmation. Historique des 20 dernieres approbations resolues (7 jours).

---

## 5. Build et packaging (STORY-142)

### 5.1 Formats de sortie

| Plateforme | Format | Commande |
|---|---|---|
| macOS | `.dmg` + `.app` | `cargo tauri build` |
| Linux | `.AppImage` + `.deb` | `cargo tauri build` |

### 5.2 CI

Le workflow `.github/workflows/build-desktop.yml` se declenche sur les tags `v*` et produit les artefacts pour macOS (macos-latest) et Linux (ubuntu-latest).

### 5.3 Installation

Voir la section "Installation application desktop" dans [INSTALL](./INSTALL.md).

---

## 6. Coexistence CLI + Desktop

Les deux modes d'acces (CLI et Desktop) partagent le meme runtime :

- **API REST** — TCP `localhost:7771` (Tauri commandes + SSE)
- **Unix socket** — `/tmp/apollia.sock` (CLI `apollia-os status`)
- **EventBus** — Broadcast Tokio partage (SSE dashboard + Tauri SSE stores)

Un seul processus, un seul Supervisor, un seul jeu d'acteurs Tokio. Pas de conflit de port ou de socket.

---

## 7. Decisions architecturales

- **ADR-027** — Processus unique Tauri + runtime embarque
- **ADR-028** — Frontend Svelte : UX first, UI sprint dedie
