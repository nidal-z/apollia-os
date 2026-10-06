//! Tauri IPC commands for declaring an inference server from the interface.
//!
//! Each command delegates to the runtime's REST API, so it serves both modes:
//! `POST /api/v1/llm/servers/test` lists the models of an address that is not
//! saved yet, `POST /api/v1/llm/servers` saves a server (its key going to the
//! secret store of the runtime), and `PUT /api/v1/llm/default-model` makes one
//! model the default of the machine.
//!
//! A refused test reaches the webview as a [`ServerTestFailure`]: the sentence
//! of the runtime, and the kind of thing to check, so the interface can say it
//! in the language of the person.

use apollia_runtime::governance::{REMOTE_SERVER_DENIED_REASON, SERVER_NOT_ALLOWED_REASON};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::attached::RuntimeLink;

const SERVERS_PATH: &str = "/api/v1/llm/servers";
const TEST_PATH: &str = "/api/v1/llm/servers/test";
const DEFAULT_MODEL_PATH: &str = "/api/v1/llm/default-model";

/// Why a server test did not list any model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServerTestFailure {
    /// `unreachable`, `key_refused`, `not_openai_compatible`, `policy`, or
    /// `other` for a refusal this application does not recognise.
    pub kind: &'static str,
    /// The sentence of the runtime.
    pub message: String,
}

/// The fragments that tell the refusals of a server test apart.
///
/// The runtime answers a kind beside its sentence, and the seam to the runtime
/// carries the sentence alone. The kind is read back from it here; the tests
/// below build each fragment's sentence from the type that writes it, so a
/// rewording fails them instead of silently turning every refusal into
/// `other`.
const KIND_FRAGMENTS: &[(&str, &str)] = &[
    ("cannot be reached", "unreachable"),
    ("refused the key", "key_refused"),
    (
        "is not an OpenAI-compatible server",
        "not_openai_compatible",
    ),
];

/// The kind of thing to check after a refusal that reads `message`.
fn failure_kind(message: &str) -> &'static str {
    if message == SERVER_NOT_ALLOWED_REASON || message == REMOTE_SERVER_DENIED_REASON {
        return "policy";
    }
    KIND_FRAGMENTS
        .iter()
        .find(|(fragment, _)| message.contains(fragment))
        .map_or("other", |(_, kind)| kind)
}

/// The key as the runtime expects it: absent when the field was left empty.
fn key_of(api_key: Option<String>) -> Option<String> {
    api_key
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
}

/// The model identifiers in the answer of a server test.
fn listed_models(answer: &Value) -> Vec<String> {
    answer
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Lists the models of the server at `base_url`, saving nothing.
///
/// The key, when one is given, is used for this call only.
#[tauri::command]
pub async fn test_llm_server(
    base_url: String,
    api_key: Option<String>,
    link: State<'_, RuntimeLink>,
) -> Result<Vec<String>, ServerTestFailure> {
    let body = json!({ "base_url": base_url.trim(), "api_key": key_of(api_key) });
    match link.post_json(TEST_PATH, &body).await {
        Ok(answer) => Ok(listed_models(&answer)),
        Err(message) => {
            let kind = failure_kind(&message);
            tracing::event!(tracing::Level::WARN, kind = %kind, "llm.server.test.refused");
            Err(ServerTestFailure { kind, message })
        }
    }
}

/// Saves the server `name` at `base_url`, replacing one of the same name. Its
/// models are routable as `<name>/<model>` once this returns.
#[tauri::command]
pub async fn save_llm_server(
    name: String,
    base_url: String,
    api_key: Option<String>,
    link: State<'_, RuntimeLink>,
) -> Result<Value, String> {
    let body = json!({
        "name": name,
        "base_url": base_url.trim(),
        "api_key": key_of(api_key),
    });
    link.post_json(SERVERS_PATH, &body).await
}

/// Makes `model_id` the default model of the machine. `model_source` is
/// `local` or `server`; a server model is named `<server>/<model>`.
#[tauri::command]
pub async fn set_default_model(
    model_source: String,
    model_id: String,
    link: State<'_, RuntimeLink>,
) -> Result<Value, String> {
    let body = json!({ "model_source": model_source, "model_id": model_id });
    link.put_json(DEFAULT_MODEL_PATH, &body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::server_models::ServerModelsError;

    #[test]
    fn each_refusal_of_the_runtime_is_read_back_as_its_own_kind() {
        // GIVEN the three refusals a server test can end on, as the type that
        // writes them words them
        let refusals = [
            ServerModelsError::Unreachable {
                url: "http://localhost:11434/v1/models".into(),
                reason: "connection refused".into(),
            },
            ServerModelsError::KeyRefused { status: 401 },
            ServerModelsError::NotOpenAiCompatible {
                url: "http://localhost:8080/v1/models".into(),
                reason: "HTTP 404".into(),
            },
        ];

        for refusal in refusals {
            // WHEN the sentence alone crosses the seam
            let kind = failure_kind(&refusal.to_string());

            // THEN the kind the runtime gave it is found again
            assert_eq!(kind, refusal.kind());
        }
    }

    #[test]
    fn a_refusal_of_the_organisation_policy_is_its_own_kind() {
        // GIVEN the two reasons the policy refuses a server with
        // WHEN each is read back
        // THEN both say the policy, not the server, is what to look at
        assert_eq!(failure_kind(SERVER_NOT_ALLOWED_REASON), "policy");
        assert_eq!(failure_kind(REMOTE_SERVER_DENIED_REASON), "policy");
    }

    #[test]
    fn an_unknown_refusal_keeps_its_sentence_under_the_other_kind() {
        // GIVEN a refusal no fragment names
        // WHEN it is read back
        // THEN it is not mistaken for one of the known kinds
        assert_eq!(
            failure_kind("LLM backend repository not available"),
            "other"
        );
    }

    #[test]
    fn an_empty_key_is_sent_as_no_key() {
        // GIVEN a key field left empty, or holding spaces only
        // WHEN the request is built
        // THEN no key goes to the runtime, so no header goes to the server
        assert_eq!(key_of(None), None);
        assert_eq!(key_of(Some(String::new())), None);
        assert_eq!(key_of(Some("   ".into())), None);
        // AND a real key is sent without the spaces around it
        assert_eq!(key_of(Some(" sk-test ".into())), Some("sk-test".into()));
    }

    #[test]
    fn the_models_of_a_test_are_read_in_the_order_of_the_server() {
        // GIVEN the answer of a successful test
        let answer = json!({ "models": ["llama3.2", "qwen2.5:14b"] });

        // WHEN it is read
        // THEN the identifiers come out as the server listed them
        assert_eq!(listed_models(&answer), vec!["llama3.2", "qwen2.5:14b"]);
        // AND an answer of another shape lists nothing
        assert!(listed_models(&json!({})).is_empty());
    }
}
