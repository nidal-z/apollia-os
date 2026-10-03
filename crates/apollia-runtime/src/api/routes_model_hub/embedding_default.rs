//! `GET` and `PUT /api/v1/llm/embeddings/default`: the embeddings model search
//! by meaning uses.
//!
//! The setting names an entry of the embeddings category of the registry, as
//! `GET /api/v1/llm/embeddings` lists them. Unset, search goes on with
//! keywords alone.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_llm::recommend::EmbeddingCatalogue;

use super::ErrorResponse;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::embeddings::{default_model, installed_path, set_default_model, SetDefaultError};

/// The default embeddings model, and whether this machine has its file.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingDefaultResponse {
    /// Identifier of the registry entry; absent when none is chosen, and
    /// search then uses keywords alone.
    pub model: Option<String>,
    /// Whether the file of that entry is in the model index.
    pub installed: bool,
    /// Path of that file, when installed.
    pub path: Option<String>,
}

/// Body of `PUT /api/v1/llm/embeddings/default`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetEmbeddingDefaultBody {
    /// Identifier of an embeddings entry of the registry.
    pub model: String,
}

type RouteError = (StatusCode, Json<ErrorResponse>);

fn error(status: StatusCode, message: impl ToString) -> RouteError {
    (
        status,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// The answer for the data directory `data_dir`, read off the async workers.
async fn current(data_dir: std::path::PathBuf) -> Result<EmbeddingDefaultResponse, RouteError> {
    tokio::task::spawn_blocking(move || {
        let model =
            default_model(&data_dir).map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        let path = model.as_deref().and_then(|id| {
            let catalogue = EmbeddingCatalogue::embedded().ok()?;
            let entry = catalogue.get(id)?;
            installed_path(&crate::installed_models::models_dir(&data_dir), entry)
        });
        Ok(EmbeddingDefaultResponse {
            model,
            installed: path.is_some(),
            path: path.map(|p| p.display().to_string()),
        })
    })
    .await
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
}

/// `GET /api/v1/llm/embeddings/default`
#[utoipa::path(
    get,
    path = "/api/v1/llm/embeddings/default",
    tag = "model_hub",
    responses(
        (status = 200, description = "The default embeddings model, absent when none is chosen", body = EmbeddingDefaultResponse),
        (status = 500, description = "system.db could not be read", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_embedding_default<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<EmbeddingDefaultResponse>, RouteError> {
    current(state.data_dir.clone()).await.map(Json)
}

/// `PUT /api/v1/llm/embeddings/default`
#[utoipa::path(
    put,
    path = "/api/v1/llm/embeddings/default",
    tag = "model_hub",
    request_body = SetEmbeddingDefaultBody,
    responses(
        (status = 200, description = "The default embeddings model, as now set", body = EmbeddingDefaultResponse),
        (status = 400, description = "Not an embeddings model of the registry", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "system.db could not be written", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn set_embedding_default<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<SetEmbeddingDefaultBody>,
) -> Result<Json<EmbeddingDefaultResponse>, RouteError> {
    let data_dir = state.data_dir.clone();
    let written = tokio::task::spawn_blocking(move || set_default_model(&data_dir, &body.model))
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    match written {
        Ok(()) => current(state.data_dir.clone()).await.map(Json),
        Err(e @ SetDefaultError::UnknownModel(_)) => Err(error(StatusCode::BAD_REQUEST, e)),
        Err(e) => Err(error(StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::StatusCode;

    use crate::api::test_support::{app_state, call, StemLoader};

    #[tokio::test]
    async fn test_the_default_is_absent_then_set_then_read_back() {
        // GIVEN a runtime on an empty data directory
        let tmp = tempfile::tempdir().expect("tempdir");
        let state = || {
            app_state(
                Arc::new(StemLoader { refusal: None }),
                tmp.path().to_path_buf(),
            )
        };

        // WHEN the default is read, set to a registry entry, and read again
        let (before_status, before) =
            call(state(), "GET", "/api/v1/llm/embeddings/default", None).await;
        let (put_status, put) = call(
            state(),
            "PUT",
            "/api/v1/llm/embeddings/default",
            Some(serde_json::json!({ "model": "multilingual-e5-small" })),
        )
        .await;
        let (_, after) = call(state(), "GET", "/api/v1/llm/embeddings/default", None).await;

        // THEN none is set at first, and the entry is read back, not installed
        assert_eq!(before_status, StatusCode::OK, "{before}");
        assert_eq!(before["model"], serde_json::Value::Null);
        assert_eq!(before["installed"], false);
        assert_eq!(put_status, StatusCode::OK, "{put}");
        assert_eq!(put["model"], "multilingual-e5-small");
        assert_eq!(after["model"], "multilingual-e5-small");
        assert_eq!(after["installed"], false);
    }

    #[tokio::test]
    async fn test_a_model_outside_the_registry_is_refused() {
        // GIVEN a runtime on an empty data directory
        let tmp = tempfile::tempdir().expect("tempdir");
        let state = app_state(
            Arc::new(StemLoader { refusal: None }),
            tmp.path().to_path_buf(),
        );

        // WHEN a chat model is set as the embeddings default
        let (status, body) = call(
            state,
            "PUT",
            "/api/v1/llm/embeddings/default",
            Some(serde_json::json!({ "model": "qwen3-8b" })),
        )
        .await;

        // THEN it is refused with 400 and the reason
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(body["error"]
            .as_str()
            .is_some_and(|e| e.contains("not an embeddings model")));
    }
}
