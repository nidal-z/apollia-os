//! The model calls of one conversation turn,
//! `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.
//!
//! A turn's answer carries its total; this route gives the detail behind it,
//! one row per call, read from the store the router's calls are written to.
//! The turn is named by the identifier of its answer message.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// One model call of a turn.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TurnLlmCall {
    /// Logical backend name.
    pub backend: String,
    /// Model identifier.
    pub model: String,
    /// Tokens read.
    pub prompt_tokens: Option<u32>,
    /// Tokens generated.
    pub completion_tokens: Option<u32>,
    /// Time to first token in milliseconds; `null` when not measured.
    pub ttft_ms: Option<u64>,
    /// Duration of the call in milliseconds, from dispatch to its last chunk.
    pub latency_ms: Option<u64>,
    /// Time spent generating in milliseconds; `null` when not measured.
    pub generation_ms: Option<u64>,
    /// Why the call ended: `stop`, `tool_calls`, `length`, `error` or
    /// `cancelled`.
    pub finish_reason: Option<String>,
}

/// Response body for `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TurnLlmCallsResponse {
    /// The calls, in the order they were made. Empty for a turn that made
    /// none or that is unknown.
    pub calls: Vec<TurnLlmCall>,
}

/// Handler for `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/turns/{turn_id}/llm-calls",
    tag = "llm",
    params(
        ("id" = String, Path, description = "Conversation identifier"),
        ("turn_id" = String, Path, description = "Identifier of the answer message of the turn"),
    ),
    responses(
        (status = 200, description = "The model calls of the turn, in order", body = TurnLlmCallsResponse),
        (status = 500, description = "Query failed", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "No LLM call repository configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_turn_llm_calls<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Json<TurnLlmCallsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let repo = state.llm_call_repository.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no LLM call repository configured"})),
        )
    })?;
    let repo = Arc::clone(repo);
    let records = tokio::task::spawn_blocking(move || {
        let guard = repo
            .lock()
            .map_err(|e| format!("failed to lock repository: {e}"))?;
        guard
            .query_by_turn(&session_id, &turn_id)
            .map_err(|e| format!("query failed: {e}"))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("join error: {e}")})),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
    })?;

    let calls = records
        .into_iter()
        .map(|r| TurnLlmCall {
            backend: r.backend,
            model: r.model,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            ttft_ms: r.ttft_ms,
            latency_ms: r.latency_ms,
            generation_ms: r.generation_ms,
            finish_reason: r.finish_reason,
        })
        .collect();
    Ok(Json(TurnLlmCallsResponse { calls }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use apollia_llm::{LlmCallRecord, LlmCallRepository};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::api::routes_llm::{llm_routes, test_app_state_no_llm, MockBackend};

    fn record(id: &str, turn: &str, ttft: Option<u64>) -> LlmCallRecord {
        LlmCallRecord {
            id: id.into(),
            task_id: None,
            step_id: None,
            backend: "local".into(),
            model: "qwen3-8b".into(),
            prompt_tokens: Some(812),
            completion_tokens: Some(64),
            cost_usd: None,
            latency_ms: Some(1_400),
            prompt_text: None,
            completion_text: None,
            session_id: Some("s-1".into()),
            turn_id: Some(turn.into()),
            ttft_ms: ttft,
            finish_reason: Some("stop".into()),
            generation_ms: Some(1_100),
        }
    }

    #[tokio::test]
    async fn test_the_route_returns_the_calls_of_one_turn_only() {
        // GIVEN a store holding one call of the turn and one of another turn
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = LlmCallRepository::open(&dir.path().join("llm_calls.db")).expect("open");
        repo.save(&record("c-1", "answer-1", None)).expect("save");
        repo.save(&record("c-2", "answer-2", Some(200)))
            .expect("save");
        let mut state = test_app_state_no_llm();
        state.llm_call_repository = Some(Arc::new(std::sync::Mutex::new(repo)));
        let app = llm_routes::<MockBackend>().with_state(state);

        // WHEN the calls of the first turn are read
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/sessions/s-1/turns/answer-1/llm-calls")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        // THEN one call comes back, with its model, tokens, duration and
        // stop reason, and its unmeasured TTFT as null rather than zero
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let calls = json["calls"].as_array().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["model"], "qwen3-8b");
        assert_eq!(calls[0]["prompt_tokens"], 812);
        assert_eq!(calls[0]["completion_tokens"], 64);
        assert_eq!(calls[0]["latency_ms"], 1_400);
        assert_eq!(calls[0]["finish_reason"], "stop");
        assert!(calls[0]["ttft_ms"].is_null());
    }

    #[tokio::test]
    async fn test_the_route_answers_503_without_a_store() {
        // GIVEN a runtime with no LLM call store
        let app = llm_routes::<MockBackend>().with_state(test_app_state_no_llm());

        // WHEN a turn's calls are read
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/sessions/s-1/turns/answer-1/llm-calls")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        // THEN the absence is said, not hidden behind an empty list
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
