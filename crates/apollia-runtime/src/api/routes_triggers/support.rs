//! Shared plumbing behind the trigger routes.
//!
//! Row-to-response projection, the repository and engine accessors every
//! handler starts from, and the error mapping the whole module answers with.

use axum::http::StatusCode;
use axum::Json;

use apollia_triggers::{
    DefinitionRepositoryError, OnBusy, TriggerDefinitionRepository, TriggerDefinitionRow,
};

use crate::api::routes_triggers::{ErrorResponse, TriggerDefinitionResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Converts a [`TriggerDefinitionRow`] into a [`TriggerDefinitionResponse`].
pub(super) fn row_to_response(row: TriggerDefinitionRow) -> TriggerDefinitionResponse {
    let on_busy = match row.on_busy {
        OnBusy::Queue => "queue".to_string(),
        OnBusy::Drop => "drop".to_string(),
    };
    TriggerDefinitionResponse {
        id: row.id,
        agent: row.agent,
        enabled: row.enabled,
        on_busy,
        source_type: row.source_type,
        source_config: row.source_config,
        input_template: row.input_template,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Parses the `on_busy` field from the request (default: `"queue"`).
pub(super) fn parse_on_busy(
    value: Option<&str>,
) -> Result<OnBusy, (StatusCode, Json<ErrorResponse>)> {
    match value {
        None | Some("queue") => Ok(OnBusy::Queue),
        Some("drop") => Ok(OnBusy::Drop),
        Some(other) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: format!("validation error: invalid on_busy value: {other}"),
            }),
        )),
    }
}

/// Maps a [`DefinitionRepositoryError`] to a `(StatusCode, Json<ErrorResponse>)` tuple.
pub(super) fn map_repo_error(err: DefinitionRepositoryError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, msg) = match &err {
        DefinitionRepositoryError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        DefinitionRepositoryError::DuplicateId(_) => (StatusCode::CONFLICT, err.to_string()),
        DefinitionRepositoryError::ValidationError(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
        DefinitionRepositoryError::Database(_) | DefinitionRepositoryError::Schema(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
        // The enum is #[non_exhaustive]: a variant added upstream reaches the
        // client as a 500 carrying its own message, never as a compile break.
        _ => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };
    (status, Json(ErrorResponse { error: msg }))
}

/// Extracts the repository from state, returns 503 if absent.
pub(super) fn require_repo<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<
    &std::sync::Arc<std::sync::Mutex<TriggerDefinitionRepository>>,
    (StatusCode, Json<ErrorResponse>),
> {
    state.trigger_def_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "trigger definition repository not available".into(),
            }),
        )
    })
}

/// Reloads the `TriggerEngine` from the repository definitions (fire-and-forget).
///
/// Reads all definitions, converts them to rich types, and sends them to the engine.
/// Conversion errors are silently ignored: the trigger stays as-is in the DB but is
/// not loaded at runtime.
pub(super) async fn reload_engine_from_repo<B: ExecutionBackend + Clone>(state: &AppState<B>) {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => return,
    };
    let repo = match &state.trigger_def_repo {
        Some(r) => r.clone(),
        None => return,
    };

    let definitions = {
        let guard = match repo.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match guard.list() {
            Ok(rows) => rows,
            Err(_) => return,
        }
    };

    let rich_defs: Vec<_> = definitions
        .into_iter()
        .filter_map(|row| apollia_triggers::TriggerDefinition::try_from(row).ok())
        .collect();

    engine.reload(rich_defs).await;
}

/// Returns the source type string for use in the `TriggerStatus` fallback.
pub(super) fn source_detail_kind(source: &apollia_triggers::TriggerSourceConfig) -> String {
    source_kind_and_detail(source).0
}

/// Returns `(kind, detail)` from a [`TriggerSourceConfig`].
pub(super) fn source_kind_and_detail(
    source: &apollia_triggers::TriggerSourceConfig,
) -> (String, String) {
    use apollia_triggers::TriggerSourceConfig;
    match source {
        TriggerSourceConfig::Cron { schedule } => ("cron".into(), schedule.clone()),
        TriggerSourceConfig::Interval { every } => ("interval".into(), every.clone()),
        TriggerSourceConfig::Oneshot { fire_at } => ("oneshot".into(), fire_at.to_rfc3339()),
        TriggerSourceConfig::FileWatch { path, events, .. } => {
            let evts: Vec<_> = events.iter().map(|e| e.to_string()).collect();
            (
                "file_watch".into(),
                format!("{} [{}]", path.display(), evts.join(",")),
            )
        }
        TriggerSourceConfig::Webhook { .. } => ("webhook".into(), String::new()),
    }
}
