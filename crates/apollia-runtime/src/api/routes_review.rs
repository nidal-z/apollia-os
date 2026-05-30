//! REST route for on-demand code review, `POST /api/v1/tasks/{id}/review`.
//!
//! Submits a review task to the `apollia-review` agent, polls for completion,
//! and returns the parsed [`ReviewReport`] as JSON.  The endpoint is synchronous
//! from the client's perspective and times out after 120 seconds.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use tokio::time::{Duration, Instant};

use apollia_core::{AIPInput, AIPPart, DataPart, ReviewReport, TaskStatus};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Standard error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// Maximum wall-clock time the route waits for the review task to complete.
const REVIEW_TIMEOUT: Duration = Duration::from_secs(120);

/// Interval between status polls while waiting for the review agent.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Handler for `POST /api/v1/tasks/{id}/review`.
///
/// Starts an `apollia-review` task for the given task ID, waits synchronously
/// for completion (up to 120 s), then parses and returns the [`ReviewReport`].
pub async fn post_review<B: ExecutionBackend + Clone>(
    Path(task_id): Path<String>,
    State(state): State<AppState<B>>,
) -> Result<Json<ReviewReport>, (StatusCode, Json<ErrorResponse>)> {
    // Build the review input: pass the task_id so the agent knows what to review.
    let review_input = AIPInput {
        parts: vec![AIPPart::Data(DataPart {
            data: serde_json::json!({ "task_id": task_id }),
        })],
    };

    // Submit a task to the apollia-review agent.
    let review_task_id = state
        .router_handle
        .submit("apollia-review", review_input)
        .await
        .map_err(|e| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: format!("review agent unavailable: {e}"),
                }),
            )
        })?;

    // Poll until completion or timeout.
    let deadline = Instant::now() + REVIEW_TIMEOUT;

    loop {
        let status = state
            .router_handle
            .get_status(review_task_id.as_str())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("router error while polling review: {e}"),
                    }),
                )
            })?;

        match status {
            Some(TaskStatus::Completed) => break,
            Some(TaskStatus::Failed) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "review agent task failed".into(),
                    }),
                ));
            }
            Some(TaskStatus::Canceled) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "review agent task was cancelled".into(),
                    }),
                ));
            }
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err((
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!("review timed out after {}s", REVIEW_TIMEOUT.as_secs()),
                }),
            ));
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // Retrieve the task output text.
    let raw_output = state
        .router_handle
        .get_output(review_task_id.as_str())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to retrieve review output: {e}"),
                }),
            )
        })?
        .unwrap_or_default();

    // Parse the JSON output as ReviewReport.
    // On parse failure, return a degraded report (score = 0) rather than an error.
    let report: ReviewReport = serde_json::from_str(&raw_output).unwrap_or(ReviewReport {
        score: 0,
        issues: vec![],
        summary: "Erreur parsing: invalid review agent output.".into(),
    });

    Ok(Json(report))
}
