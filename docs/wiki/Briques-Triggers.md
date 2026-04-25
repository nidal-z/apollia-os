# Triggers Engine — Déclenchement automatique des agents

> *La crate `apollia-triggers` expose un moteur déclaratif pour déclencher des agents automatiquement via des règles persistées en SQLite : cron, interval, file watch, webhooks HMAC-SHA256. Les triggers se créent, modifient et suppriment via l'API REST ou l'application desktop (ADR-033).*

---

## 1. Vue d'ensemble

Le `TriggerEngine` est un acteur Tokio positionné en **position 6** dans la séquence de démarrage du Supervisor (après le `LlmRouter`). Il gère un ensemble de `TriggerDefinition` persistées en SQLite (`~/.apollia/triggers_def.db`) et déclenche des tâches vers le `TaskRouter` selon les événements reçus.

Les triggers ne sont plus déclarés dans `apollia.toml` — ils sont gérés exclusivement via SQLite + API REST CRUD (ADR-033). L'opérateur crée, modifie et supprime ses triggers depuis l'application desktop ou via `curl`.

```
triggers_def.db (SQLite)
         │
         ▼ chargement au démarrage
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

## 2. Gestion des triggers — CRUD SQLite

(ADR-033), les triggers sont persistés en SQLite (`~/.apollia/triggers_def.db`) et se gèrent via l'API REST ou l'application desktop. La section `[[triggers]]` de `apollia.toml` n'est plus utilisée.

### 2.1 Créer un trigger via API

```bash
# Créer un trigger cron
$ curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "rapport-hebdomadaire",
    "agent": "rapport-agent",
    "enabled": true,
    "on_busy": "queue",
    "source": {
      "type": "cron",
      "schedule": "0 8 * * MON"
    },
    "input_template": "Génère le rapport de la semaine"
  }'
```

### 2.2 Modifier un trigger

```bash
$ curl -X PUT http://localhost:7771/api/v1/triggers/rapport-hebdomadaire \
  -H "Content-Type: application/json" \
  -d '{
    "source": { "type": "cron", "schedule": "0 9 * * MON" },
    "on_busy": "drop"
  }'
```

### 2.3 Supprimer un trigger

```bash
$ curl -X DELETE http://localhost:7771/api/v1/triggers/rapport-hebdomadaire
```

### 2.4 Types de source supportés

| Type | Champs requis | Exemple |
|---|---|---|
| `cron` | `schedule` (expression cron) | `"0 8 * * MON"` |
| `interval` | `every` (durée) | `"30m"`, `"1h"`, `"6h"` |
| `oneshot` | `at` (datetime ISO) | `"2026-04-01T10:00:00Z"` |
| `file_watch` | `path`, `events` | `"~/imports/"`, `["create"]` |
| `webhook` | `secret` (min 32 chars) | `"un-secret-robuste..."` |

### 2.5 Webhook — appel externe

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

**Améliorations :**

- **Symlinks** : les symlinks dans le répertoire surveillé sont maintenant suivis (`notify::Config::with_follow_symlinks(true)`). Un lien symbolique créé dans le dossier déclenche l'événement `create` si la cible existe. Comportement cohérent sur Linux et macOS.

- **Exclusions** : la définition peut déclarer des patterns glob à exclure de la surveillance :

```json
{
  "source": {
    "type": "file_watch",
    "path": "~/imports/",
    "events": ["create", "modify"],
    "exclude": ["*.tmp", "*.part", ".~*"]
  }
}
```

Les patterns d'exclusion sont évalués sur le nom de fichier uniquement (pas le chemin complet). Les fichiers temporaires courants (`.tmp`, `.part`, éditeurs en `.~*`) sont exclus par défaut si `exclude` est absent via une liste interne configurable.

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

### 6.1 Définitions — `TriggerDefinitionRepository`

Les définitions de triggers sont persistées dans `~/.apollia/triggers_def.db` via le `TriggerDefinitionRepository` :

```rust
// apollia-triggers/src/definition_repository.rs

pub struct TriggerDefinitionRepository { /* ... rusqlite::Connection */ }

impl TriggerDefinitionRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, TriggerDefinitionError>;
    pub fn insert(&self, def: &TriggerDefinitionRow) -> Result<(), TriggerDefinitionError>;
    pub fn update(&self, id: &str, def: &TriggerDefinitionRow) -> Result<(), TriggerDefinitionError>;
    pub fn delete(&self, id: &str) -> Result<(), TriggerDefinitionError>;
    pub fn get(&self, id: &str) -> Result<Option<TriggerDefinitionRow>, TriggerDefinitionError>;
    pub fn list(&self) -> Result<Vec<TriggerDefinitionRow>, TriggerDefinitionError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TriggerDefinitionError {
    NotFound { id: String },
    DuplicateId { id: String },
    ValidationError(String),
    Database(rusqlite::Error),
}
```

Le repository est wrappé dans `Arc<Mutex<TriggerDefinitionRepository>>` dans `AppState` (ADR-033). Les mutations sont rares (opérateur humain), pas de contention en pratique.

**Validation avant écriture** (`apollia-triggers/src/validation.rs`) :
- XOR : `agent` ou `pipeline` (jamais les deux, jamais aucun)
- Expression cron syntaxiquement valide
- Secret webhook ≥ 32 caractères

### 6.2 Historique — `trigger_history`

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
    payload_json  TEXT,              -- payload JSON du trigger
    dispatch_ms   INTEGER            -- latence fire → soumission tâche
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

### 6.3 Compteurs persistés au redémarrage

Les compteurs `fire_count`, `skip_count`, `error_count` de la table `trigger_state` sont désormais persistés et survivent au redémarrage du runtime. Ils étaient en mémoire uniquement et remis à zéro à chaque démarrage.

Au boot du `TriggerEngine`, chaque `TriggerDefinition` chargée récupère ses compteurs depuis `trigger_state` via `TriggerStateRepository::load(trigger_id)`. Les `TriggerStatus` retournés par `TriggerEngineHandle::list` reflètent l'historique cumulé depuis l'installation.

```rust
pub struct TriggerStatus {
    pub id: TriggerId,
    pub agent: String,
    pub enabled: bool,
    pub fire_count: u64,   // cumulé depuis l'installation, survit aux redémarrages
    pub skip_count: u64,
    pub error_count: u64,
    pub last_fired_at: Option<DateTime<Utc>>,
}
```

---

## 7. Événements EventBus

ajoute 6 nouveaux variants `RuntimeEvent` dans `apollia-core` :

```rust
// apollia-core/src/events.rs

pub enum RuntimeEvent {
    // ... variants existants ...

    // Triggers
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

Le hot reload est déclenché automatiquement après chaque opération CRUD via l'API REST. Le pattern est : **écriture SQLite → engine.reload** (ADR-033, Option A).

```bash
# Créer un trigger via API → reload automatique
$ curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{"id": "nouveau-trigger", "agent": "mon-agent", ...}'
# → Le TriggerEngine recharge automatiquement depuis SQLite

# Reload manuel (relit triggers_def.db)
$ apollia-os trigger reload
✔ 3 triggers rechargés
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

Au démarrage, le Supervisor :
1. Ouvre `TriggerDefinitionRepository` depuis `data_dir/triggers_def.db`
2. Charge toutes les lignes, convertit en `TriggerDefinition` (les définitions invalides sont ignorées avec un `warn!`)
3. Wraps le repository dans `Arc<Mutex<>>` → stocké dans `AppState`
4. Affiche : `✔ TriggerEngine — 3 trigger(s) actif(s)`

Si la base est vide, `TriggerEngine` démarre avec 0 définitions (comportement no-op, pas d'erreur).

---

---

## 10. `OnBusyPolicy::Queue` — File bornée

, `OnBusyPolicy` dispose d'un troisième variant `Queue` qui met en file d'attente les triggers quand l'agent est occupé, dans la limite d'une capacité configurable.

### Enum mis à jour

```rust
// crates/apollia-triggers/src/types.rs

pub enum OnBusyPolicy {
    /// Ignore le trigger si l'agent est occupé. Comportement par défaut historique.
    Skip,
    /// (Existait mais non implémenté — remplacé par Queue)
    Enqueue,
    /// Met le trigger en file FIFO bornée.
    /// Si la file est pleine, le trigger est droppé et `RuntimeEvent::TriggerQueueFull` est émis.
    Queue {
        /// Capacité maximale de la file (nombre d'éléments).
        /// Configurable via `[triggers] queue_max_depth` dans `apollia.toml`.
        max_depth: usize,
    },
}
```

**Exemple de configuration :**

```json
// Payload API REST pour créer un trigger avec policy Queue
{
  "id": "rapport-hebdomadaire",
  "agent": "rapport-agent",
  "on_busy": {"queue": {"max_depth": 5}},
  "source": { "type": "cron", "schedule": "0 8 * * MON" }
}
```

```toml
# apollia.toml — capacité par défaut pour les queues non spécifiées
[triggers]
queue_max_depth = 10
```

### Événement `TriggerQueueFull`

```rust
// crates/apollia-runtime/src/events.rs — nouveau variant

/// Émis quand un trigger est droppé parce que la file de l'agent est pleine.
TriggerQueueFull {
    trigger_id: TriggerId,
},
```

### Comportement

| Situation | Résultat |
|---|---|
| Agent occupé, queue < max_depth | Trigger en queue — exécuté dès que l'agent se libère (FIFO) |
| Agent occupé, queue == max_depth | Trigger droppé + `TriggerQueueFull` émis |
| Agent libre | Trigger dispatché immédiatement (pas de queuing) |
| Policy `Skip` | Trigger ignoré silencieusement (comportement pré) |

---

*Voir aussi : [Configuration apollia.toml](./Config-apollia-toml) · [Dashboard Observabilité](./Dashboard-Observabilite) · [ADR-021](./Decisions-Log) · [ADR-033](../adr/ADR-033-config-operateur-sqlite.md)*
