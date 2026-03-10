# Pipelines Engine — Orchestration Multi-Agent

> *Coordinatez plusieurs agents indépendants via un pipeline TOML déclaratif. Fan-out, fan-in, conditions, fallback, HITL intégré — sans une ligne de code.*

---

## 1. Vue d'ensemble

La crate `apollia-pipelines` (Sprint 12) permet de décrire un workflow multi-agent directement dans `apollia.toml`. Un **pipeline** est un graphe orienté acyclique (DAG) de **steps**, où chaque step soumet une tâche à un agent déjà démarré dans le runtime.

```
[[pipelines]]
id = "traitement-facture"
description = "OCR → validation → comptabilisation → archivage"

[[pipelines.steps]]
id    = "ocr"
agent = "ocr-agent"
input = "{{trigger.payload}}"

[[pipelines.steps]]
id         = "validation"
agent      = "validation-agent"
input      = "{{steps.ocr.output}}"
depends_on = ["ocr"]
on_failure = "fallback"
condition  = { when = "contains", field = "steps.ocr.output", value = "PDF" }

[[pipelines.steps]]
id           = "validation-fallback"
agent        = "fallback-agent"
input        = "{{steps.ocr.output}}"
fallback_for = "validation"
```

---

## 1.1 Diagrammes

**Topologie d'un pipeline** — DAG avec fan-out, fan-in, condition et fallback :

![Topologie pipeline](../diagrams/component-pipeline-topology.svg)

**Machine d'état** — cycle de vie d'un `PipelineRun` et d'un `StepRun` :

![Machines d'état pipeline](../diagrams/state-pipeline.svg)

---

## 2. Concepts fondamentaux

### 2.1 Types publics (`apollia-pipelines::types`)

```rust
/// Définition statique d'un pipeline (lue depuis apollia.toml, immuable ensuite).
pub struct PipelineDefinition {
    pub id: PipelineId,           // "traitement-facture"
    pub description: String,
    pub on_failure: GlobalFailurePolicy,  // Fail | Continue
    pub steps: Vec<PipelineStepDef>,
}

/// Définition d'un step dans le pipeline.
pub struct PipelineStepDef {
    pub id: StepId,                    // unique dans le pipeline
    pub agent: String,                 // nom de l'agent dans AgentRegistry
    pub input: String,                 // template — "{{steps.x.output}}"
    pub depends_on: Vec<StepId>,       // dépendances amont
    pub on_failure: StepFailurePolicy, // Fail | Skip | Fallback
    pub condition: Option<StepCondition>,
    pub fallback_for: Option<StepId>,  // si défini : step inactif par défaut
}

/// État d'exécution d'un run (persisté en SQLite).
pub struct PipelineRun {
    pub run_id: RunId,               // "r-3f7a2b9c"
    pub pipeline_id: PipelineId,
    pub trigger_id: Option<String>,
    pub status: PipelineStatus,      // Running | WaitingApproval | Completed | Failed
    pub step_runs: HashMap<StepId, StepRun>,
    pub trigger_payload: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}
```

### 2.2 Statuts d'un run

| Statut | Description |
|---|---|
| `Running` | Au moins un step est en cours ou en attente |
| `WaitingApproval { step_id, task_id }` | Pipeline suspendu — step attend une approbation HITL |
| `Completed` | Tous les steps ont terminé avec succès (ou ont été skippés) |
| `Failed { step_id, reason }` | Un step a échoué avec `on_failure = fail` |

### 2.3 Statuts d'un step

| Statut | Description |
|---|---|
| `Pending` | En attente de ses dépendances |
| `Running` | Tâche soumise au `TaskRouter` |
| `WaitingApproval` | Tâche suspendue en attente HITL |
| `Completed` | Tâche terminée avec succès — `output` populé |
| `Failed` | Tâche en erreur — `error` populé |
| `Skipped` | Step sauté (`on_failure = skip` ou condition false) |
| `FallbackActive` | Step remplacé par son fallback |

---

## 3. Architecture

**Séquence d'exécution complète** — de la soumission à la completion (layers topologiques, fan-out) :

![Séquence exécution pipeline](../diagrams/seq-pipeline-execution.svg)

**Séquence HITL** — suspension d'un step et reprise après approbation :

![Séquence HITL pipeline](../diagrams/seq-pipeline-hitl.svg)

### 3.1 Composants

```
apollia-pipelines crate
├── types.rs         — PipelineDefinition, PipelineRun, StepRun, enums
├── repository.rs    — PipelineRepository (SQLite sync, migration 006)
├── template.rs      — TemplateContext, render() — {{steps.x.output}}
├── topo.rs          — topological_layers() via Kahn BFS
├── condition.rs     — evaluate_condition() — 5 opérateurs
├── executor.rs      — PipelineExecutor, StepResult, TaskSubmitter trait
└── engine.rs        — PipelineEngine acteur Tokio, PipelineEngineHandle
```

### 3.2 Acteur Tokio

```
PipelineEngineHandle (Clone + Send + Sync)
  │  mpsc::channel(256)
  ▼
PipelineEngine (Tokio task)
  ├── pipelines: HashMap<PipelineId, PipelineDefinition>  — chargées depuis apollia.toml
  ├── repo: Arc<Mutex<PipelineRepository>>               — SQLite pipelines.db
  ├── submitter: Arc<dyn TaskSubmitter>                  — injecté (ADR-015)
  └── event_bus: EventBusSender                          — observe ShutdownRequested
        │
        └── spawn PipelineExecutor (détaché) par run
```

`PipelineEngine` est démarré en **position 8** dans le `Supervisor` (après `NotificationEngine`). Au démarrage, il reprend automatiquement les runs dont le statut SQLite était `running`.

### 3.3 Exécuteur et topologie

`PipelineExecutor` exécute les steps couche par couche (layers topologiques calculés par `topological_layers()`) :

```
Layer 0 : [ocr]                    → soumis simultanément (FuturesUnordered)
Layer 1 : [validation, extraction] → soumis quand Layer 0 est terminé
Layer 2 : [comptabilisation]       → soumis quand Layer 1 est terminé
```

Pour chaque step, l'exécuteur :
1. Évalue la condition (`condition.rs`) — skip si false
2. Vérifie si le step est un fallback inactif — skip si oui
3. Rend l'input via `TemplateContext` — résout `{{steps.ocr.output}}`
4. Soumet la tâche au `TaskRouter` via `TaskSubmitter`
5. Attend `TaskCompleted` ou `TaskFailed` sur l'EventBus
6. Persiste le résultat dans SQLite via `PipelineRepository`

### 3.4 Intégration EventBus

Nouveaux variants `RuntimeEvent` ajoutés dans `apollia-core` (STORY-116) :

| Événement | Émis par | Données |
|---|---|---|
| `PipelineStarted { run_id, pipeline_id, trigger_id, step_count }` | PipelineEngine | Identifiants + nb steps |
| `PipelineStepStarted { run_id, step_id, task_id, agent }` | PipelineExecutor | Step soumis + agent cible |
| `PipelineStepCompleted { run_id, step_id }` | PipelineExecutor | Step terminé |
| `PipelineStepFailed { run_id, step_id, reason, on_failure }` | PipelineExecutor | Raison + politique appliquée |
| `PipelineStepSkipped { run_id, step_id, reason }` | PipelineExecutor | Raison du skip |
| `PipelineSuspended { run_id, step_id, task_id }` | PipelineExecutor | HITL en attente |
| `PipelineResumed { run_id, step_id }` | PipelineExecutor | Après approbation |
| `PipelineCompleted { run_id, pipeline_id, duration_ms }` | PipelineExecutor | Fin réussie + durée |
| `PipelineFailed { run_id, pipeline_id, step_id, reason }` | PipelineExecutor | Fin en erreur |

---

## 4. Configuration `apollia.toml`

### 4.1 Syntaxe de base

```toml
[[pipelines]]
id          = "mon-pipeline"
description = "Description courte affichée dans pipeline list"

# Politique globale si un step échoue sans on_failure local
# "fail" (défaut) | "continue"
on_failure = "fail"

[[pipelines.steps]]
id    = "step-1"
agent = "nom-agent-enregistre"
input = "{{trigger.payload}}"

[[pipelines.steps]]
id         = "step-2"
agent      = "autre-agent"
input      = "Traite : {{steps.step-1.output}}"
depends_on = ["step-1"]
on_failure = "skip"   # "fail" | "skip" | "fallback"
```

### 4.2 Conditions de step

Un step peut être conditionnel — il est **skipped** si la condition est fausse :

```toml
[[pipelines.steps]]
id         = "enrichissement"
agent      = "crm-agent"
input      = "{{steps.validation.output}}"
depends_on = ["validation"]

[pipelines.steps.condition]
when  = "contains"       # contains | equals | starts_with | ends_with | regex
field = "steps.validation.output"
value = "VALIDE"
```

| Opérateur | Comportement |
|---|---|
| `contains` | Le champ contient la valeur en sous-chaîne |
| `equals` | Égalité stricte |
| `starts_with` | Le champ commence par la valeur |
| `ends_with` | Le champ se termine par la valeur |
| `regex` | La valeur est un regex interprété sur le champ |

### 4.3 Fallback de step

Quand `on_failure = "fallback"`, le step désigne un autre step comme repli. Le fallback est inactif par défaut et s'active si son référent échoue :

```toml
[[pipelines.steps]]
id         = "validation"
agent      = "validation-agent"
input      = "{{steps.ocr.output}}"
depends_on = ["ocr"]
on_failure = "fallback"

[[pipelines.steps]]
id           = "validation-fallback"
agent        = "manual-review-agent"
input        = "Validation manuelle requise : {{steps.ocr.output}}"
depends_on   = ["ocr"]
fallback_for = "validation"   # sera activé si "validation" échoue
```

### 4.4 Triggers → Pipelines

Un trigger peut déclencher un pipeline plutôt qu'un agent individuel :

```toml
[[triggers]]
id      = "import-factures"
enabled = true
on_busy = "queue"

[triggers.source]
type   = "file_watch"
path   = "~/factures/entrant/"
events = ["create"]

# Champ exclusif avec "agent" — l'un ou l'autre, jamais les deux
pipeline = "traitement-facture"

[triggers.input]
text = "{{filepath}}"   # transmis comme trigger_payload au pipeline
```

### 4.5 Variables de template

| Variable | Disponible | Description |
|---|---|---|
| `{{trigger.payload}}` | Toujours | Payload transmis au déclenchement (fichier, webhook body, etc.) |
| `{{pipeline.id}}` | Toujours | Identifiant du pipeline |
| `{{pipeline.run_id}}` | Toujours | Identifiant unique du run |
| `{{steps.<id>.output}}` | Après complétion du step `<id>` | Sortie textuelle du step |

Les variables non résolues (step non encore terminé, variable inconnue) sont remplacées par une chaîne vide — le pipeline ne s'arrête pas.

### 4.6 Validation au démarrage

À chaque `apollia-os start`, Apollia valide chaque pipeline défini :

- `step_id` unique dans le pipeline
- Tous les `depends_on` référencent des steps existants
- Tout `fallback_for` référence un step existant
- Pas de cycle dans le graphe de dépendances (DFS)
- Les agents référencés existent dans la configuration (warning si absent)

---

## 5. Persistance SQLite

Les runs et steps sont persistés dans `~/.apollia/pipelines.db` (migration `006_pipeline_tables.sql`).

```sql
CREATE TABLE pipeline_runs (
    run_id      TEXT PRIMARY KEY,
    pipeline_id TEXT NOT NULL,
    trigger_id  TEXT,
    status_json TEXT NOT NULL,   -- PipelineStatus sérialisé en JSON
    trigger_payload TEXT,
    started_at  TEXT NOT NULL,
    ended_at    TEXT
);

CREATE TABLE pipeline_step_runs (
    run_id      TEXT NOT NULL,
    step_id     TEXT NOT NULL,
    task_id     TEXT,
    status      TEXT NOT NULL,   -- StepRunStatus snake_case
    output      TEXT,
    error       TEXT,
    started_at  TEXT,
    ended_at    TEXT,
    PRIMARY KEY (run_id, step_id)
);
```

**Reprise après restart :** au démarrage du `PipelineEngine`, `find_running_runs()` charge tous les runs avec `status = "running"` et relance leurs executors. Les steps déjà `completed` ou `failed` sont ignorés ; les steps `running` sont remis en `pending` pour être re-soumis.

---

## 6. HITL dans les pipelines

Quand un step produit `AIPResult.input_required()`, le pipeline se suspend automatiquement :

1. `PipelineExecutor` reçoit `StepResult::InputRequired { task_id }` depuis l'EventBus
2. Le run passe en `PipelineStatus::WaitingApproval { step_id, task_id }`
3. `PipelineSuspended` est émis sur l'EventBus → notification desktop/webhook
4. L'opérateur approuve via `apollia-os task resume <task_id> --approve`
5. `TaskResumed` est émis sur l'EventBus
6. `PipelineExecutor` reçoit l'événement, relance le step, le pipeline reprend

```bash
# Voir les pipelines suspendus
$ apollia-os pipeline list

# Approuver
$ apollia-os task resume t-abc123 --approve

# Rejeter (le step est marqué Failed, on_failure s'applique)
$ apollia-os task resume t-abc123 --reject --reason "Montant incorrect"
```

---

## 7. Interface CLI

```bash
# Lister les pipelines configurés
$ apollia-os pipeline list

# Déclencher un pipeline manuellement
$ apollia-os pipeline run traitement-facture --input "facture.pdf"
  ✔ Pipeline run démarré : r-3f7a2b9c

# Déclencher et suivre la progression (polling par défaut — --detach pour fire-and-forget)
$ apollia-os pipeline run traitement-facture --input "facture.pdf"
  [10:01:32]  ⟿ [ocr] running
  [10:01:45]  ✔ [ocr] completed
  [10:01:45]  ⟿ [validation] running
  [10:01:47]  ✔ [validation] completed
  [10:01:47]  ⟿ [comptabilite] running
  [10:01:48]  ⏸ [comptabilite] waiting_approval

# Déclencher sans attendre la fin (fire-and-forget)
$ apollia-os pipeline run traitement-facture --input "facture.pdf" --detach
  ● traitement-facture › démarré (run r-3f7a2b9c)

# Voir l'historique des runs
$ apollia-os pipeline runs traitement-facture
  RUN ID       STATUT        DÉMARRÉ          DURÉE
  r-3f7a2b9c   Completed     2026-03-10 10:01   1m23s
  r-2e6b1a8b   Failed        2026-03-09 14:32   0m08s

# Inspecter un run spécifique
$ apollia-os pipeline status r-3f7a2b9c
  Pipeline : traitement-facture
  Run      : r-3f7a2b9c
  Statut   : Completed
  Démarré  : 2026-03-10 10:01:32
  Terminé  : 2026-03-10 10:02:55

  STEP              STATUT      DURÉE
  ocr               Completed   13.2s
  validation        Completed    1.8s
  comptabilisation  Completed    5.1s
  archivage         Completed    2.3s
```

---

## 8. API REST

Voir [API HTTP Reference](./API-HTTP-Reference) — section Pipelines pour la référence complète.

| Méthode | Route | Description |
|---|---|---|
| `GET` | `/api/v1/pipelines` | Liste tous les pipelines configurés |
| `POST` | `/api/v1/pipelines/{id}/run` | Démarre un run |
| `GET` | `/api/v1/pipelines/{id}/runs` | Historique des runs |
| `GET` | `/api/v1/pipelines/{id}/runs/{run_id}` | État d'un run |

---

## 9. Notifications

Trois événements pipeline sont mappés vers le système de notifications (configurable) :

| Événement notification | RuntimeEvent | Sévérité |
|---|---|---|
| `pipeline.completed` | `PipelineCompleted` | Info |
| `pipeline.failed` | `PipelineFailed` | Error |
| `pipeline.suspended` | `PipelineSuspended` | Warning |

```toml
[notifications]
events = ["pipeline.suspended", "pipeline.failed", "task.input_required"]

[[notifications.channels]]
id   = "desktop"
type = "desktop"
```

---

## 10. Implémentation — détails Rust

### TaskSubmitter trait (ADR-015)

`PipelineExecutor` ne dépend pas directement du `TaskRouter` — il utilise le trait `TaskSubmitter` injecté :

```rust
#[async_trait]
pub trait TaskSubmitter: Send + Sync {
    async fn submit_task(&self, agent: &str, input: &str) -> Result<String, ExecutorError>;
}
```

Cela permet de tester l'executor avec un mock sans runtime complet, selon le même pattern que `ToolExecutor` (ADR-015) et `AgentRunner` (ADR-016).

### Exécution concurrente par layer

```rust
// Dans PipelineExecutor — fan-out d'un layer complet
let mut futures = FuturesUnordered::new();
for step_id in current_layer {
    futures.push(self.execute_step(step_id, &context));
}
while let Some(result) = futures.next().await {
    // traitement de chaque StepResult
}
```

---

## Voir aussi

- [Config apollia.toml](./Config-apollia-toml) — section `[[pipelines]]` complète
- [API HTTP Reference](./API-HTTP-Reference) — endpoints `/api/v1/pipelines`
- [Briques Triggers](./Briques-Triggers) — déclenchement automatique de pipelines
- [Briques Notifications](./Briques-Notifications) — canal `pipeline.suspended`
- [Briques Runtime Core](./Briques-Runtime-Core) — acteur `PipelineEngine` dans le Supervisor
- [Architecture Modèle Acteur](./Architecture-Modele-Acteur) — pattern Handle + mpsc
- [ADR-025](../adr/ADR-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md) — décision architecture pipelines
