//! `POST /api/v1/llm/embeddings/test`: try the embeddings model of this
//! machine on one sentence.
//!
//! The default embeddings model is loaded on its dedicated instance and asked
//! for the vector of a fixed sentence, exactly as a search would do. The
//! answer says which model answered, the size of its vector and how long it
//! took, so the person who just installed the model sees it work before
//! anything depends on it. Nothing is kept: no source is indexed, no memory
//! is written.

use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use apollia_llm::embeddings_client::{EmbedPurpose, EmbeddingError};

use super::ErrorResponse;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::embeddings::LocalEmbedder;

/// The sentence the test embeds. Its content does not matter: the model has
/// to be loaded and to answer.
const PROBE: &str = "Search by meaning finds a passage by what it says.";

/// What the embeddings model answered to the test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct EmbeddingsTestResponse {
    /// Identifier of the model that answered, an entry of the registry.
    pub model: String,
    /// Number of values of the vector it returned.
    pub dimensions: usize,
    /// Time from the request to the vector, loading included, in milliseconds.
    pub elapsed_ms: u64,
}

/// The status a failed test answers with: 409 when the machine is not ready
/// for it (no model chosen, its file absent, no local engine), 502 when the
/// model was there and failed.
pub(crate) fn status_of(error: &EmbeddingError) -> StatusCode {
    if error.falls_back_to_keywords() {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_GATEWAY
    }
}

/// `POST /api/v1/llm/embeddings/test`
#[utoipa::path(
    post,
    path = "/api/v1/llm/embeddings/test",
    tag = "model_hub",
    responses(
        (status = 200, description = "The model answered: its identifier, the size of the vector and the time taken", body = EmbeddingsTestResponse),
        (status = 409, description = "No embeddings model is chosen, its file is not installed, or no local engine can serve it", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "The model was loaded and failed to answer", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn test_embeddings_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<EmbeddingsTestResponse>, (StatusCode, Json<ErrorResponse>)> {
    let embedder = LocalEmbedder::new(
        state.data_dir.clone(),
        state.llama_server_supervisor.clone(),
    );
    let refused = |error: EmbeddingError| {
        (
            status_of(&error),
            Json(ErrorResponse {
                error: error.to_string(),
            }),
        )
    };
    let (entry, _) = embedder.resolve().await.map_err(refused)?;
    let started = Instant::now();
    let vectors = embedder
        .embed(&[PROBE.to_owned()], EmbedPurpose::Query)
        .await
        .map_err(refused)?;
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let dimensions = vectors.first().map_or(0, Vec::len);
    tracing::event!(
        tracing::Level::INFO,
        model = %entry.id,
        dimensions,
        elapsed_ms,
        "llm.embeddings.tested"
    );
    Ok(Json(EmbeddingsTestResponse {
        model: entry.id,
        dimensions,
        elapsed_ms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::{app_state, call};

    #[tokio::test]
    async fn test_without_a_chosen_model_the_test_says_what_is_missing() {
        // GIVEN a runtime on which no embeddings model was chosen
        let dir = tempfile::tempdir().unwrap();
        let state = app_state(dir.path().to_path_buf());

        // WHEN the test is asked for
        let (status, body) = call(state, "POST", "/api/v1/llm/embeddings/test", None).await;

        // THEN it answers that the machine is not ready, and names the reason
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("no embeddings model"),
            "{body}"
        );
    }

    #[test]
    fn test_a_machine_not_ready_and_a_model_that_failed_answer_differently() {
        // GIVEN a missing model, a missing engine and an engine that failed
        let missing = EmbeddingError::NoModel;
        let no_engine = EmbeddingError::EngineUnavailable;
        let failed = EmbeddingError::Transport("connection reset".into());

        // WHEN the status of each is read
        let statuses = [
            status_of(&missing),
            status_of(&no_engine),
            status_of(&failed),
        ];

        // THEN the first two say the machine is not ready, the third that the
        // model failed
        assert_eq!(
            statuses,
            [
                StatusCode::CONFLICT,
                StatusCode::CONFLICT,
                StatusCode::BAD_GATEWAY
            ]
        );
    }
}
