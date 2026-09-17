//! The typed half of a task pause, as the HTTP API sees it: checking an answer
//! at resume, and describing a paused task in the listing.
//!
//! Split out of `routes_tasks.rs`: the routes stay in the parent, the rules
//! that bind an answer to the pause it answers live here.

use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use apollia_core::{HitlPayload, InputResponseData};
use apollia_tools::PausedTaskRow;

/// Error body of `POST /api/v1/tasks/{id}/resume`.
///
/// The historical `{error}` shape, plus a machine `code` on the refusals a
/// caller branches on. `code` is absent on the older errors, so a client that
/// read `error` alone keeps working.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ResumeErrorBody {
    /// Human-readable error description.
    pub error: String,
    /// Stable code, e.g. `INVALID_ANSWER`, when the refusal is one a caller
    /// is expected to handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// A resume error with no code, for the refusals that predate typed pauses.
pub(super) fn plain(status: StatusCode, error: String) -> (StatusCode, Json<ResumeErrorBody>) {
    (status, Json(ResumeErrorBody { error, code: None }))
}

/// Check a resume request against the pause it answers and build the response
/// handed to the engine.
///
/// The stored context is given back rather than an empty object, and the
/// payload being answered travels with the response, so the resumed agent knows
/// which of its pauses it is resuming from.
///
/// # Errors
///
/// `422 INVALID_ANSWER` when the answer does not fit the pause: an id naming no
/// proposition, free text where none is allowed, a value of the wrong type, an
/// answer to an approval, a missing answer to an approved question, or any
/// answer to a pause that carries no payload.
pub(super) fn check_resume(
    pending: Option<&PausedTaskRow>,
    approved: bool,
    reason: Option<String>,
    answer: Option<serde_json::Value>,
) -> Result<InputResponseData, (StatusCode, Json<ResumeErrorBody>)> {
    let refuse = |message: String| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ResumeErrorBody {
                error: message,
                code: Some("INVALID_ANSWER".to_string()),
            }),
        )
    };

    let answer = answer.filter(|a| !a.is_null());
    let payload_value = pending.and_then(|p| p.payload.clone());

    match payload_value.as_ref() {
        Some(raw) => {
            // Stored only after it parsed at the pause, so a failure here means
            // the row was altered outside the runtime: refuse rather than guess.
            let payload = HitlPayload::parse(raw)
                .map_err(|e| refuse(format!("the pending payload no longer parses: {e}")))?;
            payload
                .check_answer(approved, answer.as_ref())
                .map_err(|e| refuse(e.to_string()))?;
        }
        None => {
            if answer.is_some() {
                return Err(refuse(apollia_core::AnswerError::NoPayload.to_string()));
            }
        }
    }

    Ok(InputResponseData {
        approved,
        reason,
        context: pending
            .map(|p| p.context.clone())
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())),
        responded_at: chrono::Utc::now().to_rfc3339(),
        answer,
        payload: payload_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn paused(payload: Option<serde_json::Value>) -> PausedTaskRow {
        PausedTaskRow {
            agent_name: "agent".into(),
            skill_id: None,
            created_at: "2026-09-16T10:00:00Z".into(),
            prompt: "?".into(),
            payload,
            context: json!({"kept": 1}),
        }
    }

    fn choix() -> serde_json::Value {
        json!({"genre": "choix", "question": "Which?", "propositions": [{"id": "a", "libelle": "A"}]})
    }

    #[test]
    fn a_matching_answer_is_accepted_with_the_stored_context_and_payload() {
        // GIVEN a task paused on a choice
        let row = paused(Some(choix()));
        // WHEN it is resumed with a proposition id
        let response = check_resume(Some(&row), true, None, Some(json!("a"))).expect("accepted");
        // THEN the response carries the answer, the payload it answers and the
        // agent's own context rather than an empty object
        assert_eq!(response.answer, Some(json!("a")));
        assert_eq!(response.payload, Some(choix()));
        assert_eq!(response.context, json!({"kept": 1}));
    }

    #[test]
    fn an_answer_naming_no_proposition_is_a_typed_422() {
        // GIVEN a task paused on a choice without free text
        let row = paused(Some(choix()));
        // WHEN it is resumed with an id that is not proposed
        let (status, Json(body)) =
            check_resume(Some(&row), true, None, Some(json!("z"))).expect_err("refused");
        // THEN the refusal is a 422 carrying the code a caller branches on
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body.code.as_deref(), Some("INVALID_ANSWER"));
        assert!(body.error.contains("`z`"), "error: {}", body.error);
    }

    #[test]
    fn an_answer_to_a_prompt_only_pause_is_refused() {
        // GIVEN a historical pause with no payload
        let row = paused(None);
        // WHEN an answer is sent anyway
        let refused = check_resume(Some(&row), true, None, Some(json!("a")));
        // THEN it is refused, since nothing says what it answers
        assert!(refused.is_err());
        // AND the historical resume, without an answer, still works
        assert!(check_resume(Some(&row), true, None, None).is_ok());
    }

    #[test]
    fn an_explicit_null_answer_counts_as_no_answer() {
        // GIVEN an approval pause
        let row = paused(Some(
            json!({"genre": "approbation", "geste": "x", "risque": "low"}),
        ));
        // WHEN a client sends `"answer": null`
        // THEN it is read as absent, which an approval accepts
        assert!(check_resume(Some(&row), false, None, Some(serde_json::Value::Null)).is_ok());
    }
}
