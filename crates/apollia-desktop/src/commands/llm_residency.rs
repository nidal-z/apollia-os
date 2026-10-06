//! Tauri IPC commands for the residency of local models: the lease a
//! conversation holds on its model while it is open, and the pin that keeps
//! a model in memory.
//!
//! Each command delegates to the internal REST API: `POST /api/v1/llm/leases`
//! and `POST /api/v1/llm/leases/release` for the lease,
//! `GET` and `PUT /api/v1/llm/pins` for the pin. A lease is held in the name
//! of this application, which names itself once per process; attached to a
//! daemon, the event stream carries the same name, so the daemon releases
//! the leases of an application that went away.

use std::sync::OnceLock;

use crate::attached::RuntimeLink;
use serde_json::Value;
use tauri::State;

const LEASES_PATH: &str = "/api/v1/llm/leases";
const RELEASE_PATH: &str = "/api/v1/llm/leases/release";
const PINS_PATH: &str = "/api/v1/llm/pins";

/// The name this application holds its leases under, drawn once per process.
pub(crate) fn client_id() -> &'static str {
    static CLIENT_ID: OnceLock<String> = OnceLock::new();
    CLIENT_ID.get_or_init(|| format!("desktop-{}", uuid::Uuid::new_v4()))
}

fn open_body(client: &str, conversation_id: &str, model_path: &str) -> Value {
    serde_json::json!({
        "clientId": client,
        "conversationId": conversation_id,
        "modelPath": model_path,
    })
}

fn release_body(client: &str, conversation_id: &str) -> Value {
    serde_json::json!({ "clientId": client, "conversationId": conversation_id })
}

fn pin_body(model_path: &str, pinned: bool) -> Value {
    serde_json::json!({ "modelPath": model_path, "pinned": pinned })
}

/// Opens the lease of a conversation on the local model it talks to; a lease
/// the conversation already held on another model moves.
///
/// Delegates to `POST /api/v1/llm/leases`.
#[tauri::command]
pub async fn open_model_lease(
    state: State<'_, RuntimeLink>,
    conversation_id: String,
    model_path: String,
) -> Result<Value, String> {
    state
        .post_json(
            LEASES_PATH,
            &open_body(client_id(), &conversation_id, &model_path),
        )
        .await
}

/// Releases the lease of a conversation; the runtime unloads a model whose
/// last lease this was, unless it is pinned.
///
/// Delegates to `POST /api/v1/llm/leases/release`.
#[tauri::command]
pub async fn release_model_lease(
    state: State<'_, RuntimeLink>,
    conversation_id: String,
) -> Result<Value, String> {
    state
        .post_json(RELEASE_PATH, &release_body(client_id(), &conversation_id))
        .await
}

/// Lists the paths of the models kept in memory.
///
/// Delegates to `GET /api/v1/llm/pins`.
#[tauri::command]
pub async fn list_model_pins(state: State<'_, RuntimeLink>) -> Result<Value, String> {
    state.get_json(PINS_PATH).await
}

/// Keeps a model in memory, or stops keeping it.
///
/// Delegates to `PUT /api/v1/llm/pins`.
#[tauri::command]
pub async fn set_model_pin(
    state: State<'_, RuntimeLink>,
    model_path: String,
    pinned: bool,
) -> Result<Value, String> {
    state
        .put_json(PINS_PATH, &pin_body(&model_path, pinned))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_application_holds_every_lease_under_one_name() {
        // GIVEN the name this application draws for itself
        let first = client_id();

        // WHEN it is asked again, and two leases are built
        let again = client_id();
        let open = open_body(first, "conv-1", "/models/a.gguf");
        let release = release_body(again, "conv-1");

        // THEN the name is stable, and the release names the client and the
        // conversation the lease was opened under
        assert!(first.starts_with("desktop-"));
        assert_eq!(first, again);
        assert_eq!(open["clientId"], release["clientId"]);
        assert_eq!(open["conversationId"], release["conversationId"]);
        assert_eq!(open["modelPath"], "/models/a.gguf");
    }

    #[test]
    fn test_a_pin_names_the_model_by_its_path_in_the_casing_of_the_route() {
        // GIVEN a model to keep in memory
        // WHEN the body of the pin is built
        let body = pin_body("/models/a.gguf", true);

        // THEN the route reads the path and the choice
        assert_eq!(
            body,
            serde_json::json!({ "modelPath": "/models/a.gguf", "pinned": true })
        );
    }
}
