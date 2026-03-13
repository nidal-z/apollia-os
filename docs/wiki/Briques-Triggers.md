# Triggers Engine — Déclenchement automatique des agents

> *La crate `apollia-triggers` expose un moteur déclaratif pour déclencher des agents automatiquement via des règles TOML : cron, interval, file watch, webhooks HMAC-SHA256.*

---

## 1. Vue d'ensemble

Le `TriggerEngine` est un acteur Tokio positionné en **position 6** dans la séquence de démarrage du Supervisor (après le `LlmRouter`). Il gère un ensemble de `TriggerDefinition` chargées depuis `apollia.toml` et déclenche des tâches vers le `TaskRouter` selon les événements reçus.

```
apollia.toml [[triggers]]
         │
         ▼ parsing + validation au démarrage
 ┌──────────────────┐
 │  TriggerEngine   │ ← acteur Tokio, position 6 Supervisor
 │  (acteur Tokio)  │
 └──────┬───────────┘
        │ canal mpsc<TriggerEvent>
        │
   ┌────▼──────────────────────────────────────┐
   │  Sources indépendantes (JoinHandle<()>)    │
   │                                           │
   │  ┌──────────────┐  Tier 1 (Timer)         │
   │  │ CronTrigger  │  calcul next occurrence  │
   │  │ IntervalTrig │  fires périodiques       │
   │  │ OneshotTrig  │  fire unique             │
   │  └──────────────┘                         │
   │                                           │
   │  ┌──────────────┐  Tier 2 (File)          │
   │  │ FileWatchTrig│  notify v6 (inotify/    │
   │  │              │  kqueue/FSEvents)       │
   │  └──────────────┘                         │
   │                                           │
   │  ┌──────────────┐  Tier 3 (Réseau)        │
   │  │ Webhook      │  POST /webhooks/{id}    │
   │  │ (axum route) │  HMAC-SHA256            │
   │  └──────────────┘                         │
   └───────────────────────────────────────────┘
        │
        ▼ InputTemplate.render() + OnBusyPolicy
 ┌──────────────┐
 │  TaskRouter  │ ← soumission AIPTask
 └──────────────┘
```

*Voir le diagramme de séquence complet : [Séquence — Trigger Fire](./Architecture-Vue-Ensemble#diagrammes)*

---

## 2. Configuration `apollia.toml`

Les triggers sont déclarés comme un tableau TOML `[[triggers]]`. Chaque entrée peut être de type `cron`, `interval`, `oneshot`, `file_watch`, ou `webhook`.

### 2.1 Trigger Cron

```toml
[[triggers]]
id          = "rapport-hebdomadaire"
agent       = "rapport-agent"
enabled     = true
on_busy     = "queue"       # queue | drop | error

[triggers.source]
type        = "cron"
schedule    = "0 8 * * MON" # Chaque lundi à 8h

[triggers.input]
text        = "Génère le rapport de la semaine {{week_iso}}"
```

### 2.2 Trigger Interval

```toml
[[triggers]]
id      = "check-inbox"
agent   = "mail-agent"
enabled = true
on_busy = "drop"

[triggers.source]
type     = "interval"
every    = "30m"      # 30m | 1h | 6h | 1d
```

### 2.3 Trigger FileWatch

```toml
[[triggers]]
id      = "import-csv"
agent   = "import-agent"
enabled = true
on_busy = "queue"

[triggers.source]
type   = "file_watch"
path   = "~/imports/"
events = ["create"]   # create | modify | delete | any

[triggers.input]
text = "Importe le fichier {{filename}} ({{size_bytes}} octets)"
```

### 2.4 Trigger Webhook

```toml
[[triggers]]
id      = "github-push"
agent   = "deploy-agent"
enabled = true
on_busy = "error"

[triggers.source]
type   = "webhook"
secret = "un-secret-robuste-min-32-caracteres"
```

Appel depuis l'extérieur :
```bash
$ curl -X POST http://localhost:7771/webhooks/github-push \
  -H "X-Apollia-Signature: sha256=<hmac_hex>" \
  -H "Content-Type: application/json" \
  -d '{"ref": "refs/heads/main"}'
```

---

## 3. Types fondamentaux

```rust
// apollia-triggers/src/types.rs

pub struct TriggerDefinition {
    pub id: TriggerId,               // identifiant unique
    pub agent: String,               // nom de l'agent cible
    pub enabled: bool,
    pub on_busy: OnBusyPolicy,
    pub source: TriggerSourceConfig,
    pub input_template: InputTemplate,
}

pub enum OnBusyPolicy {
    Queue,   // soumet la tâche même si l'agent est occupé
    Drop,    // ignore le fire si l'agent est WORKING
    Error,   // émet TriggerError sur EventBus
}

pub enum TriggerSourceConfig {
    Cron    { schedule: String },
    Interval { every: Duration },
    Oneshot  { at: DateTime<Utc> },
    FileWatch { path: PathBuf, events: Vec<FileEventKind> },
    Webhook  { secret: String },
}

pub struct InputTemplate {
    pub text: String,  // peut contenir {{week_iso}}, {{filename}}, etc.
}

impl InputTemplate {
    /// Substitue les variables connues. Variable inconnue → chaîne vide.
    pub fn render(&self, payload: &TriggerPayload) -> String { ... }
}

pub struct TriggerPayload {
    pub fired_at: DateTime<Utc>,
    pub trigger_id: TriggerId,
    // Timer variables
    pub week_iso: Option<String>,
    pub date_iso: Option<String>,
    // File variables
    pub filename: Option<String>,
    pub filepath: Option<String>,
    pub size_bytes: Option<u64>,
    pub file_event: Option<String>,
    // Webhook variables
    pub webhook_body: Option<String>,
}
```

---

## 4. TriggerEngine — acteur Tokio

```rust
// apollia-triggers/src/engine.rs

pub struct TriggerEngineHandle {
    sender: Arc<mpsc::Sender<TriggerCommand>>,
}

impl TriggerEngineHandle {
    /// Clone + Send + Sync — injectable dans AppState<B>
    pub fn clone(&self) -> Self { ... }

    /// Déclenche immédiatement un trigger (test ou CLI fire)
    pub async fn fire_now(&self, id: &TriggerId) -> Result<(), TriggerEngineError>;

    /// Active/désactive un trigger sans redémarrer le runtime
    pub async fn enable(&self, id: &TriggerId)  -> Result<(), TriggerEngineError>;
    pub async fn disable(&self, id: &TriggerId) -> Result<(), TriggerEngineError>;

    /// Liste les définitions et statuts
    pub async fn list(&self) -> Result<Vec<TriggerStatus>, TriggerEngineError>;
    pub async fn get(&self, id: &TriggerId) -> Result<TriggerStatus, TriggerEngineError>;

    /// Relit apollia.toml, redémarre les sources modifiées (hot reload)
    pub async fn reload(&self, defs: Vec<TriggerDefinition>) -> Result<usize, TriggerEngineError>;
}

pub struct TriggerStatus {
    pub id: TriggerId,
    pub agent: String,
    pub enabled: bool,
    pub fire_count: u64,
    pub skip_count: u64,
    pub error_count: u64,
    pub last_fired_at: Option<DateTime<Utc>>,
}
```

### Politique `OnBusyPolicy`

| Politique | Comportement si agent WORKING |
|---|---|
| `Queue` | Soumet la tâche — elle sera exécutée quand le slot se libère |
| `Drop` | Ignore le fire, émet `TriggerSkipped` sur EventBus, incrémente `skip_count` |
| `Error` | Émet `TriggerError` sur EventBus, incrémente `error_count` |

---

## 5. Sources — implémentation

### 5.1 CronTrigger

```rust
// apollia-triggers/src/sources/cron.rs

/// Spawn un JoinHandle<()> indépendant.
/// Calcule la prochaine occurrence via `cron::Schedule::upcoming(Utc).next()`.
/// Si le délai d'attente est < 1s, émet un warning tracing.
pub fn spawn_cron(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()>;
```

### 5.2 IntervalTrigger

```rust
// apollia-triggers/src/sources/interval.rs

/// Formats acceptés : "30m", "1h", "6h", "1d", "300s"
pub fn parse_interval(s: &str) -> Result<Duration, TriggerDefinitionError>;

pub fn spawn_interval(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()>;
```

### 5.3 FileWatchTrigger

```rust
// apollia-triggers/src/sources/file_watch.rs

/// Bridge sync→async via std::sync::mpsc::recv_timeout(50ms).
/// Résout ~/ via dirs_next::home_dir().
/// Arrêt propre quand le canal tx est fermé (engine dropped).
pub fn spawn_file_watch(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()>;
```

### 5.4 Webhook (route axum)

```
POST /webhooks/:trigger_id
Header: X-Apollia-Signature: sha256=<hmac_sha256_hex>
Body:   JSON quelconque
```

Séquence de validation :
```
1. Vérifier que TriggerEngine existe (AppState) → sinon 503
2. Vérifier que trigger_id existe             → sinon 404
3. Calculer HMAC-SHA256(secret, body)
4. Comparer avec constant_time_eq              → sinon 401
5. Envoyer TriggerEvent au canal               → 200
```

---

## 6. Persistance SQLite

Chaque fire/skip/error est persisté dans la base de l'`AuditTrail`.

```sql
-- apollia-tools/migrations/003_trigger_tables.sql

CREATE TABLE trigger_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    trigger_id    TEXT    NOT NULL,
    status        TEXT    NOT NULL,  -- 'fired' | 'skipped' | 'error'
    task_id       TEXT,
    reason        TEXT,
    error_msg     TEXT,
    fired_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    payload_json  TEXT,              -- payload JSON du trigger (Sprint 13)
    dispatch_ms   INTEGER            -- latence fire → soumission tâche (Sprint 13)
);
CREATE INDEX idx_trigger_history_id  ON trigger_history(trigger_id);
CREATE INDEX idx_trigger_history_at  ON trigger_history(fired_at DESC);

CREATE TABLE trigger_state (
    trigger_id   TEXT PRIMARY KEY,
    fire_count   INTEGER NOT NULL DEFAULT 0,
    skip_count   INTEGER NOT NULL DEFAULT 0,
    error_count  INTEGER NOT NULL DEFAULT 0,
    last_fired_at TEXT
);
```

---

## 7. Événements EventBus

Sprint 9 ajoute 6 nouveaux variants `RuntimeEvent` dans `apollia-core` :

```rust
// apollia-core/src/events.rs

pub enum RuntimeEvent {
    // ... variants existants ...

    // Sprint 9 — Triggers
    TriggerFired    { trigger_id: TriggerId, agent: String, task_id: TaskId },
    TriggerSkipped  { trigger_id: TriggerId, reason: String },
    TriggerError    { trigger_id: TriggerId, error: String },
    TriggerEnabled  { trigger_id: TriggerId },
    TriggerDisabled { trigger_id: TriggerId },
    TriggersReloaded { count: usize },
}
```

Ces événements alimentent :
- Le dashboard (SSE stream `/api/v1/dashboard/stream`)
- Les logs observabilité (`tracing`)
- La persistance SQLite (`trigger_history`)

---

## 8. CLI `trigger`

```bash
# Lister tous les triggers (table formatée)
$ apollia-os trigger list

# Statut détaillé d'un trigger
$ apollia-os trigger status rapport-hebdomadaire

# Déclencher immédiatement (test)
$ apollia-os trigger fire rapport-hebdomadaire

# Activer / désactiver sans modifier apollia.toml
$ apollia-os trigger enable  check-inbox
$ apollia-os trigger disable check-inbox

# Historique depuis SQLite
$ apollia-os trigger logs rapport-hebdomadaire --last 20

# Hot reload (relit apollia.toml, redémarre les sources modifiées)
$ apollia-os trigger reload

# Mode JSON (--json global)
$ apollia-os trigger list --json
```

Exemple de sortie `trigger list` :

```
ID                    AGENT           TYPE      ENABLED  FIRES  SKIPS  LAST FIRE
rapport-hebdomadaire  rapport-agent   cron      ✓        42     3      2026-03-08 08:00
check-inbox           mail-agent      interval  ✓        1204   89     2026-03-09 14:32
import-csv            import-agent    file_watch ✓       17     0      2026-03-09 11:15
github-push           deploy-agent    webhook   ✓        8      1      2026-03-08 16:47
```

---

## 9. Hot Reload

Le hot reload permet de modifier `apollia.toml` et de recharger les triggers **sans redémarrer le runtime**.

```bash
# 1. Modifier apollia.toml (ajouter/modifier/supprimer un [[triggers]])
$ vim apollia.toml

# 2. Recharger sans restart
$ apollia-os trigger reload
✔ 3 triggers rechargés (1 ajouté, 1 modifié, 0 supprimé)
```

Comportement interne :
- Les sources modifiées reçoivent un signal de shutdown (via `CancellationToken`)
- Timeout 2s puis abort forcé si nécessaire
- Nouvelles sources spawned immédiatement
- EventBus émet `TriggersReloaded { count }`

---

## 10. Intégration Supervisor

`TriggerEngine` est démarré en **position 6** dans la séquence du Supervisor :

```
1. EventBus        → broadcast interne
2. AgentRegistry   → état agents
3. Tool Registry   → catalogue outils
4. Memory Engine   → SQLite
5. LlmRouter       → backends LLM
6. TriggerEngine   → moteur de déclenchement  ← SPRINT 9
7. APIServer       → connexions externes
```

Au démarrage, le Supervisor affiche :
```
✔ TriggerEngine — 3 trigger(s) actif(s)
```

Si `apollia.toml` ne contient aucun `[[triggers]]`, `TriggerEngine` démarre avec 0 définitions (comportement no-op, pas d'erreur).

---

*Voir aussi : [Configuration apollia.toml](./Config-apollia-toml) · [Dashboard Observabilité](./Dashboard-Observabilite) · [ADR-021](./Decisions-Log)*
