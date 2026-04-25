# Architecture — Machines d'État — Apollia OS

> Les deux machines d'état indépendantes d'Apollia OS : ProcessState (agent) et TaskState (tâche), leurs transitions valides, et la règle d'or qui les distingue.
> Public cible : développeur d'agent, contributeur Rust

---

## Vue d'ensemble

Apollia OS maintient deux machines d'état totalement indépendantes. Les confondre est la source d'erreur la plus commune dans l'interaction avec le runtime.

- **`ProcessState`** : état du processus agent en tant que service opérationnel. Géré par `AgentRegistry`.
- **`TaskState`** : état d'une tâche individuelle. Géré par `TaskRouter` + ORIA Engine.

Un agent peut être `ACTIVE` (ProcessState) tout en traitant une tâche `working` (TaskState), ou `ACTIVE` avec zéro tâche. Un agent `DEGRADED` peut quand même traiter des tâches. Un agent `STOPPING` refuse de nouvelles tâches mais laisse les tâches en cours se terminer.

---

## ProcessState — Machine d'état du processus

```
INITIALIZING ──────────────► ACTIVE ──────────────► STOPPING ──► STOPPED
     │                         │                        ▲
     │ (échec)                 ▼                        │
     └──────────────► STOPPED  DEGRADED ────────────────┘
                               │
                               └────────────────────────► STOPPING (erreur fatale)
```

### Transitions valides

| De | Vers | Déclencheur |
|---|---|---|
| `INITIALIZING` | `ACTIVE` | Tous les `tools_required` résolus, SQLite ouvert, `on_start()` OK |
| `INITIALIZING` | `STOPPED` | Outil requis manquant, manifest invalide, `on_start()` erreur |
| `ACTIVE` | `DEGRADED` | Outil optionnel manquant ou circuit breaker déclenché |
| `ACTIVE` | `STOPPING` | `apollia-os agent stop` ou `ShutdownController` |
| `DEGRADED` | `ACTIVE` | Outil optionnel redevenu disponible (circuit breaker rétabli) |
| `DEGRADED` | `STOPPING` | `apollia-os agent stop` |
| `STOPPING` | `STOPPED` | Toutes les tâches drainées OU timeout 30s atteint |

### Signification de chaque état

**`INITIALIZING`** : phase de démarrage. Le runtime résout les outils, ouvre la base SQLite, appelle `on_start()` si défini. Aucune tâche n'est acceptée. Toute erreur détectable ici est détectée ici (Principe #4 — Fail Fast).

**`ACTIVE`** : l'agent est opérationnel. Il accepte les nouvelles tâches jusqu'à sa limite de concurrence (`max_concurrent_tasks`).

**`DEGRADED`** : l'agent fonctionne mais des `tools_optional` sont indisponibles. Les tâches sont acceptées. L'opérateur est notifié. Le CLI affiche un warning `⚠`.

**`STOPPING`** : l'agent ne peut plus accepter de nouvelles tâches. Les tâches existantes continuent jusqu'à completion ou timeout 30s.

**`STOPPED`** : arrêt propre. L'acteur est retiré de l'`AgentRegistry`. `on_stop()` a été appelé.

### Observation via CLI

```bash
$ apollia-os agent list
  NAME          STATUS        TASKS    VERSION    NOTE
  hello-agent   ACTIVE        1/1      1.0.0
  devis-agent   DEGRADED      0/2      1.0.0      ⚠ mcp:filesystem indisponible
  old-agent     STOPPING      1/1      0.9.0      drain en cours...
```

```bash
$ apollia-os agent info hello-agent
  ProcessState : ACTIVE
  Tâches       : 1 / 1 (max concurrent)
  Uptime       : 3h 42m
  Outils       : file_io ✓  bash_executor ✓
  Mémoire      : hello-agent-ns (42 épisodes, 8 faits)
```

---

## TaskState — Machine d'état des tâches

Alignée sur A2A TaskState (Google Agent-to-Agent Protocol).

```
submitted ──► working ──► completed
                │
                ├──► input_required ──► working   (POST /resume { approved: true })
                │         │
                │         ├──► failed              (POST /resume { approved: false })
                │         └──► canceled            (TimeoutWatcher > 24h)
                ├──► failed
                └──► canceled
```

### Transitions valides

| De | Vers | Déclencheur |
|---|---|---|
| `submitted` | `working` | `ExecutionCoordinator` accepte et démarre la tâche |
| `submitted` | `canceled` | `apollia-os task cancel` avant démarrage |
| `working` | `completed` | Agent retourne `AIPResult.completed` |
| `working` | `failed` | Agent retourne `AIPResult.failed` ou exception Python |
| `working` | `failed` | `StepBudget` épuisé (steps, tool_calls, ou wall_clock) |
| `working` | `canceled` | `apollia-os task cancel` + signal à l'agent |
| `working` | `input_required` | Agent retourne `AIPResult.input_required(prompt, context)` — HITL |
| `input_required` | `working` | `POST /api/v1/tasks/{id}/resume { approved: true }` — ORIA rappelle `agent.run` |
| `input_required` | `failed` | `POST /api/v1/tasks/{id}/resume { approved: false }` → `AIPResult::failed("REJECTED")` |
| `input_required` | `canceled` | `TimeoutWatcher` (scan 60s, expire après `input_required_timeout`, défaut 24h) |

### Observation via CLI

```bash
$ apollia-os task list
  TASK_ID     AGENT         STATUS       STARTED     DURATION
  t-abc123    hello-agent   completed    14:32:01    0.3s
  t-def456    devis-agent   working      14:35:00    2m 14s
  t-ghi789    devis-agent   failed       14:30:00    1m 05s    BudgetExceeded

$ apollia-os task status t-def456
  TaskId    : t-def456
  Agent     : devis-agent
  Status    : working
  Steps     : 7 / 20
  ToolCalls : 12 / 40
  Elapsed   : 2m 14s / 300s max
  Input     : "Devis 10 licences Figma pour Acme"
```

### SSE streaming temps réel

```bash
# Suivre l'évolution d'une tâche
$ curl -N http://localhost:7771/api/v1/tasks/t-def456/stream
data: {"event":"TaskStarted","task_id":"t-def456","agent_id":"devis-agent"}
data: {"event":"StepCompleted","task_id":"t-def456","step":1}
data: {"event":"ToolCalled","task_id":"t-def456","tool":"file_io"}
data: {"event":"TaskCompleted","task_id":"t-def456","status":"completed"}
```

---

## La règle d'or

**Pourquoi cette règle existe :** les deux machines d'état sont indépendantes mais doivent rester cohérentes. Un agent `DEGRADED` (ex: outil optionnel manquant) peut encore traiter des tâches — il fonctionne en mode dégradé, pas arrêté. Un agent `STOPPING` attend que ses tâches en cours finissent (drain gracieux). Un agent `STOPPED` ne peut avoir aucune tâche active.

**`input_required` et HITL :** quand une tâche passe en `input_required`, elle est **pausée** (pas verrouillée, pas terminée). L'agent ne s'exécute plus pour cette tâche. L'opérateur doit fournir une réponse via `apollia-os task resume <id>` ou l'API REST, puis le runtime relance `run()` avec `task["is_resumed"] = True` et la réponse dans `task["input_response"]`.

```
ProcessState  │  TaskState possible
──────────────┼──────────────────────────────────────────
INITIALIZING  │  (aucune tâche)
ACTIVE        │  submitted, working, input_required
DEGRADED      │  submitted, working, input_required
STOPPING      │  working (drain en cours, no new submitted)
STOPPED       │  (aucune tâche)
```

---

## Implémentation Rust

```rust
// apollia-core/src/process.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    Initializing,
    Active,
    Degraded,
    Stopping,
    Stopped,
}

impl ProcessState {
    pub fn can_transition_to(&self, next: &ProcessState) -> bool {
        matches!(
            (self, next),
            (ProcessState::Initializing, ProcessState::Active)
                | (ProcessState::Initializing, ProcessState::Stopped)
                | (ProcessState::Active, ProcessState::Degraded)
                | (ProcessState::Active, ProcessState::Stopping)
                | (ProcessState::Degraded, ProcessState::Active)
                | (ProcessState::Degraded, ProcessState::Stopping)
                | (ProcessState::Stopping, ProcessState::Stopped)
        )
    }
}
```

```rust
// apollia-core/src/result.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Submitted,
    Working,
    Completed,
    Failed,
    InputRequired,
    Canceled,
}
```

---

## Voir aussi

- [Architecture Vue d'ensemble](./Architecture-Vue-Ensemble) — les deux machines d'état expliquées dans leur contexte
- [Architecture Modèle Acteur](./Architecture-Modele-Acteur) — AgentRegistry et TaskRouter
- [Briques ORIA Engine](./Briques-ORIA-Engine) — StepBudget et les transitions TaskState
- [ADR-004](../adr/ADR-004-deux-modes-execution-oria) — pourquoi deux machines d'état séparées
