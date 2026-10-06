//! What the chat can talk to, and the route of a model chosen for a session.
//!
//! The selector of a conversation lists the agents in force and the models the
//! chat can run on. `list_chat_targets` gathers them from the routes the
//! runtime already serves, so the list reads the same thing whether the
//! runtime runs in this process or as a daemon:
//!
//! - the agents, as resolved: `GET /api/v1/agent-definitions`;
//! - the models installed on the machine: the model index;
//! - the declared servers and their models: `GET /api/v1/llm/servers`, then
//!   `GET /api/v1/llm/servers/{name}/models`;
//! - the default model: `GET /api/v1/llm/default-model`.
//!
//! Each part is read on its own: one that fails leaves the others listed, and
//! a server that does not answer is listed with the reason.
//!
//! A model chosen in this process is routed by [`embedded_session_route`]; a
//! daemon routes the model it is given itself.

use apollia_core::paths::DataFile;
use apollia_core::LlmBackendRepository;
use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::attached::{segment, RuntimeLink};
use crate::SharedLlmRouter;

/// The models of one declared server, or why they could not be read.
#[derive(Debug, Clone, Serialize)]
pub struct ServerModelsView {
    /// Server name, the first segment of `<server>/<model>`.
    pub server: String,
    /// The models as `GET /api/v1/llm/servers/{name}/models` lists them.
    pub models: Vec<Value>,
    /// Why the server did not list its models, `None` when it did.
    pub error: Option<String>,
}

/// Everything the selector of a conversation can offer.
#[derive(Debug, Clone, Serialize)]
pub struct ChatTargets {
    /// The agents in force, as `GET /api/v1/agent-definitions` resolves them.
    pub agents: Vec<Value>,
    /// The models installed on the machine, every category included: the
    /// selector keeps the language models.
    pub installed: Vec<Value>,
    /// The declared servers, each with its models.
    pub servers: Vec<ServerModelsView>,
    /// The default model of the machine, `None` when none is set.
    pub default_model: Option<Value>,
}

/// `GET path` on the runtime `link` drives.
async fn get_on(link: &RuntimeLink, path: &str) -> Result<Value, String> {
    match link {
        RuntimeLink::Embedded(runtime) => super::super::http_get_json(runtime.api_port, path).await,
        RuntimeLink::Attached(remote) => remote.get::<Value>(path).await.map_err(Into::into),
    }
}

/// The array a route answers, empty when it answers something else or fails.
fn listed(part: &str, answer: Result<Value, String>) -> Vec<Value> {
    match answer {
        Ok(Value::Array(items)) => items,
        Ok(_) => Vec::new(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                part = %part,
                error = %error,
                "chat.targets.unread"
            );
            Vec::new()
        }
    }
}

/// The names of the declared servers in the answer of `GET /api/v1/llm/servers`.
fn server_names(answer: &Value) -> Vec<String> {
    answer
        .get("servers")
        .and_then(Value::as_array)
        .map(|servers| {
            servers
                .iter()
                .filter_map(|s| s.get("name").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The agents and the models the chat can talk to.
#[tauri::command]
pub async fn list_chat_targets(link: State<'_, RuntimeLink>) -> Result<ChatTargets, String> {
    let link = link.inner();
    let agents = listed("agents", get_on(link, "/api/v1/agent-definitions").await);
    let installed_answer = match link {
        RuntimeLink::Embedded(_) => crate::commands::model_hub::local_installed_models().await,
        RuntimeLink::Attached(remote) => remote
            .get::<Value>("/api/v1/llm/installed-models")
            .await
            .map_err(Into::into),
    };
    let installed = listed("installed", installed_answer);
    let names = match get_on(link, "/api/v1/llm/servers").await {
        Ok(answer) => server_names(&answer),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                part = "servers",
                error = %error,
                "chat.targets.unread"
            );
            Vec::new()
        }
    };
    let mut servers = Vec::with_capacity(names.len());
    for server in names {
        let path = format!("/api/v1/llm/servers/{}/models", segment(&server));
        let view = match get_on(link, &path).await {
            Ok(answer) => ServerModelsView {
                models: answer
                    .get("models")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                error: None,
                server,
            },
            Err(error) => ServerModelsView {
                models: Vec::new(),
                error: Some(error),
                server,
            },
        };
        servers.push(view);
    }
    let default_model = get_on(link, "/api/v1/llm/default-model").await.ok();
    Ok(ChatTargets {
        agents,
        installed,
        servers,
        default_model,
    })
}

/// The name the router of this process routes `model` by.
///
/// A language model of the index chosen by its identifier gets its backend
/// row, and the routers are rebuilt when that row is new, so the next turn of
/// the session reaches it. A store that cannot be read leaves `model` as it
/// was asked: the turn then reports the fallback it makes.
pub(crate) async fn embedded_session_route(
    app: &tauri::AppHandle,
    runtime: &RuntimeHandle,
    model: String,
) -> String {
    let data_dir = apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp());
    let asked = model.clone();
    let routed = tokio::task::spawn_blocking(move || {
        let repo = LlmBackendRepository::open(&DataFile::System.path(&data_dir))
            .map_err(|e| e.to_string())?;
        let models_dir = apollia_runtime::installed_models::models_dir(&data_dir);
        apollia_runtime::local_engine::chat_route(&repo, &models_dir, &model)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|routed| routed);
    let route = match routed {
        Ok(route) => route,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                model = %asked,
                error = %error,
                "chat.session.model.unrouted"
            );
            return asked;
        }
    };
    if route.written {
        let rebuilt = match tauri::Manager::try_state::<SharedLlmRouter>(app) {
            Some(shared) => super::super::llm::rebuild_desktop_router(&shared, runtime).await,
            None => Err("LLM router not available".to_owned()),
        };
        if let Err(error) = rebuilt {
            tracing::event!(
                tracing::Level::WARN,
                model = %asked,
                backend = %route.name,
                error = %error,
                "chat.session.model.router_not_rebuilt"
            );
        }
    }
    route.name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_names_are_read_from_the_listing_and_nothing_else() {
        // GIVEN the answer of the server listing, and an answer of another shape
        let answer = serde_json::json!({
            "servers": [{"name": "lab", "base_url": "http://lab"}, {"base_url": "x"}]
        });
        let other = serde_json::json!(["lab"]);

        // WHEN their names are read
        let names = server_names(&answer);
        let none = server_names(&other);

        // THEN only the named server is kept
        assert_eq!(names, vec!["lab".to_owned()]);
        assert!(none.is_empty());
    }

    #[test]
    fn a_part_that_fails_or_is_not_a_list_is_listed_empty() {
        // GIVEN a listing, an answer of another shape, and a failure
        let items = Ok(serde_json::json!([{"id": "a"}]));
        let object = Ok(serde_json::json!({"id": "a"}));
        let failed = Err("API error (500)".to_owned());

        // WHEN each is listed
        let (one, zero, empty) = (
            listed("agents", items),
            listed("agents", object),
            listed("agents", failed),
        );

        // THEN only the listing keeps its items
        assert_eq!(one.len(), 1);
        assert!(zero.is_empty());
        assert!(empty.is_empty());
    }
}
