//! `GET /api/v1/tasks/{id}/trace` — event-sourced execution trace (ADR-088).
//!
//! Source de vérité unique : table `runtime_events` peuplée par
//! [`crate::observability::EventPersistorHandle`]. Pagination par curseur
//! UUIDv7 (`?since=<event_id>`) — pas d'`OFFSET`, l'ordre lexicographique
//! des UUIDv7 est l'ordre causal.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::observability::{RuntimeEventRecord, RuntimeEventsRepository};

/// Limite par défaut sur le nombre d'événements retournés par appel.
///
/// Suffit pour rendre 95 % des traces typiques (< 200 events) en un seul
/// fetch. Les longues traces sont paginées via `?since=<event_id>` avec un
/// `limit` configurable (jusqu'à 5000).
const DEFAULT_LIMIT: usize = 500;

/// Borne haute pour éviter qu'un client demande l'intégralité d'une base.
const MAX_LIMIT: usize = 5000;

/// Paramètres de query string.
#[derive(Debug, Deserialize)]
pub struct TraceQuery {
    /// Curseur de pagination — ne retourne que les événements *strictement*
    /// après cet `event_id` (UUIDv7 lex-ordonnable). Absent = depuis le début.
    #[serde(default)]
    pub since: Option<String>,
    /// Nombre maximum d'événements à retourner. Borné à `MAX_LIMIT`.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Réponse JSON.
#[derive(Debug, Serialize)]
pub struct TraceResponse {
    /// Identifiant de la tâche.
    pub task_id: String,
    /// Événements ordonnés chronologiquement (UUIDv7 ASC).
    pub events: Vec<RuntimeEventRecord>,
    /// Curseur à passer en `?since=` au prochain appel pour récupérer la
    /// suite. `None` quand la page courante atteint la fin connue.
    pub next_cursor: Option<String>,
}

/// Réponse d'erreur structurée.
#[derive(Debug, Serialize)]
pub struct TraceErrorResponse {
    /// Message d'erreur.
    pub error: String,
}

/// Handler `GET /api/v1/tasks/{id}/trace`.
///
/// 200 : `TraceResponse` (peut être vide si la tâche n'a encore produit
/// aucun événement persisté).
/// 500 : erreur d'ouverture de la base ou de requête.
pub async fn get_task_trace<B: ExecutionBackend + Clone>(
    Path(task_id): Path<String>,
    Query(q): Query<TraceQuery>,
    State(state): State<AppState<B>>,
) -> Result<Json<TraceResponse>, (StatusCode, Json<TraceErrorResponse>)> {
    let limit = q
        .limit
        .unwrap_or(DEFAULT_LIMIT)
        .min(MAX_LIMIT)
        .max(1);
    let since = q.since.clone();
    let task_id_for_query = task_id.clone();

    let db_path = resolve_runtime_events_db(&state);

    let result = tokio::task::spawn_blocking(move || -> Result<Vec<RuntimeEventRecord>, String> {
        let repo = RuntimeEventsRepository::open(&db_path)
            .map_err(|e| format!("open runtime_events: {e}"))?;
        repo.list_for_task(&task_id_for_query, since.as_deref(), limit)
            .map_err(|e| format!("query runtime_events: {e}"))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TraceErrorResponse {
                error: format!("blocking task panicked: {e}"),
            }),
        )
    })?;

    let events = result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TraceErrorResponse { error: e }),
        )
    })?;

    // Curseur : si on a renvoyé exactement `limit` événements, il en reste
    // probablement d'autres — exposer le dernier event_id comme prochain
    // curseur. Sinon (page partielle), aucun cursor.
    let next_cursor = if events.len() == limit {
        events.last().map(|e| e.event_id.clone())
    } else {
        None
    };

    Ok(Json(TraceResponse {
        task_id,
        events,
        next_cursor,
    }))
}

/// Résout le chemin de `runtime_events.db` depuis l'`AppState`.
///
/// Aujourd'hui même heuristique que `routes_timeline::resolve_data_dir`
/// (`$HOME/.apollia/`). Quand la config dir devient un champ structuré dans
/// l'`AppState`, déplacer ce helper vers `api::server`.
fn resolve_runtime_events_db<B: ExecutionBackend + Clone>(
    _state: &AppState<B>,
) -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(format!("{h}/.apollia")))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/apollia"));
    base.join("runtime_events.db")
}
