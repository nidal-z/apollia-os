//! The connection admission read from the organisation policy.
//!
//! The runtime reads the policy once at boot; [`PolicyAdmission`] holds that
//! verdict and answers the admission call with it. Without a policy every
//! connection is admitted, the user is free. A valid policy that carries
//! `connections.remote = "deny"` refuses every connection that sends data off
//! this machine, a native Google or Microsoft account among them. A policy that
//! is present but refused cannot be read, so the admission fails closed.
//!
//! This is the only place that forbids the cloud for a connection: the check
//! runs in the runtime, on what the runtime read, never on a value a caller
//! hands it.

use std::sync::Arc;

use apollia_permissions::org_policy::DenyOnly;
use apollia_permissions::PolicyVerdict;

use crate::governance::admission::{
    AdmissionDecision, AdmissionRequest, ConnectionAdmission, PolicyReadError,
};

/// Reason of a refusal under `connections.remote = "deny"`.
pub const REMOTE_DENIED_REASON: &str =
    "refused by the organisation policy: connections to online services are not allowed";

/// Admission over the organisation policy the runtime read at boot.
#[derive(Debug, Clone)]
pub struct PolicyAdmission {
    verdict: Arc<PolicyVerdict>,
}

impl PolicyAdmission {
    /// Admission over `verdict`.
    #[must_use]
    pub fn new(verdict: Arc<PolicyVerdict>) -> Self {
        Self { verdict }
    }
}

impl ConnectionAdmission for PolicyAdmission {
    fn read_decision(
        &self,
        request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        match self.verdict.as_ref() {
            PolicyVerdict::Absent => Ok(AdmissionDecision::Admitted),
            PolicyVerdict::Invalid(invalid) => Err(PolicyReadError::Unreadable(format!(
                "the organisation policy is refused ({}): {}",
                invalid.reason, invalid.detail
            ))),
            PolicyVerdict::Valid(valid) => {
                let remote_denied = valid.policy.connections.remote == Some(DenyOnly::Deny);
                if request.remote && remote_denied {
                    return Ok(AdmissionDecision::Refused {
                        reason: REMOTE_DENIED_REASON.to_owned(),
                    });
                }
                Ok(AdmissionDecision::Admitted)
            }
        }
    }
}

/// A valid policy whose `[connections]` section is `connections`, for tests.
#[cfg(test)]
pub(crate) fn valid_verdict_with_connections(connections: &str) -> Arc<PolicyVerdict> {
    let text = format!(
        "schema_version = 1\nsigned_at = 2026-10-01T09:00:00Z\n[connections]\n{connections}\n"
    );
    let policy = apollia_permissions::parse_policy(&text).expect("policy parses");
    Arc::new(PolicyVerdict::Valid(Box::new(
        apollia_permissions::ValidPolicy {
            signed_at: policy.signed_at,
            policy,
            key_fingerprint: "00".into(),
            publisher: "it".into(),
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::admission::admit;

    fn verdict_with_connections(connections: &str) -> Arc<PolicyVerdict> {
        valid_verdict_with_connections(connections)
    }

    fn google() -> AdmissionRequest {
        AdmissionRequest::for_native_account(apollia_auth::ConnectorProvider::Google)
    }

    // Without a policy the user is free: a native account is admitted.
    #[test]
    fn test_absent_policy_admits_a_native_account() {
        // GIVEN no deposited policy
        let admission = PolicyAdmission::new(Arc::new(PolicyVerdict::Absent));
        // WHEN a Google account is submitted
        let decision = admit(&admission, &google());
        // THEN it is admitted
        assert_eq!(decision, AdmissionDecision::Admitted);
    }

    // `connections.remote = "deny"` refuses a native account, which is remote.
    #[test]
    fn test_remote_deny_refuses_a_native_account() {
        // GIVEN a valid policy refusing remote connections
        let admission = PolicyAdmission::new(verdict_with_connections("remote = \"deny\""));
        // WHEN a Google account is submitted
        let decision = admit(&admission, &google());
        // THEN it is refused by the policy
        assert_eq!(
            decision,
            AdmissionDecision::Refused {
                reason: REMOTE_DENIED_REASON.into()
            }
        );
    }

    // `connections.remote = "deny"` leaves a local connection alone.
    #[test]
    fn test_remote_deny_admits_a_local_folder() {
        // GIVEN a valid policy refusing remote connections
        let admission = PolicyAdmission::new(verdict_with_connections("remote = \"deny\""));
        // WHEN a folder of this machine is submitted
        let decision = admit(
            &admission,
            &AdmissionRequest::for_folder("/Users/ada/Documents", vec![]),
        );
        // THEN it is admitted
        assert_eq!(decision, AdmissionDecision::Admitted);
    }

    // A valid policy that says nothing about remote connections admits them.
    #[test]
    fn test_policy_without_remote_rule_admits_a_native_account() {
        // GIVEN a valid policy with an empty connections section
        let admission = PolicyAdmission::new(verdict_with_connections(""));
        // WHEN a Google account is submitted
        let decision = admit(&admission, &google());
        // THEN it is admitted
        assert_eq!(decision, AdmissionDecision::Admitted);
    }

    // A present but refused policy cannot be read: the admission fails closed.
    #[test]
    fn test_invalid_policy_refuses() {
        // GIVEN a policy refused for its signature
        let admission = PolicyAdmission::new(Arc::new(PolicyVerdict::Invalid(
            apollia_permissions::PolicyInvalid {
                reason: apollia_permissions::PolicyInvalidReason::Signature,
                detail: "the signature does not hold".into(),
            },
        )));
        // WHEN a Google account is submitted
        let decision = admit(&admission, &google());
        // THEN it is refused, naming the reason the policy was refused
        assert!(
            matches!(&decision, AdmissionDecision::Refused { reason } if reason.contains("signature")),
            "{decision:?}"
        );
    }
}
