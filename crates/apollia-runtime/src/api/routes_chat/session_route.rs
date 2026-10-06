//! The route a session names for the model chosen for it.
//!
//! The chat chooses a model by the identifier the model index gives it, or by
//! `<server>/<model>` for a model of a declared server. A local model is
//! routed by its backend row: choosing it writes that row when it has none
//! yet, and the router is then rebuilt so the next turn reaches it.

use axum::extract::State;

use crate::api::routes_llm::backends::{map_backend_error, reload_llm_router};
use crate::api::routes_llm::servers::with_repo;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// The name the router routes `model` by. A store that cannot be read leaves
/// `model` as it was asked: the turn then reports the fallback it makes.
pub(crate) async fn session_route<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    model: String,
) -> String {
    let models_dir = crate::installed_models::models_dir(&state.data_dir);
    let routed = with_repo(state, |repo| {
        crate::local_engine::chat_route(repo, &models_dir, &model).map_err(map_backend_error)
    });
    let route = match routed {
        Ok(route) => route,
        Err((status, _)) => {
            tracing::event!(
                tracing::Level::WARN,
                model = %model,
                status = %status,
                "chat.session.model.unrouted"
            );
            return model;
        }
    };
    if route.written {
        if let Err((status, _)) = reload_llm_router(State(state.clone())).await {
            tracing::event!(
                tracing::Level::WARN,
                model = %model,
                backend = %route.name,
                status = %status,
                "chat.session.model.router_not_rebuilt"
            );
        }
    }
    route.name
}
