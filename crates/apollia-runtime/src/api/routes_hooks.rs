//! REST route for lifecycle hooks.
//!
//! - `GET /api/v1/hooks`, the lifecycle hook handlers registered at startup.
//!
//! Returns an empty array when no hooks are configured or when the chat manager
//! (which owns the shared hook executor) is not running.

use axum::extract::State;
use axum::Json;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::hooks::HookHandlerSummary;

/// `GET /api/v1/hooks`, list every registered lifecycle hook handler.
///
/// The response is a JSON array of [`HookHandlerSummary`] in declaration order.
#[utoipa::path(
    get,
    path = "/api/v1/hooks",
    tag = "governance",
    responses(
        (status = 200, description = "Registered lifecycle hook handlers in declaration order"),
    )
)]
pub async fn list_hooks<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<Vec<HookHandlerSummary>> {
    let summaries = state
        .chat_manager
        .as_ref()
        .map(|manager| manager.hook_summaries())
        .unwrap_or_default();
    Json(summaries)
}
