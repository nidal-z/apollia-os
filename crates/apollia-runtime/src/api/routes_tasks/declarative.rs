//! A task on a stored agent that runs no code.
//!
//! Such an agent is a definition and the user's overrides, with no Python
//! behind it. Its task is run by the agent executor on the message the input
//! carries, outside any conversation; every other agent keeps the coordinator
//! path.

use axum::http::StatusCode;
use axum::Json;

use apollia_core::TaskId;
use apollia_tools::ResolvedAgent;

use super::{json_to_aip_input, ErrorResponse};
use crate::agents::task::{message_of, resolve_without_code, AgentTaskError, AgentTaskLauncher};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

type RouteError = (StatusCode, Json<ErrorResponse>);

fn error(status: StatusCode, message: impl ToString) -> RouteError {
    (
        status,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// The agent `name` as resolved, when it is stored in `agents.db` and runs no
/// code. `None` for an agent that runs code, for an unknown name, and when the
/// runtime has no executor to run it with.
///
/// # Errors
///
/// A 500 when `agents.db` or the model store cannot be read.
pub(super) fn stored_agent_without_code<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    name: &str,
) -> Result<Option<ResolvedAgent>, RouteError> {
    if state.chat_manager.is_none() {
        return Ok(None);
    }
    let guard = match state.llm_backend_repo.as_ref() {
        Some(llm) => Some(llm.lock().map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "model store lock poisoned",
            )
        })?),
        None => None,
    };
    resolve_without_code(&state.data_dir, guard.as_deref(), name)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// Run `agent` on the message `input` carries.
///
/// # Errors
///
/// A 503 when the agent is disabled or the executor cannot start the run,
/// a 500 when the task cannot be recorded.
pub(super) async fn launch<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    agent: &ResolvedAgent,
    input: serde_json::Value,
) -> Result<TaskId, RouteError> {
    let Some(chat) = state.chat_manager.clone() else {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "no agent executor in this runtime",
        ));
    };
    let message = message_of(&json_to_aip_input(input.clone()));
    let launcher = AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat,
        repository: state.task_repository.clone(),
        event_bus: state.event_sender.clone(),
        observability: state.obs_config.clone(),
        plan_cache: state.plan_cache.clone(),
    };
    launcher
        .launch(agent, &input, message)
        .await
        .map_err(|e| match e {
            AgentTaskError::Store(_) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
            other => error(StatusCode::SERVICE_UNAVAILABLE, other),
        })
}

#[cfg(test)]
mod tests;
