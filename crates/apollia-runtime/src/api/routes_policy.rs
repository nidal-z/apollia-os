//! `GET /api/v1/governance/policy`: the organisation policy the runtime
//! applies, as it read it at startup.
//!
//! The answer is the machine form `apollia-os policy show --json` prints
//! ([`apollia_permissions::PolicyView`]): the state (`none`, `active`,
//! `invalid`), the folder, and for an active policy the key fingerprint, the
//! signing date and every section; for an invalid one, the reason class and
//! its detail. The route reads the verdict held since boot rather than the
//! folder, because the boot verdict is the one every decision applies. It
//! only reads: nothing in this module writes a policy.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Builds the router of the policy route.
pub fn policy_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new().route("/api/v1/governance/policy", get(read_policy::<B>))
}

/// `GET /api/v1/governance/policy`: the policy read at startup.
#[utoipa::path(
    get,
    path = "/api/v1/governance/policy",
    tag = "governance",
    responses(
        (status = 200, description = "The organisation policy the runtime applies: `state` is `none`, `active` or `invalid`", body = Object),
    )
)]
pub async fn read_policy<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<apollia_permissions::PolicyView> {
    let location = apollia_permissions::policy_location();
    Json(apollia_permissions::PolicyView::of(
        state.inference_governance.verdict(),
        &location.dir,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use apollia_permissions::{PolicyInvalid, PolicyInvalidReason, PolicyVerdict};
    use axum::http::StatusCode;

    use tempfile::TempDir;

    use crate::api::server::AppState;
    use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
    use crate::governance::InferenceGovernance;

    /// A runtime that read `verdict` at boot, with the folder that holds its data.
    fn state_with(verdict: PolicyVerdict) -> (TempDir, AppState<MockBackend>) {
        let dir = tempfile::tempdir().unwrap();
        let mut state = app_state(
            Arc::new(StemLoader { refusal: None }),
            dir.path().to_path_buf(),
        );
        state.inference_governance = InferenceGovernance::new(Arc::new(verdict));
        (dir, state)
    }

    #[tokio::test]
    async fn test_no_policy_is_served_as_state_none() {
        // GIVEN a runtime that found no policy at boot
        let (_dir, state) = state_with(PolicyVerdict::Absent);

        // WHEN a client reads the policy
        let (status, body) = call(state, "GET", "/api/v1/governance/policy", None).await;

        // THEN the answer is the state none with the folder read
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["state"], "none");
        assert!(body["dir"].is_string());
    }

    #[tokio::test]
    async fn test_an_invalid_policy_is_served_with_its_reason() {
        // GIVEN a runtime whose policy was refused at boot for its signature
        let (_dir, state) = state_with(PolicyVerdict::Invalid(PolicyInvalid {
            reason: PolicyInvalidReason::Signature,
            detail: "the signature does not hold".into(),
        }));

        // WHEN a client reads the policy
        let (status, body) = call(state, "GET", "/api/v1/governance/policy", None).await;

        // THEN the reason class and its detail are served
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["state"], "invalid");
        assert_eq!(body["reason"], "signature");
        assert_eq!(body["detail"], "the signature does not hold");
    }

    #[tokio::test]
    async fn test_an_active_policy_is_served_in_the_form_policy_show_prints() {
        // GIVEN a runtime that read a valid policy refusing remote servers
        let policy = apollia_permissions::parse_policy(
            "schema_version = 1\nsigned_at = 2026-10-02T09:30:00Z\n[connections]\nremote = \"deny\"\n",
        )
        .unwrap();
        let verdict = PolicyVerdict::Valid(Box::new(apollia_permissions::ValidPolicy {
            signed_at: policy.signed_at,
            policy,
            key_fingerprint: "ab".repeat(32),
            publisher: "it".into(),
        }));
        let expected = serde_json::to_value(apollia_permissions::PolicyView::of(
            &verdict,
            &apollia_permissions::policy_location().dir,
        ))
        .unwrap();
        let (_dir, state) = state_with(verdict);

        // WHEN a client reads the policy
        let (status, body) = call(state, "GET", "/api/v1/governance/policy", None).await;

        // THEN the body is the shared machine form, key, date and sections included
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, expected);
        assert_eq!(body["signed_at"], "2026-10-02T09:30:00Z");
        assert_eq!(body["policy"]["connections"]["remote"], "deny");
    }

    #[tokio::test]
    async fn test_the_policy_route_refuses_a_write() {
        // GIVEN a runtime with a policy
        let (_dir, state) = state_with(PolicyVerdict::Absent);

        // WHEN a client tries to replace the policy
        let (status, _) = call(
            state,
            "PUT",
            "/api/v1/governance/policy",
            Some(serde_json::json!({"state": "none"})),
        )
        .await;

        // THEN the route has no write method
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }
}
