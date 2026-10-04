//! The model calls of one turn of a conversation.
//!
//! The answer of a turn carries its total; the chat shows the detail behind it
//! on demand, one row per model call. The rows are read from the route the
//! runtime serves, `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`, so
//! the list is the same whether the runtime runs in this process or as a
//! daemon. The turn is named by the identifier of its answer message.

use serde_json::Value;
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// The runtime route of the model calls of turn `turn_id` of `session_id`.
pub(crate) fn turn_calls_path(session_id: &str, turn_id: &str) -> String {
    format!(
        "/api/v1/sessions/{}/turns/{}/llm-calls",
        segment(session_id),
        segment(turn_id)
    )
}

/// The calls in the answer of the route, in the order they were made.
fn calls_of(answer: Value) -> Result<Vec<Value>, String> {
    match answer {
        Value::Object(mut body) => match body.remove("calls") {
            Some(Value::Array(calls)) => Ok(calls),
            _ => Err("the runtime answered no list of model calls".to_owned()),
        },
        _ => Err("the runtime answered no list of model calls".to_owned()),
    }
}

/// The model calls of a turn on the runtime `link` drives.
async fn turn_calls_on(
    link: &RuntimeLink,
    session_id: &str,
    turn_id: &str,
) -> Result<Vec<Value>, String> {
    calls_of(link.get_json(&turn_calls_path(session_id, turn_id)).await?)
}

/// The model calls of one turn, in order: model, tokens read and generated,
/// time to first token, duration, and why each call ended. Empty for a turn
/// that made no call. A value the engine did not measure is `null`.
#[tauri::command]
pub async fn list_turn_llm_calls(
    link: State<'_, RuntimeLink>,
    session_id: String,
    turn_id: String,
) -> Result<Vec<Value>, String> {
    turn_calls_on(link.inner(), &session_id, &turn_id).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    #[tokio::test]
    async fn test_the_calls_of_a_turn_are_read_from_the_daemon_in_order() {
        // GIVEN a daemon holding two calls for the turn `answer-1` of `s1`,
        // the second without a measured first token
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/s1/turns/answer-1/llm-calls"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "calls": [
                    {"backend": "local", "model": "qwen3-8b", "prompt_tokens": 3120,
                     "completion_tokens": 212, "ttft_ms": 480, "latency_ms": 5200,
                     "generation_ms": 4700, "finish_reason": "tool_calls"},
                    {"backend": "local", "model": "qwen3-8b", "prompt_tokens": 3940,
                     "completion_tokens": 200, "ttft_ms": null, "latency_ms": 5600,
                     "generation_ms": null, "finish_reason": "stop"},
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the chat reads the calls of the turn
        let calls = turn_calls_on(&attached(&server), "s1", "answer-1")
            .await
            .unwrap();

        // THEN both come back in order, the unknown first token still unknown
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["finish_reason"], "tool_calls");
        assert_eq!(calls[0]["ttft_ms"], 480);
        assert!(calls[1]["ttft_ms"].is_null());
    }

    #[tokio::test]
    async fn test_a_refusal_of_the_daemon_is_an_error_not_an_empty_list() {
        // GIVEN a daemon with no store of model calls
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/s1/turns/answer-1/llm-calls"))
            .respond_with(
                ResponseTemplate::new(503).set_body_json(
                    serde_json::json!({"error": "no LLM call repository configured"}),
                ),
            )
            .mount(&server)
            .await;

        // WHEN the chat reads the calls of the turn
        let read = turn_calls_on(&attached(&server), "s1", "answer-1").await;

        // THEN the refusal is reported
        assert!(read.is_err());
    }

    #[test]
    fn test_the_identifiers_are_encoded_as_path_segments() {
        // GIVEN identifiers that carry a slash and a space
        // WHEN the route is built
        let route = turn_calls_path("a/b", "turn 1");

        // THEN each stays one segment
        assert_eq!(route, "/api/v1/sessions/a%2Fb/turns/turn%201/llm-calls");
    }

    #[test]
    fn test_an_answer_without_calls_is_an_error() {
        // GIVEN an answer of another shape
        // WHEN its calls are read
        let read = calls_of(serde_json::json!({"rows": []}));

        // THEN it is refused rather than read as an empty turn
        assert!(read.is_err());
    }

    #[test]
    fn test_the_command_is_routed_to_a_daemon() {
        // GIVEN the command that lists the calls of a turn
        // WHEN it passes the gate of an attached window
        // THEN it is not refused
        assert!(gate(LinkMode::Attached, "list_turn_llm_calls").is_ok());
    }
}
