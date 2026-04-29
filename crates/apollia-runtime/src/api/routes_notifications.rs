//! Routes REST pour les notifications.
//!
//! Expose les endpoints de gestion des notifications :
//! - `GET    /api/v1/notifications/channels`         — liste des canaux (depuis SQLite)
//! - `POST   /api/v1/notifications/channels`         — créer un canal
//! - `PUT    /api/v1/notifications/channels/:id`     — modifier un canal
//! - `DELETE /api/v1/notifications/channels/:id`     — supprimer un canal
//! - `GET    /api/v1/notifications/events`           — événements globaux
//! - `PUT    /api/v1/notifications/events`           — remplacer événements globaux
//! - `POST   /api/v1/notifications/channels/:id/test` — test d'un canal
//! - `POST   /api/v1/notifications/test`             — test de tous les canaux
//! - `GET    /api/v1/notifications/logs`             — historique depuis notifications.db

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_notifications::{
    build_channels,
    config::{ChannelKind, NotificationConfig},
    engine::Notification,
    NotificationChannelRow, NotificationConfigError, Severity,
};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Request types ──────────────────────────────────────────────────────────

/// Corps de requête pour `POST /api/v1/notifications/channels`.
#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    /// Identifiant unique du canal.
    pub id: String,
    /// Type de canal : `"desktop"` ou `"webhook"`.
    pub channel_type: String,
    /// Indique si le canal est actif (défaut : `true`).
    pub enabled: Option<bool>,
    /// Configuration spécifique au type (ex: `{"url": "..."}` pour webhook).
    pub config: serde_json::Value,
    /// Liste d'événements spécifiques. `null` = utilise les événements globaux.
    pub events: Option<Vec<String>>,
}

/// Corps de requête pour `PUT /api/v1/notifications/channels/:id`.
#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    /// Type de canal (optionnel — conserve l'existant si absent).
    pub channel_type: Option<String>,
    /// Indique si le canal est actif.
    pub enabled: Option<bool>,
    /// Configuration spécifique au type.
    pub config: Option<serde_json::Value>,
    /// Liste d'événements spécifiques.
    pub events: Option<Vec<String>>,
}

/// Corps de requête pour `PUT /api/v1/notifications/events`.
#[derive(Debug, Deserialize)]
pub struct SetEventsRequest {
    /// Nouvelle liste d'événements globaux.
    pub events: Vec<String>,
}

// ─── Response types ─────────────────────────────────────────────────────────

/// Canal de notification complet retourné par les opérations CRUD.
#[derive(Debug, Serialize)]
pub struct ChannelResponse {
    /// Identifiant unique du canal.
    pub id: String,
    /// Type de canal.
    pub channel_type: String,
    /// `true` si le canal est activé.
    pub enabled: bool,
    /// Configuration spécifique au type.
    pub config: serde_json::Value,
    /// Événements spécifiques au canal.
    pub events: Option<Vec<String>>,
    /// Horodatage de création (ISO 8601).
    pub created_at: String,
    /// Horodatage de dernière modification (ISO 8601).
    pub updated_at: String,
}

/// Réponse pour `GET /api/v1/notifications/events`.
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    /// Liste des événements globaux.
    pub events: Vec<String>,
}

/// Description publique d'un canal retournée par `GET /channels`.
#[derive(Debug, Serialize)]
pub struct ChannelInfo {
    /// Identifiant unique du canal (ex: `"desktop"`, `"slack"`).
    pub channel_id: String,
    /// Type de canal : `"desktop"`, `"webhook"`, ou `"sse"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `true` si le canal est activé dans la configuration.
    pub enabled: bool,
    /// Liste des événements que ce canal accepte.
    pub events: Vec<String>,
}

/// Résultat du test d'un canal individuel retourné par `POST /test`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelTestResult {
    /// Identifiant unique du canal.
    pub channel_id: String,
    /// Type de canal.
    #[serde(rename = "type")]
    pub kind: String,
    /// Statut du test : `"ok"`, `"error"`, ou `"disabled"`.
    pub status: String,
    /// Message d'erreur si `status == "error"`.
    pub error: Option<String>,
    /// Latence mesurée en millisecondes (`None` si le canal est désactivé).
    pub latency_ms: Option<u64>,
}

/// Corps de réponse en cas d'erreur.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Message d'erreur.
    pub error: String,
}

/// Réponse pour `DELETE /api/v1/notifications/channels/:id`.
#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    /// Identifiant du canal supprimé.
    pub deleted: String,
}

// ─── Query params ───────────────────────────────────────────────────────────

/// Paramètres de requête pour `GET /api/v1/notifications/logs`.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Nombre maximal d'entrées à retourner (défaut : 20, max : 1000).
    #[serde(default = "default_last")]
    pub last: usize,
}

fn default_last() -> usize {
    20
}

// ─── CRUD Handlers ──────────────────────────────────────────────────────────

/// `POST /api/v1/notifications/channels` — créer un canal de notification.
///
/// Valide le canal, l'insère dans `notifications.db`, puis recharge le
/// [`NotificationEngine`] via son handle.
pub async fn create_channel<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateChannelRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    let row = NotificationChannelRow {
        id: body.id,
        channel_type: body.channel_type,
        enabled: body.enabled.unwrap_or(true),
        config_json: body.config,
        events_json: body.events,
        created_at: String::new(),
        updated_at: String::new(),
    };

    let created = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = guard.insert_channel(&row) {
            return map_notif_error(e);
        }
        match guard.get_channel(&row.id) {
            Ok(Some(ch)) => ch,
            Ok(None) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "channel inserted but not found"})),
                );
            }
            Err(e) => return map_notif_error(e),
        }
    };

    reload_notification_engine(&state, &repo).await;

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(row_to_response(&created)).unwrap_or_default()),
    )
}

/// `PUT /api/v1/notifications/channels/:id` — modifier un canal existant.
///
/// Met à jour le canal dans `notifications.db`, puis recharge le
/// [`NotificationEngine`].
pub async fn update_channel<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateChannelRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    let updated = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        let existing = match guard.get_channel(&id) {
            Ok(Some(ch)) => ch,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": format!("channel not found: {id}")})),
                );
            }
            Err(e) => return map_notif_error(e),
        };

        let merged = NotificationChannelRow {
            id: id.clone(),
            channel_type: body.channel_type.unwrap_or(existing.channel_type),
            enabled: body.enabled.unwrap_or(existing.enabled),
            config_json: body.config.unwrap_or(existing.config_json),
            events_json: body.events.or(existing.events_json),
            created_at: existing.created_at,
            updated_at: existing.updated_at,
        };

        if let Err(e) = guard.update_channel(&id, &merged) {
            return map_notif_error(e);
        }
        match guard.get_channel(&id) {
            Ok(Some(ch)) => ch,
            Ok(None) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "channel updated but not found"})),
                );
            }
            Err(e) => return map_notif_error(e),
        }
    };

    reload_notification_engine(&state, &repo).await;

    (
        StatusCode::OK,
        Json(serde_json::to_value(row_to_response(&updated)).unwrap_or_default()),
    )
}

/// `DELETE /api/v1/notifications/channels/:id` — supprimer un canal.
///
/// Supprime le canal dans `notifications.db`, puis recharge le
/// [`NotificationEngine`].
pub async fn delete_channel<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = guard.delete_channel(&id) {
            return map_notif_error(e);
        }
    }

    reload_notification_engine(&state, &repo).await;

    (
        StatusCode::OK,
        Json(serde_json::to_value(DeleteResponse { deleted: id }).unwrap_or_default()),
    )
}

/// `GET /api/v1/notifications/events` — liste des événements globaux.
pub async fn get_events<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    let events = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        match guard.get_global_events() {
            Ok(ev) => ev,
            Err(e) => return map_notif_error(e),
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::to_value(EventsResponse { events }).unwrap_or_default()),
    )
}

/// `PUT /api/v1/notifications/events` — remplacer les événements globaux.
///
/// Valide chaque événement contre [`KNOWN_EVENTS`](apollia_notifications::KNOWN_EVENTS),
/// puis remplace la liste dans `notifications.db` et recharge le
/// [`NotificationEngine`].
pub async fn set_events<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<SetEventsRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = guard.set_global_events(&body.events) {
            return map_notif_error(e);
        }
    }

    reload_notification_engine(&state, &repo).await;

    let events = body.events;
    (
        StatusCode::OK,
        Json(serde_json::to_value(EventsResponse { events }).unwrap_or_default()),
    )
}

// ─── Existing Handlers ──────────────────────────────────────────────────────

/// `GET /api/v1/notifications/channels` — liste des canaux configurés.
///
/// Lit depuis le repository SQLite. Fallback sur `notification_config`
/// si le repo n'est pas disponible.
pub async fn list_channels<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<serde_json::Value> {
    // Prefer SQLite repo
    if let Some(ref repo) = state.notification_repo {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        let channels: Vec<ChannelResponse> = match guard.list_channels() {
            Ok(rows) => rows.iter().map(row_to_response).collect(),
            Err(_) => vec![],
        };
        return Json(serde_json::json!({ "channels": channels }));
    }

    // Fallback: in-memory config (backward compat)
    let Some(config) = &state.notification_config else {
        return Json(serde_json::json!({ "channels": [] }));
    };

    let channels: Vec<ChannelInfo> = config
        .channels
        .iter()
        .map(|ch| ChannelInfo {
            channel_id: ch.id.clone(),
            kind: channel_kind_str(&ch.kind),
            enabled: ch.enabled,
            events: ch.events.clone().unwrap_or_else(|| config.events.clone()),
        })
        .collect();

    Json(serde_json::json!({ "channels": channels }))
}

/// `POST /api/v1/notifications/test` — envoi d'une notification de test.
///
/// Pour chaque canal activé dans la config :
/// - Instancie le canal via [`build_channels`]
/// - Envoie une [`Notification`] de test avec l'événement `"test.ping"`
/// - Mesure la latence et collecte le statut (`"ok"`, `"error"`, `"disabled"`)
pub async fn test_channels<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(config) = state.notification_config.clone() else {
        return (StatusCode::OK, Json(serde_json::json!({ "results": [] })));
    };

    let mut results: Vec<ChannelTestResult> = Vec::new();

    for ch in &config.channels {
        if !ch.enabled {
            results.push(ChannelTestResult {
                channel_id: ch.id.clone(),
                kind: channel_kind_str(&ch.kind),
                status: "disabled".to_string(),
                error: None,
                latency_ms: None,
            });
        }
    }

    let channels = match build_channels(&config.channels) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let test_notif = make_test_notification();

    for channel in &channels {
        let start = Instant::now();
        let outcome = channel.send(&test_notif).await;
        let latency_ms = start.elapsed().as_millis() as u64;
        let kind = channel_kind_by_id(channel.id(), &config);

        let (status, error) = match outcome {
            Ok(()) => ("ok".to_string(), None),
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };

        results.push(ChannelTestResult {
            channel_id: channel.id().to_string(),
            kind,
            status,
            error,
            latency_ms: Some(latency_ms),
        });
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({ "results": results })),
    )
}

/// `GET /api/v1/notifications/logs?last=N` — historique des notifications.
///
/// Lit depuis `notifications.db` via le repository.
/// Fallback sur `hitl.db` si le repo n'est pas disponible (backward compat).
pub async fn notification_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<LogsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let last = params.last.min(1000);

    // Prefer notifications.db repo
    if let Some(ref repo) = state.notification_repo {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        match guard.query_logs(last) {
            Ok(logs) => {
                let entries: Vec<serde_json::Value> = logs
                    .iter()
                    .map(|log| {
                        let channels = serde_json::from_str(&log.channels)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        serde_json::json!({
                            "id": log.id,
                            "event_name": log.event_name,
                            "task_id": log.task_id,
                            "agent_id": log.agent_id,
                            "sent_at": log.sent_at,
                            "channels": channels,
                            "error": log.error,
                        })
                    })
                    .collect();
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({ "entries": entries })),
                );
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        }
    }

    // Fallback: hitl.db (backward compat)
    let db_path = resolve_notif_db_path(&state);
    let entries_result =
        tokio::task::spawn_blocking(move || query_notification_logs(&db_path, last)).await;

    match entries_result {
        Ok(Ok(entries)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "entries": entries })),
        ),
        Ok(Err(msg)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": msg })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("spawn_blocking failed: {e}") })),
        ),
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Convertit une [`NotificationChannelRow`] en [`ChannelResponse`].
fn row_to_response(row: &NotificationChannelRow) -> ChannelResponse {
    ChannelResponse {
        id: row.id.clone(),
        channel_type: row.channel_type.clone(),
        enabled: row.enabled,
        config: row.config_json.clone(),
        events: row.events_json.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// Mappe une [`NotificationConfigError`] vers un couple `(StatusCode, JSON)`.
fn map_notif_error(err: NotificationConfigError) -> (StatusCode, Json<serde_json::Value>) {
    let (status, msg) = match &err {
        NotificationConfigError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        NotificationConfigError::DuplicateId(_) => (StatusCode::CONFLICT, err.to_string()),
        NotificationConfigError::ValidationError(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
        NotificationConfigError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    };
    (status, Json(serde_json::json!({ "error": msg })))
}

/// Recharge le [`NotificationEngine`] depuis les données du repository.
///
/// Lit les canaux et événements globaux du repo, construit la nouvelle
/// [`NotificationConfig`], instancie les canaux concrets via [`build_channels`],
/// puis envoie le tout au moteur pour hot-reload.
async fn reload_notification_engine<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    repo: &Arc<std::sync::Mutex<apollia_notifications::NotificationConfigRepository>>,
) {
    let engine_handle = match &state.notification_engine_handle {
        Some(h) => h.clone(),
        None => return,
    };

    let (channel_rows, global_events) = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        let rows = guard.list_channels().unwrap_or_default();
        let events = guard.get_global_events().unwrap_or_default();
        (rows, events)
    };

    let channel_configs: Vec<apollia_notifications::ChannelConfig> = channel_rows
        .iter()
        .map(|row| row.to_channel_config())
        .collect();

    let config = NotificationConfig {
        events: global_events,
        channels: channel_configs,
        inactivity_timeout_secs: 30,
    };

    let channels = match build_channels(&config.channels) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "notification reload: failed to build channels");
            return;
        }
    };

    engine_handle.reload(config, channels).await;
}

/// Build a test [`Notification`] for the `"test.ping"` event.
fn make_test_notification() -> Notification {
    Notification {
        event: "test.ping".to_string(),
        timestamp: chrono::Utc::now(),
        task_id: None,
        agent: None,
        message: "Notification de test Apollia OS".to_string(),
        metadata: HashMap::new(),
        severity: Severity::Info,
    }
}

/// Return the string representation of a [`ChannelKind`].
fn channel_kind_str(kind: &ChannelKind) -> String {
    match kind {
        ChannelKind::Desktop => "desktop".to_string(),
        ChannelKind::Webhook => "webhook".to_string(),
        ChannelKind::Sse => "sse".to_string(),
        ChannelKind::Terminal => "terminal".to_string(),
    }
}

/// Return the `kind` string for the channel identified by `id` in `config`.
///
/// Falls back to `"unknown"` if the ID is not found.
fn channel_kind_by_id(id: &str, config: &NotificationConfig) -> String {
    config
        .channels
        .iter()
        .find(|ch| ch.id == id)
        .map(|ch| channel_kind_str(&ch.kind))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Resolve the path of the notification log database.
///
/// Uses `~/.apollia/hitl.db` by default.
fn resolve_notif_db_path<B: ExecutionBackend + Clone>(state: &AppState<B>) -> std::path::PathBuf {
    state
        .task_repository
        .as_ref()
        .and_then(|_| std::env::var("HOME").ok())
        .map(|home| std::path::PathBuf::from(format!("{home}/.apollia/hitl.db")))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| std::path::PathBuf::from(format!("{home}/.apollia/hitl.db")))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/apollia-notif.db"))
}

/// Open `db_path`, create `notification_logs` if needed, and return the last `N` entries.
fn query_notification_logs(
    db_path: &std::path::Path,
    last: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("impossible d'ouvrir la base : {e}"))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notification_logs (
            id          TEXT    PRIMARY KEY,
            event_name  TEXT    NOT NULL,
            task_id     TEXT,
            agent_id    TEXT,
            sent_at     TEXT    NOT NULL DEFAULT (datetime('now')),
            channels    TEXT    NOT NULL DEFAULT '{}',
            error       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_notif_logs_sent_at ON notification_logs(sent_at);",
    )
    .map_err(|e| format!("migration notification_logs échouée : {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, event_name, task_id, agent_id, sent_at, channels, error
               FROM notification_logs
              ORDER BY sent_at DESC
              LIMIT ?1",
        )
        .map_err(|e| format!("prepare échoué : {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![last as i64], |row| {
            let channels_raw: String = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                channels_raw,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| format!("query échouée : {e}"))?;

    let mut entries = Vec::new();
    for row_result in rows {
        let (id, event_name, task_id, agent_id, sent_at, channels_raw, error) =
            row_result.map_err(|e| format!("lecture ligne échouée : {e}"))?;

        let channels = serde_json::from_str(&channels_raw)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        entries.push(serde_json::json!({
            "id": id,
            "event_name": event_name,
            "task_id": task_id,
            "agent_id": agent_id,
            "sent_at": sent_at,
            "channels": channels,
            "error": error,
        }));
    }

    Ok(entries)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_notifications::config::{ChannelConfig, ChannelKind, NotificationConfig};
    use apollia_notifications::NotificationConfigRepository;

    use axum::body::Body;
    use axum::http::Request;
    use axum::Router;
    use tower::ServiceExt;

    use crate::coordinator::{DynBackend, ExecutionBackend};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use std::future::Future;
    use std::pin::Pin;

    #[derive(Clone)]
    struct MockBackend;

    impl From<DynBackend> for MockBackend {
        fn from(_: DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    fn make_state_with_repo(db_path: &std::path::Path) -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 64);
        let repo = NotificationConfigRepository::open(db_path).expect("open notifications.db");
        AppState {
            router_handle,
            registry_handle: registry,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: Some(Arc::new(std::sync::Mutex::new(repo))),
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
        }
    }

    fn make_crud_router(state: AppState<MockBackend>) -> Router {
        Router::new()
            .route(
                "/api/v1/notifications/channels",
                axum::routing::get(list_channels::<MockBackend>)
                    .post(create_channel::<MockBackend>),
            )
            .route(
                "/api/v1/notifications/channels/:id",
                axum::routing::put(update_channel::<MockBackend>)
                    .delete(delete_channel::<MockBackend>),
            )
            .route(
                "/api/v1/notifications/events",
                axum::routing::get(get_events::<MockBackend>).put(set_events::<MockBackend>),
            )
            .route(
                "/api/v1/notifications/logs",
                axum::routing::get(notification_logs::<MockBackend>),
            )
            .with_state(state)
    }

    async fn read_body(resp: axum::response::Response) -> serde_json::Value {
        let body = axum::body::to_bytes(resp.into_body(), 65536)
            .await
            .expect("read body");
        serde_json::from_slice(&body).expect("parse JSON")
    }

    // ── POST /api/v1/notifications/channels -> 201 ──────────────────────────

    #[tokio::test]
    async fn test_create_channel_201() {
        // GIVEN un repository vide
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN POST avec un canal webhook valide
        let body = serde_json::json!({
            "id": "slack-ops",
            "channel_type": "webhook",
            "config": {"url": "https://hooks.slack.com/test"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 201 avec le canal complet
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = read_body(resp).await;
        assert_eq!(json["id"], "slack-ops");
        assert_eq!(json["channel_type"], "webhook");
        assert_eq!(json["enabled"], true);
        assert!(!json["created_at"].as_str().unwrap_or("").is_empty());
    }

    // ── PUT /api/v1/notifications/channels/:id -> 200 ───────────────────────

    #[tokio::test]
    async fn test_update_channel_200() {
        // GIVEN un canal existant
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        let create_body = serde_json::json!({
            "id": "slack-ops",
            "channel_type": "webhook",
            "config": {"url": "https://hooks.slack.com/old"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&create_body).expect("json")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN PUT avec une nouvelle URL
        let update_body = serde_json::json!({
            "config": {"url": "https://hooks.slack.com/new"}
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/notifications/channels/slack-ops")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&update_body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 avec la nouvelle URL
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(json["id"], "slack-ops");
        assert_eq!(json["config"]["url"], "https://hooks.slack.com/new");
    }

    // ── DELETE /api/v1/notifications/channels/:id -> 200 ────────────────────

    #[tokio::test]
    async fn test_delete_channel_200() {
        // GIVEN un canal existant
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        let body = serde_json::json!({
            "id": "slack-ops",
            "channel_type": "webhook",
            "config": {"url": "https://hooks.slack.com/test"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN DELETE
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/notifications/channels/slack-ops")
            .body(Body::empty())
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");

        // THEN 200
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(json["deleted"], "slack-ops");

        // ET le canal n'existe plus
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/notifications/channels")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");
        let json = read_body(resp).await;
        assert_eq!(json["channels"].as_array().map(|a| a.len()), Some(0));
    }

    // ── GET /api/v1/notifications/events ────────────────────────────────────

    #[tokio::test]
    async fn test_get_events() {
        // GIVEN des events globaux configurés
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));

        // Insert events via repo
        {
            let guard = state
                .notification_repo
                .as_ref()
                .expect("repo")
                .lock()
                .expect("lock");
            guard
                .set_global_events(&["task.completed".into(), "task.failed".into()])
                .expect("set events");
        }

        let router = make_crud_router(state);

        // WHEN GET /events
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/notifications/events")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 avec les events
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        let events = json["events"].as_array().expect("events array");
        assert_eq!(events.len(), 2);
        assert!(events.contains(&serde_json::json!("task.completed")));
        assert!(events.contains(&serde_json::json!("task.failed")));
    }

    // ── PUT /api/v1/notifications/events -> 200 ─────────────────────────────

    #[tokio::test]
    async fn test_set_events() {
        // GIVEN un repository vide
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN PUT avec de nouveaux events
        let body = serde_json::json!({
            "events": ["task.completed", "pipeline.failed"]
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/notifications/events")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 avec les events mis à jour
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        let events = json["events"].as_array().expect("events array");
        assert_eq!(events.len(), 2);
    }

    // ── Validation webhook sans URL -> 422 ──────────────────────────────────

    #[tokio::test]
    async fn test_validation_webhook_no_url_422() {
        // GIVEN
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN POST webhook sans url
        let body = serde_json::json!({
            "id": "bad-webhook",
            "channel_type": "webhook",
            "config": {}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 422 avec message de validation
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = read_body(resp).await;
        let error = json["error"].as_str().expect("error string");
        assert!(
            error.contains("url"),
            "expected error about url, got: {error}"
        );
    }

    // ── Validation event inconnu -> 422 ─────────────────────────────────────

    #[tokio::test]
    async fn test_validation_unknown_event_422() {
        // GIVEN
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN PUT events avec un event inconnu
        let body = serde_json::json!({
            "events": ["bad.event"]
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/notifications/events")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 422 avec message de validation
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = read_body(resp).await;
        let error = json["error"].as_str().expect("error string");
        assert!(
            error.contains("bad.event"),
            "expected error about bad.event, got: {error}"
        );
    }

    // ── Tests de types ──────────────────────────────────────────────────────

    #[test]
    fn test_channel_test_result_json_structure_ok() {
        // GIVEN
        let result = ChannelTestResult {
            channel_id: "desktop".to_string(),
            kind: "desktop".to_string(),
            status: "ok".to_string(),
            error: None,
            latency_ms: Some(12),
        };

        // WHEN
        let json = serde_json::to_value(&result).expect("sérialisation");

        // THEN
        assert_eq!(json["channel_id"], "desktop");
        assert_eq!(json["type"], "desktop");
        assert_eq!(json["status"], "ok");
        assert!(json["error"].is_null());
        assert_eq!(json["latency_ms"], 12);
    }

    #[test]
    fn test_channel_kind_str_all_variants() {
        assert_eq!(channel_kind_str(&ChannelKind::Desktop), "desktop");
        assert_eq!(channel_kind_str(&ChannelKind::Webhook), "webhook");
        assert_eq!(channel_kind_str(&ChannelKind::Sse), "sse");
    }

    #[test]
    fn test_channel_kind_by_id_found() {
        let config = NotificationConfig {
            events: vec![],
            channels: vec![ChannelConfig {
                id: "mon-desktop".to_string(),
                kind: ChannelKind::Desktop,
                enabled: true,
                events: None,
                url: None,
                signing_secret: None,
                min_severity: None,
            }],
            inactivity_timeout_secs: 30,
        };
        assert_eq!(channel_kind_by_id("mon-desktop", &config), "desktop");
    }

    #[test]
    fn test_channel_kind_by_id_not_found_returns_unknown() {
        let config = NotificationConfig {
            events: vec![],
            channels: vec![],
            inactivity_timeout_secs: 30,
        };
        assert_eq!(channel_kind_by_id("inconnu", &config), "unknown");
    }

    #[test]
    fn test_logs_lazy_table_creation_returns_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let result = query_notification_logs(&db_path, 20);
        let entries = result.expect("query_notification_logs");
        assert!(entries.is_empty());
    }
}
