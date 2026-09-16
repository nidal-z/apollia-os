//! The approval a task-path MCP call pauses on, and the decision to let it run.
//!
//! A server declared `requires_approval` used to be documented as gating every
//! call and gated none: the only gate lived behind a builder no production path
//! called, and it waited on a key no route could resolve. This module is the
//! gate as the task path runs it, through the same pause as a question an agent
//! asks itself.
//!
//! 1. A call with no matching approval answers
//!    [`GateDecision::Require`], an `approbation` payload naming the gesture
//!    `<server>/<tool>` and one detail line per argument. The task pauses on it.
//! 2. When the task resumes with that very gesture approved, the call runs,
//!    once. The approval is consumed, so a second identical call in the same
//!    resumed run pauses again rather than riding on the first decision.
//! 3. When the task resumes with that very gesture declined, the call is
//!    refused with [`GateDecision::Deny`], which reaches the agent as a typed
//!    error.
//!
//! "That very gesture" means the same gesture and the same arguments. An
//! approval of `crm/delete` for contact 42 does not let a resumed run delete
//! contact 43.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use apollia_core::InputResponseData;
use serde_json::{json, Value};

/// The approval state of one task, shared by every MCP executor built for it.
#[derive(Debug, Clone, Default)]
pub struct TaskApproval {
    response: Option<InputResponseData>,
    consumed: Arc<AtomicBool>,
}

impl TaskApproval {
    /// The approval state of a task, from the response it was resumed with.
    ///
    /// `None` on a first run: every gated call then pauses.
    pub fn new(response: Option<InputResponseData>) -> Self {
        Self {
            response,
            consumed: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// What the gate decides for one call.
#[derive(Debug, Clone, PartialEq)]
pub enum GateDecision {
    /// The call runs.
    Run,
    /// The task must pause on this approval first.
    Require {
        /// Sentence shown to the human.
        prompt: String,
        /// The `approbation` payload of the pause.
        payload: Value,
    },
    /// The operator declined this call.
    Deny {
        /// The operator's reason, when one was given.
        reason: Option<String>,
    },
}

/// One detail line per argument, in a stable order.
///
/// Object keys are sorted, so the same arguments always render the same lines
/// and the comparison against an approved payload does not depend on the order
/// an agent built its dict in.
pub fn detail_lines(input: &Value) -> Vec<String> {
    match input {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            keys.into_iter()
                .map(|k| format!("{k}: {}", map[k]))
                .collect()
        }
        Value::Null => Vec::new(),
        other => vec![other.to_string()],
    }
}

/// The `approbation` payload a gated call pauses on.
///
/// The risk is `medium`: a server declares that its calls need a human, not how
/// much each one is worth, and neither `low` nor `critical` would be a
/// measurement.
pub fn approval_payload(gesture: &str, input: &Value) -> Value {
    json!({
        "genre": "approbation",
        "geste": gesture,
        "risque": "medium",
        "detail": detail_lines(input),
    })
}

/// Decide whether a gated call runs, pauses, or is refused.
pub fn decide(gesture: &str, input: &Value, approval: &TaskApproval) -> GateDecision {
    let expected = approval_payload(gesture, input);

    if let Some(response) = approval.response.as_ref() {
        let answers_this_call = response.payload.as_ref().is_some_and(|p| {
            p.get("genre") == expected.get("genre")
                && p.get("geste") == expected.get("geste")
                && p.get("detail") == expected.get("detail")
        });
        if answers_this_call {
            if !response.approved {
                return GateDecision::Deny {
                    reason: response.reason.clone(),
                };
            }
            if !approval.consumed.swap(true, Ordering::SeqCst) {
                return GateDecision::Run;
            }
        }
    }

    GateDecision::Require {
        prompt: format!("Approve the call to {gesture}"),
        payload: expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resumed(approved: bool, payload: Value) -> TaskApproval {
        TaskApproval::new(Some(InputResponseData {
            approved,
            reason: Some("not now".into()),
            context: json!({}),
            responded_at: "2026-09-16T10:00:00Z".into(),
            answer: None,
            payload: Some(payload),
        }))
    }

    #[test]
    fn a_first_run_pauses_on_the_gesture_and_its_arguments() {
        // GIVEN a gated call on a first run
        let input = json!({"id": 42, "force": true});
        // WHEN the gate decides
        let decision = decide("crm/delete", &input, &TaskApproval::new(None));
        // THEN it pauses on an approval naming the gesture and one line per
        // argument, in a stable order
        match decision {
            GateDecision::Require { payload, .. } => {
                assert_eq!(payload["genre"], "approbation");
                assert_eq!(payload["geste"], "crm/delete");
                assert_eq!(payload["detail"], json!(["force: true", "id: 42"]));
                assert!(apollia_core::HitlPayload::parse(&payload).is_ok());
            }
            other => panic!("expected a pause, got {other:?}"),
        }
    }

    #[test]
    fn an_approved_gesture_runs_once() {
        // GIVEN a task resumed with this exact call approved
        let input = json!({"id": 42});
        let approval = resumed(true, approval_payload("crm/delete", &input));
        // WHEN the same call is made twice in the resumed run
        // THEN the first runs and the second pauses again
        assert_eq!(decide("crm/delete", &input, &approval), GateDecision::Run);
        assert!(matches!(
            decide("crm/delete", &input, &approval),
            GateDecision::Require { .. }
        ));
    }

    #[test]
    fn an_approval_does_not_cover_other_arguments() {
        // GIVEN a task resumed with the deletion of contact 42 approved
        let approval = resumed(true, approval_payload("crm/delete", &json!({"id": 42})));
        // WHEN the resumed run deletes contact 43 instead
        let decision = decide("crm/delete", &json!({"id": 43}), &approval);
        // THEN it pauses rather than riding on an approval of something else
        assert!(matches!(decision, GateDecision::Require { .. }));
    }

    #[test]
    fn a_declined_gesture_is_refused_with_the_reason() {
        // GIVEN a task resumed with this call declined
        let input = json!({"id": 42});
        let approval = resumed(false, approval_payload("crm/delete", &input));
        // WHEN the call is made again
        // THEN it is refused, carrying the operator's reason
        assert_eq!(
            decide("crm/delete", &input, &approval),
            GateDecision::Deny {
                reason: Some("not now".into())
            }
        );
    }

    #[test]
    fn key_order_does_not_change_the_detail() {
        // GIVEN the same arguments built in two orders
        let a: Value = serde_json::from_str(r#"{"b": 1, "a": 2}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a": 2, "b": 1}"#).unwrap();
        // WHEN both are rendered
        // THEN the lines are identical
        assert_eq!(detail_lines(&a), detail_lines(&b));
    }
}
