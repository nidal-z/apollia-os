//! Commande IPC Tauri pour la trace d'exécution event-sourced (ADR-088).
//!
//! Délègue à l'endpoint REST `/api/v1/tasks/:id/trace` exposé par le
//! `apollia-runtime` ; pas de lecture directe de `runtime_events.db` ici
//! pour garder un point d'entrée unique (ACL future, header dépréciation).
//!
//! Le frontend appelle `invoke("get_task_trace", { taskId, since, limit })`
//! et reçoit un `TraceResponse` avec la liste paginée d'événements.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_get_json;

/// Représentation TS-friendly d'un `RuntimeEventRecord` (camelCase).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEventDto {
    /// UUID v7 — ordonnable lexicographiquement.
    pub event_id: String,
    /// Tâche.
    pub task_id: String,
    /// Agent émetteur.
    pub agent_id: String,
    /// Lien parent (tool_call_completed → started, etc.).
    pub parent_event_id: Option<String>,
    /// ID partagé sur une chaîne A2A.
    pub correlation_id: Option<String>,
    /// Tour ReAct (NULL hors loop).
    pub step_num: Option<i64>,
    /// Discriminant — ex `tool_call_started`, `thought`, `agent_log`…
    pub kind: String,
    /// Payload typé par kind, gardé en `Value` pour passer brut au front.
    pub payload: serde_json::Value,
    /// ISO 8601 RFC 3339 millisecondes.
    pub ts: String,
}

/// Réponse paginée.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceResponse {
    /// Tâche concernée (echo).
    pub task_id: String,
    /// Événements ordonnés chronologiquement (UUIDv7 ASC).
    pub events: Vec<RuntimeEventDto>,
    /// Curseur à passer en `since` au prochain appel.
    pub next_cursor: Option<String>,
}

/// Paramètres d'appel.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTraceParams {
    /// Tâche cible.
    pub task_id: String,
    /// Curseur de pagination (event_id UUIDv7).
    #[serde(default)]
    pub since: Option<String>,
    /// Nombre maximum d'événements à retourner (défaut 500, max 5000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Récupère la trace d'exécution d'une tâche.
///
/// Délègue à `GET /api/v1/tasks/:task_id/trace?since=…&limit=…` exposé par
/// le runtime. Parse la réponse JSON en `TraceResponse` (le `payload_json`
/// brut côté serveur est désérialisé en `serde_json::Value` ici pour offrir
/// au front un objet plutôt qu'une string à reparser).
#[tauri::command]
pub async fn get_task_trace(
    state: State<'_, RuntimeHandle>,
    params: GetTraceParams,
) -> Result<TraceResponse, String> {
    let port = state.api_port;

    let mut path = format!("/api/v1/tasks/{}/trace", params.task_id);
    let mut query: Vec<String> = Vec::new();
    if let Some(since) = params.since.as_deref() {
        query.push(format!("since={}", urlencoding::encode(since)));
    }
    if let Some(limit) = params.limit {
        query.push(format!("limit={limit}"));
    }
    if !query.is_empty() {
        path.push('?');
        path.push_str(&query.join("&"));
    }

    let raw = http_get_json(port, &path).await?;

    // Le serveur renvoie `payload_json: String` ; on le parse en Value pour
    // donner un objet exploitable côté TS. Si le parse échoue (ne devrait
    // pas), on conserve la string en tant que Value::String.
    let task_id = raw
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&params.task_id)
        .to_string();

    let next_cursor = raw
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let events_raw = raw
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut events = Vec::with_capacity(events_raw.len());
    for ev in events_raw {
        let payload_str = ev
            .get("payload_json")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");
        let payload: serde_json::Value = serde_json::from_str(payload_str)
            .unwrap_or_else(|_| serde_json::Value::String(payload_str.to_string()));

        events.push(RuntimeEventDto {
            event_id: ev
                .get("event_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            task_id: ev
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            agent_id: ev
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            parent_event_id: ev
                .get("parent_event_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            correlation_id: ev
                .get("correlation_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            step_num: ev.get("step_num").and_then(|v| v.as_i64()),
            kind: ev
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            payload,
            ts: ev
                .get("ts")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        });
    }

    Ok(TraceResponse {
        task_id,
        events,
        next_cursor,
    })
}
