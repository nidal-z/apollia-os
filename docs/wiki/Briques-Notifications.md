# Notifications Engine — Alertes découplées pour les agents

> *La crate `apollia-notifications` centralise la logique de notification d'Apollia OS : un `NotificationEngine` s'abonne à l'EventBus et dispatche 6 événements critiques vers des canaux configurables (desktop natif OS, webhooks HTTP), sans jamais bloquer le runtime.*

---

## 1. Vue d'ensemble

Le `NotificationEngine` est un acteur de fond démarré en **position 9** dans la séquence du Supervisor (après le `TriggerEngine` et l'`APIServer`). Il n'a pas de handle externe : une fois lancé via `tokio::spawn(engine.run())`, il tourne de manière autonome jusqu'à la fermeture de l'EventBus.

```
apollia.toml [notifications]
         │
         ▼ build_channels() — validation + instanciation
 ┌──────────────────────────┐
 │   NotificationEngine     │ ← acteur de fond, position 9 Supervisor
 │   (s'abonne à EventBus)  │
 └──────────┬───────────────┘
            │ RuntimeEvent (broadcast)
            │
     ┌──────▼──────────────────────────────────┐
     │  event_filter::map_event()               │
     │  Transforme RuntimeEvent → Notification  │
     │  6 événements mappés, autres → None      │
     └──────┬──────────────────────────────────┘
            │ Notification { event, message, severity, metadata }
            │
     ┌──────▼──────────────────────────────────────────────────┐
     │  dispatch_notif() — itère sur les canaux configurés      │
     │                                                          │
     │  ┌─────────────────┐   ┌──────────────────────────────┐ │
     │  │  DesktopChannel │   │  WebhookChannel              │ │
     │  │  notify-rust v4 │   │  POST JSON + X-Apollia-Event │ │
     │  │  spawn_blocking │   │  timeout 5s, reqwest         │ │
     │  └─────────────────┘   └──────────────────────────────┘ │
     │                                                          │
     │  Erreur canal → warn!, dispatch continue (non-bloquant)  │
     └──────────────────────────────────────────────────────────┘
```

**Philosophie de conception :**

- **Découplé** : le `NotificationEngine` consomme l'EventBus en lecture seule. Aucun autre acteur ne dépend de lui. Une notification ratée ne perturbe jamais l'exécution d'une tâche agent.
- **Non-bloquant** : `send()` retourne `Ok(())` immédiatement. L'attente d'une action utilisateur desktop tourne dans un `spawn_blocking` indépendant.
- **Dégradation gracieuse** : canal en erreur → `warn!` + dispatch continue. Desktop headless (Linux sans `DISPLAY` ni `DBUS_SESSION_BUS_ADDRESS`) → `Ok(())` silencieux. Webhook timeout → `Err(WebhookFailed(_))` loggé en `warn!`.
- **Pas de section = pas de démarrage** : si `[notifications]` est absent de `apollia.toml`, le Supervisor n'instancie pas d'engine (aucun coût).

---

## 2. Interface publique Rust

### 2.1 `Notification` — structure centrale

```rust
// apollia-notifications/src/engine.rs

/// Notification prête à être envoyée via un ou plusieurs canaux.
/// Produite par event_filter::map_event() à partir d'un RuntimeEvent.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Nom de l'événement déclencheur (ex: "task.input_required", "task.failed").
    pub event: String,
    /// Horodatage UTC de la notification.
    pub timestamp: DateTime<Utc>,
    /// Identifiant de la tâche concernée, si applicable.
    pub task_id: Option<String>,
    /// Nom ou identifiant de l'agent concerné, si applicable.
    pub agent: Option<String>,
    /// Message lisible destiné à l'utilisateur.
    pub message: String,
    /// Métadonnées additionnelles (URLs d'action, identifiants, contexte).
    pub metadata: HashMap<String, String>,
    /// Sévérité de la notification.
    pub severity: Severity,
}
```

### 2.2 `NotificationChannel` — trait des canaux

```rust
// apollia-notifications/src/engine.rs

/// Trait à implémenter par chaque canal de notification.
/// Object-safe (Box<dyn NotificationChannel>) et thread-safe (Send + Sync).
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Identifiant unique du canal tel que configuré dans apollia.toml.
    fn id(&self) -> &str;

    /// Retourne true si ce canal accepte l'événement nommé.
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool;

    /// Envoie la notification via ce canal.
    /// En cas d'erreur, retourner un NotifError — l'engine logge et continue.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError>;
}
```

### 2.3 `NotifError` — erreurs de canal

```rust
// apollia-notifications/src/engine.rs

#[derive(Debug, thiserror::Error)]
pub enum NotifError {
    /// Canal desktop indisponible (notifications OS non supportées ou permission refusée).
    #[error("canal desktop indisponible : {0}")]
    DesktopUnavailable(String),
    /// Appel webhook échoué (erreur réseau, timeout, code HTTP non-2xx).
    #[error("webhook échoué : {0}")]
    WebhookFailed(String),
    /// Erreur interne du canal (sérialisation, état incohérent, etc.).
    #[error("erreur interne : {0}")]
    Internal(String),
}
```

### 2.4 `Severity` — sévérité d'une notification

```rust
// apollia-notifications/src/config.rs

pub enum Severity {
    Info,     // Information — événement non bloquant.
    Warning,  // Avertissement — intervention recommandée.
    Error,    // Erreur — intervention requise.
}

impl Severity {
    pub fn as_str(&self) -> &'static str { /* "info" | "warning" | "error" */ }
}
```

### 2.5 `NotificationEngine` — moteur principal

```rust
// apollia-notifications/src/engine.rs

pub struct NotificationEngine {
    config: NotificationConfig,
    channels: Vec<Box<dyn NotificationChannel>>,
    event_bus: EventBusSender,
}

impl NotificationEngine {
    /// Crée un nouveau moteur de notification.
    pub fn new(
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
        event_bus: EventBusSender,
    ) -> Self;

    /// Boucle principale — à lancer via tokio::spawn(engine.run()).
    /// Se termine proprement quand l'EventBus est fermé (RecvError::Closed).
    /// Les erreurs Lagged (bus saturé) sont loggées en warn! sans interruption.
    pub async fn run(self);

    /// Transforme un RuntimeEvent en Notification (fonction pure, testable).
    pub fn map_event(&self, event: &RuntimeEvent) -> Option<Notification>;
}
```

### 2.6 Types de configuration

```rust
// apollia-notifications/src/config.rs

/// Configuration globale chargée depuis [notifications] dans apollia.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct NotificationConfig {
    /// Événements activés globalement.
    pub events: Vec<String>,
    /// Canaux de notification configurés.
    pub channels: Vec<ChannelConfig>,
}

/// Configuration d'un canal individuel ([[notifications.channels]]).
#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ChannelKind,
    pub enabled: bool,
    /// None → liste globale ; Some(["*"]) → tous ; Some(liste) → sous-ensemble.
    pub events: Option<Vec<String>>,
    pub url: Option<String>,  // requis pour type = "webhook"
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Desktop,  // notify-rust v4
    Webhook,  // HTTP POST
    Sse,      // géré par le dashboard (Sprint 9), ignoré ici
}

/// Instancie les canaux actifs à partir des ChannelConfig.
/// Retourne NotifConfigError::MissingWebhookUrl si un webhook actif n'a pas d'url.
pub fn build_channels(
    configs: &[ChannelConfig],
) -> Result<Vec<Box<dyn NotificationChannel>>, NotifConfigError>;

#[derive(Debug, thiserror::Error)]
pub enum NotifConfigError {
    #[error("url manquante pour le canal webhook '{id}'")]
    MissingWebhookUrl { id: String },
}
```

---

## 3. Événements mappés

La fonction `event_filter::map_event()` est pure : mêmes entrées, mêmes sorties, sans effet de bord. Elle reconnaît exactement **6 événements critiques** et retourne `None` pour tous les autres.

| `RuntimeEvent` | Nom de notification | Sévérité | Métadonnées clés |
|---|---|---|---|
| `TaskInputRequired { task_id, prompt, .. }` | `task.input_required` | `Warning` | `resume_url`, `inspect_url` |
| `TaskCompleted { success: false, .. }` | `task.failed` | `Error` | — |
| `TaskCompleted { success: true, .. }` | `task.completed` | `Info` | — |
| `AgentDegraded { agent_id, reason }` | `agent.degraded` | `Warning` | — |
| `LlmModelFailed { backend, reason }` | `llm.backend_down` | `Error` | `backend` |
| `TriggerError { trigger_id, error }` | `trigger.error` | `Error` | `trigger_id` |

**Tous les autres `RuntimeEvent`** (`AgentRegistered`, `TaskStarted`, `AllReady`, etc.) retournent `None` — aucune notification n'est émise.

Les métadonnées HITL de `task.input_required` contiennent :
- `resume_url` : `http://localhost:7771/api/v1/tasks/{id}/resume`
- `inspect_url` : `http://localhost:7771/dashboard#tasks/{id}`

### Logique de filtrage par canal

```rust
// apollia-notifications/src/config.rs — channel_accepts_event()

// canal disabled          → false (toujours)
// events = None           → true si event dans la liste globale
// events = Some(["*"])    → true si event dans la liste globale
// events = Some(liste)    → true si event dans la liste du canal
```

---

## 4. Configuration `apollia.toml`

### 4.1 Structure minimale

```toml
[notifications]
events = [
    "task.input_required",
    "task.failed",
    "agent.degraded",
]

[[notifications.channels]]
id      = "desktop"
type    = "desktop"
enabled = true
```

### 4.2 Canal desktop — tous les événements

```toml
[notifications]
events = [
    "task.input_required",
    "task.failed",
    "task.completed",
    "agent.degraded",
    "llm.backend_down",
    "trigger.error",
]

[[notifications.channels]]
id      = "desktop"
type    = "desktop"
enabled = true
# Sans champ events → hérite de la liste globale
```

### 4.3 Canal webhook — sous-ensemble d'événements

```toml
[[notifications.channels]]
id      = "monitoring"
type    = "webhook"
enabled = true
url     = "https://hooks.exemple.com/services/XXX/YYY/ZZZ"
events  = ["task.failed", "agent.degraded", "llm.backend_down"]
```

### 4.4 Canal webhook — wildcard (tous les événements globaux)

```toml
[[notifications.channels]]
id      = "audit-complet"
type    = "webhook"
enabled = true
url     = "https://interne.exemple.com/apollia-events"
events  = ["*"]
```

### 4.5 Plusieurs canaux combinés

```toml
[notifications]
events = ["task.input_required", "task.failed", "agent.degraded"]

# Canal desktop pour les alertes HITL uniquement
[[notifications.channels]]
id      = "desktop"
type    = "desktop"
enabled = true
events  = ["task.input_required"]

# Webhook pour toutes les erreurs
[[notifications.channels]]
id      = "slack-erreurs"
type    = "webhook"
enabled = true
url     = "https://hooks.slack.com/services/..."
events  = ["task.failed", "agent.degraded"]
```

---

## 5. Canal Desktop (`DesktopChannel`)

```rust
// apollia-notifications/src/channels/desktop.rs

pub struct DesktopChannel {
    id: String,
    enabled: bool,
    events: Option<Vec<String>>,
}

impl DesktopChannel {
    pub fn new(id: impl Into<String>, enabled: bool, events: Option<Vec<String>>) -> Self;
}

impl Default for DesktopChannel {
    fn default() -> Self { Self::new("desktop", true, None) }
}
```

**Comportement selon la plateforme :**

| Plateforme | Implémentation | Actions inline |
|---|---|---|
| Linux (XDG / D-Bus) | `notify-rust` + `wait_for_action` | Oui (Approuver / Rejeter / Inspecter) |
| macOS | `notify-rust` simple | Non (API `wait_for_action` XDG uniquement) |
| Linux CI headless | Return `Ok(())` silencieux | N/A |

**Actions HITL sur Linux (`task.input_required`) :**

```
Notification OS : "Apollia OS — <agent>"
  ┌────────────────────────────────────────┐
  │ Confirmer l'envoi ?                    │
  │                                        │
  │  [✔ Approuver] [✗ Rejeter] [Inspecter]│
  └────────────────────────────────────────┘

  ✔ Approuver  → POST http://localhost:7771/api/v1/tasks/{id}/resume
                  Body: { "approved": true }

  ✗ Rejeter    → POST http://localhost:7771/api/v1/tasks/{id}/resume
                  Body: { "approved": false, "reason": "Refusé depuis la notification" }

  Inspecter    → open::that("http://localhost:7771/dashboard#tasks/{id}")
```

Pour les autres événements sur Linux, un clic sur la notification ouvre le dashboard (`http://localhost:7771/dashboard`).

**Non-blocage garanti :** `send()` retourne `Ok(())` immédiatement. L'appel bloquant `os_notif.show()` + `wait_for_action()` s'exécute dans un `tokio::task::spawn_blocking` en arrière-plan. L'échec de l'affichage OS est loggé en `warn!` sans propagation.

**Dégradation CI headless (Linux) :** si `DISPLAY` et `DBUS_SESSION_BUS_ADDRESS` sont tous deux absents, la notification est ignorée silencieusement (`Ok(())`) sans même spawner le `spawn_blocking`.

---

## 6. Canal Webhook (`WebhookChannel`)

```rust
// apollia-notifications/src/channels/webhook.rs

pub struct WebhookChannelConfig {
    pub id: String,
    pub url: String,    // obligatoire (contrairement à ChannelConfig.url: Option)
    pub enabled: bool,
    pub events: Option<Vec<String>>,
}

pub struct WebhookChannel {
    config: WebhookChannelConfig,
    client: Client,     // reqwest::Client, timeout 5s
}

impl WebhookChannel {
    /// Client reqwest avec timeout 5s et User-Agent "apollia-os/<version>".
    pub fn new(config: WebhookChannelConfig) -> Self;
}
```

**Payload JSON fixe Apollia :**

```json
{
    "event":     "task.failed",
    "timestamp": "2026-03-09T14:32:17.123456Z",
    "runtime":   "apollia-os",
    "version":   "0.11.0",
    "task_id":   "t-0042",
    "agent":     "devis-agent",
    "message":   "Tâche échouée",
    "metadata":  {},
    "severity":  "error"
}
```

**Headers envoyés :**

| Header | Valeur |
|---|---|
| `Content-Type` | `application/json` (positionné par reqwest `.json()`) |
| `X-Apollia-Event` | nom de l'événement (ex: `task.failed`) |
| `User-Agent` | `apollia-os/<version>` |

**Gestion des erreurs :**

- Erreur réseau ou timeout (5s) → `Err(NotifError::WebhookFailed(_))`
- Réponse HTTP non-2xx → `Err(NotifError::WebhookFailed("HTTP 500"))` (avec le code)
- Toute erreur est loggée en `warn!` par l'engine ; le dispatch vers les autres canaux continue.

**Exemple de réception côté serveur (Python) :**

```python
from flask import Flask, request

app = Flask(__name__)

@app.route("/apollia-events", methods=["POST"])
def receive():
    event = request.headers.get("X-Apollia-Event")
    payload = request.get_json()
    print(f"[{event}] task={payload.get('task_id')} — {payload.get('message')}")
    return "", 200
```

---

## 7. Intégration Supervisor

Le `NotificationEngine` est démarré en **position 9** dans la séquence du Supervisor, après l'APIServer :

```
1. EventBus        → broadcast interne
2. AgentRegistry   → état agents
3. Tool Registry   → catalogue outils
4. Memory Engine   → SQLite
5. LlmRouter       → backends LLM
6. TriggerEngine   → déclenchement automatique
7. APIServer       → connexions externes
8. Dashboard       → observabilité HTMX + SSE
9. NotifEngine     → alertes desktop / webhook  ← SPRINT 11
```

Le Supervisor passe une `Option<NotificationConfig>` : si `None` (section absente de `apollia.toml`), aucun engine n'est démarré. Si `Some(config)`, `build_channels()` est appelé au démarrage — une `NotifConfigError` est une erreur fatale (webhook sans `url`).

**Pattern d'instanciation dans le Supervisor :**

```rust
if let Some(notif_config) = config.notifications {
    let channels = build_channels(&notif_config.channels)?;
    let engine = NotificationEngine::new(notif_config, channels, event_bus.clone());
    tokio::spawn(engine.run());
    tracing::info!("NotificationEngine démarré");
}
```

---

## 8. Garanties de robustesse

| Scénario | Comportement |
|---|---|
| Canal desktop renvoie `Err(_)` | `warn!` loggé, dispatch continue vers les canaux suivants |
| Canal webhook renvoie `Err(_)` | `warn!` loggé, dispatch continue vers les canaux suivants |
| Linux headless (CI) sans `DISPLAY` | `Ok(())` immédiat, aucune tentative d'affichage |
| Webhook timeout (5s) | `Err(NotifError::WebhookFailed(_))` → `warn!` |
| Webhook réponse non-2xx | `Err(NotifError::WebhookFailed("HTTP NNN"))` → `warn!` |
| EventBus saturé (Lagged) | `warn!(skipped = N, ...)`, boucle continue sans interruption |
| EventBus fermé (arrêt runtime) | `break` — engine se termine proprement, aucun panic |
| Section `[notifications]` absente | Engine non démarré — aucun coût, aucune erreur |
| Webhook actif sans `url` | `NotifConfigError::MissingWebhookUrl` → erreur fatale au démarrage |

---

*Voir aussi : [Configuration apollia.toml](./Config-apollia-toml) · [ADR-024](./Decisions-Log) · [Briques-Runtime-Core](./Briques-Runtime-Core) · [Briques-Triggers](./Briques-Triggers)*
