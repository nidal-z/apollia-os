//! The models an OpenAI-compatible server offers, read from `GET /v1/models`.
//!
//! Ollama, vLLM, TGI and LiteLLM all answer that route, with or without a key,
//! so one call serves both as the connection test before a server is saved and
//! as the model list of a saved one. Nothing is written here.

use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use apollia_core::inference_server::models_url;
use apollia_core::net::{
    configured_endpoint_client_builder_for, read_capped_json, ReadCappedError,
};

/// Largest model list accepted. A gateway in front of many providers lists a
/// few thousand entries at most, each well under a kilobyte.
const MAX_MODELS_BODY: u64 = 4 * 1024 * 1024;

/// How long the server has to accept the connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the whole listing may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a server's models could not be listed. Each variant is a different
/// thing for the person declaring the server to fix.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ServerModelsError {
    /// Nothing answered at the address: wrong host or port, server down,
    /// network unreachable, or no answer in time.
    #[error("the server at {url} cannot be reached: {reason}")]
    Unreachable {
        /// URL that was called.
        url: String,
        /// Transport-level cause.
        reason: String,
    },
    /// The server answered 401 or 403: the key is missing, wrong or revoked.
    #[error("the server refused the key (HTTP {status})")]
    KeyRefused {
        /// HTTP status the server answered.
        status: u16,
    },
    /// Something answered, but not an OpenAI-compatible model list.
    #[error("the server at {url} is not an OpenAI-compatible server: {reason}")]
    NotOpenAiCompatible {
        /// URL that was called.
        url: String,
        /// What was answered instead.
        reason: String,
    },
}

impl ServerModelsError {
    /// Stable machine-readable kind: `unreachable`, `key_refused` or
    /// `not_openai_compatible`.
    pub fn kind(&self) -> &'static str {
        match self {
            ServerModelsError::Unreachable { .. } => "unreachable",
            ServerModelsError::KeyRefused { .. } => "key_refused",
            ServerModelsError::NotOpenAiCompatible { .. } => "not_openai_compatible",
        }
    }
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    /// The window vLLM serves the model with. Read loosely: a server that
    /// puts something else under this key loses the window, not the list.
    #[serde(default)]
    max_model_len: Option<serde_json::Value>,
}

/// One model a server lists, with the context window it declares for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedModel {
    /// Identifier the server gives the model.
    pub id: String,
    /// Context window the server serves the model with, in tokens, when the
    /// listing says (`max_model_len` on vLLM). `None` when it does not.
    pub context_window: Option<u32>,
}

/// List the model identifiers a server offers, in the order it gives them.
///
/// Calls `GET <address>/v1/models`, or `<address>/models` when the address
/// already ends with `/v1`. With a key, the call carries
/// `Authorization: Bearer <key>`; without one, no `Authorization` header at all.
///
/// # Errors
///
/// See [`ServerModelsError`]: one variant per thing the operator has to fix.
pub async fn list_server_models(
    address: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ServerModelsError> {
    Ok(list_server_model_entries(address, api_key)
        .await?
        .into_iter()
        .map(|m| m.id)
        .collect())
}

/// [`list_server_models`], each model with the context window the listing
/// declares for it.
///
/// # Errors
///
/// See [`ServerModelsError`].
pub async fn list_server_model_entries(
    address: &str,
    api_key: Option<&str>,
) -> Result<Vec<ListedModel>, ServerModelsError> {
    let url = models_url(address);
    let unreachable = |reason: String| ServerModelsError::Unreachable {
        url: url.clone(),
        reason,
    };
    let not_compatible = |reason: String| ServerModelsError::NotOpenAiCompatible {
        url: url.clone(),
        reason,
    };

    let client = configured_endpoint_client_builder_for(&url)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| unreachable(e.to_string()))?;
    let mut request = client.get(&url);
    if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|e| {
        if e.is_builder() {
            not_compatible(format!("invalid address: {e}"))
        } else {
            unreachable(e.to_string())
        }
    })?;

    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(ServerModelsError::KeyRefused {
            status: status.as_u16(),
        });
    }
    if !status.is_success() {
        return Err(not_compatible(format!("HTTP {status}")));
    }
    let list: ModelList =
        read_capped_json(response, MAX_MODELS_BODY)
            .await
            .map_err(|e| match e {
                ReadCappedError::Transport(e) => unreachable(e.to_string()),
                other => not_compatible(other.to_string()),
            })?;

    tracing::event!(
        tracing::Level::DEBUG,
        url = %url,
        models = list.data.len(),
        "llm.server.models.listed"
    );
    Ok(list
        .data
        .into_iter()
        .map(|m| ListedModel {
            context_window: m
                .max_model_len
                .as_ref()
                .and_then(serde_json::Value::as_u64)
                .filter(|n| *n > 0)
                .and_then(|n| u32::try_from(n).ok()),
            id: m.id,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    fn model_list() -> serde_json::Value {
        serde_json::json!({
            "object": "list",
            "data": [
                { "id": "qwen3-32b", "object": "model", "owned_by": "vllm" },
                { "id": "Qwen/Qwen3-8B", "object": "model", "owned_by": "vllm" }
            ]
        })
    }

    async fn server_answering(status: u16, body: serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&server)
            .await;
        server
    }

    // GIVEN a server listing two models on `/v1/models`
    // WHEN it is listed by its bare address, then by its address ending in `/v1`
    // THEN both calls land on `/v1/models` and return the identifiers in order
    #[tokio::test]
    async fn test_lists_the_model_identifiers_with_or_without_v1() {
        // GIVEN
        let server = server_answering(200, model_list()).await;

        // WHEN
        let bare = list_server_models(&server.uri(), None).await.unwrap();
        let with_v1 = list_server_models(&format!("{}/v1/", server.uri()), None)
            .await
            .unwrap();

        // THEN
        assert_eq!(
            bare,
            vec!["qwen3-32b".to_string(), "Qwen/Qwen3-8B".to_string()]
        );
        assert_eq!(with_v1, bare);
    }

    // GIVEN an address where nothing can listen
    // WHEN its models are listed
    // THEN the error says the server cannot be reached
    #[tokio::test]
    async fn test_nothing_listening_is_unreachable() {
        // GIVEN
        // Port 0 is never a destination: the connection fails on every
        // machine, with no port reserved and released for the occasion.
        let address = "http://127.0.0.1:0".to_string();

        // WHEN
        let err = list_server_models(&address, None).await.unwrap_err();

        // THEN
        assert_eq!(err.kind(), "unreachable", "{err}");
    }

    // GIVEN a server answering 401, and one answering 403
    // WHEN their models are listed with a key
    // THEN both errors say the key was refused, with the status
    #[tokio::test]
    async fn test_401_and_403_are_a_refused_key() {
        for status in [401u16, 403] {
            // GIVEN
            let server =
                server_answering(status, serde_json::json!({"error": "invalid api key"})).await;

            // WHEN
            let err = list_server_models(&server.uri(), Some("wrong"))
                .await
                .unwrap_err();

            // THEN
            assert!(
                matches!(err, ServerModelsError::KeyRefused { status: s } if s == status),
                "{err}"
            );
            assert_eq!(err.kind(), "key_refused");
        }
    }

    // GIVEN a web server with no `/v1/models`, and one answering a JSON that
    //       is not a model list
    // WHEN their models are listed
    // THEN both errors say the server is not OpenAI-compatible
    #[tokio::test]
    async fn test_a_404_or_a_foreign_body_is_not_openai_compatible() {
        // GIVEN
        let no_route = MockServer::start().await;
        let foreign = server_answering(200, serde_json::json!({"models": ["a"]})).await;

        for server in [&no_route, &foreign] {
            // WHEN
            let err = list_server_models(&server.uri(), None).await.unwrap_err();

            // THEN
            assert_eq!(err.kind(), "not_openai_compatible", "{err}");
        }
    }

    // GIVEN a server that records the requests it receives
    // WHEN its models are listed once without a key and once with one
    // THEN the first request carries no Authorization header at all, and the
    //      second carries `Bearer <key>`
    #[tokio::test]
    async fn test_authorization_is_sent_only_with_a_key() {
        // GIVEN
        let server = server_answering(200, model_list()).await;

        // WHEN
        list_server_models(&server.uri(), None).await.unwrap();
        list_server_models(&server.uri(), Some("sk-corp"))
            .await
            .unwrap();

        // THEN
        let received: Vec<Request> = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 2);
        assert!(
            received[0].headers.get("authorization").is_none(),
            "no key, no Authorization header"
        );
        assert_eq!(
            received[1]
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok()),
            Some("Bearer sk-corp")
        );
    }

    // GIVEN a server that only answers when the expected key is presented
    // WHEN its models are listed with that key
    // THEN the list comes back
    #[tokio::test]
    async fn test_the_key_opens_a_protected_server() {
        // GIVEN
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer sk-corp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(model_list()))
            .mount(&server)
            .await;

        // WHEN
        let models = list_server_models(&server.uri(), Some("sk-corp")).await;

        // THEN
        assert_eq!(models.unwrap().len(), 2);
    }
}
