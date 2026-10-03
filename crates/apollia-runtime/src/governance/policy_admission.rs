//! The connection admission read from the organisation policy.
//!
//! The runtime reads the policy once at boot; [`PolicyAdmission`] holds that
//! verdict and answers the admission call with it. Without a policy every
//! connection is admitted, the user is free. A valid policy refuses, in this
//! order, through its `[connections]` section:
//!
//! - `remote = "deny"`: every connection that sends data off this machine, a
//!   native Google or Microsoft account among them;
//! - `deny`: every connection one of whose identifiers it lists;
//! - `allow`, when not empty: every connection none of whose identifiers it
//!   lists, a connection with no identifier among them.
//!
//! The identifiers are those of [`AdmissionRequest::identifiers`]. A policy
//! that is present but refused cannot be read, so the admission fails closed.
//!
//! This is the only place that forbids the cloud for a connection: the check
//! runs in the runtime, on what the runtime read, never on a value a caller
//! hands it.

use std::sync::Arc;

use apollia_permissions::org_policy::{ConnectionsPolicy, DenyOnly};
use apollia_permissions::PolicyVerdict;

use crate::governance::admission::{
    AdmissionDecision, AdmissionRequest, ConnectionAdmission, PolicyReadError,
};

/// What every refusal of the policy starts with, so that the interface can
/// say "refused by the policy" in the user's language.
pub const POLICY_REFUSAL_PREFIX: &str = "refused by the organisation policy";

/// Reason of a refusal under `connections.remote = "deny"`.
pub const REMOTE_DENIED_REASON: &str =
    "refused by the organisation policy: connections to online services are not allowed";

/// Reason of a refusal under `connections.deny`.
pub const CONNECTION_DENIED_REASON: &str =
    "refused by the organisation policy: this connection is refused";

/// Reason of a refusal under a non-empty `connections.allow`.
pub const CONNECTION_NOT_ALLOWED_REASON: &str =
    "refused by the organisation policy: this connection is not among the allowed ones";

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
            PolicyVerdict::Valid(valid) => Ok(decide(&valid.policy.connections, request)),
        }
    }
}

/// The decision of the `[connections]` section `policy` on `request`.
fn decide(policy: &ConnectionsPolicy, request: &AdmissionRequest) -> AdmissionDecision {
    let named_in = |list: &[String]| request.identifiers.iter().any(|id| list.contains(id));
    let reason = if request.remote && policy.remote == Some(DenyOnly::Deny) {
        REMOTE_DENIED_REASON
    } else if named_in(&policy.deny) {
        CONNECTION_DENIED_REASON
    } else if !policy.allow.is_empty() && !named_in(&policy.allow) {
        CONNECTION_NOT_ALLOWED_REASON
    } else {
        return AdmissionDecision::Admitted;
    };
    AdmissionDecision::Refused {
        reason: reason.to_owned(),
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

    fn mcp_server() -> AdmissionRequest {
        let mut config: apollia_mcp::config::McpServerConfig =
            serde_json::from_value(serde_json::json!({
                "name": "memory",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-memory"],
            }))
            .expect("config");
        config.origin = Some(apollia_mcp::ConnectionOrigin {
            provenance: apollia_mcp::Provenance::Selection,
            official: false,
            entry: Some("memory-entry".into()),
        });
        AdmissionRequest::for_mcp_server(&config)
    }

    fn folder() -> AdmissionRequest {
        AdmissionRequest::for_folder("/Users/ada/Documents", vec![])
    }

    /// One request of each kind, with the identifier a list names it by.
    fn one_of_each_kind() -> [(AdmissionRequest, &'static str); 3] {
        [
            (
                mcp_server(),
                "package:npm:@modelcontextprotocol/server-memory",
            ),
            (google(), "native:google"),
            (folder(), "native:folder"),
        ]
    }

    fn refused(reason: &str) -> AdmissionDecision {
        AdmissionDecision::Refused {
            reason: reason.to_owned(),
        }
    }

    // `connections.deny` refuses a connection it names, of every kind.
    #[test]
    fn test_deny_refuses_a_named_connection_of_every_kind() {
        for (request, identifier) in one_of_each_kind() {
            // GIVEN a policy denying the connection's identifier
            let admission = PolicyAdmission::new(verdict_with_connections(&format!(
                "deny = [\"{identifier}\"]"
            )));
            // WHEN the connection is submitted
            let decision = admit(&admission, &request);
            // THEN it is refused by the policy
            assert_eq!(decision, refused(CONNECTION_DENIED_REASON), "{identifier}");
        }
    }

    // `connections.deny` leaves alone a connection it does not name.
    #[test]
    fn test_deny_admits_a_connection_it_does_not_name() {
        for (request, identifier) in one_of_each_kind() {
            // GIVEN a policy denying another identifier
            let admission =
                PolicyAdmission::new(verdict_with_connections("deny = [\"native:microsoft\"]"));
            // WHEN the connection is submitted
            let decision = admit(&admission, &request);
            // THEN it is admitted
            assert_eq!(decision, AdmissionDecision::Admitted, "{identifier}");
        }
    }

    // A non-empty `connections.allow` admits a connection it names, of every kind.
    #[test]
    fn test_allow_admits_a_named_connection_of_every_kind() {
        for (request, identifier) in one_of_each_kind() {
            // GIVEN a policy allowing the connection's identifier and another one
            let admission = PolicyAdmission::new(verdict_with_connections(&format!(
                "allow = [\"native:imap\", \"{identifier}\"]"
            )));
            // WHEN the connection is submitted
            let decision = admit(&admission, &request);
            // THEN it is admitted
            assert_eq!(decision, AdmissionDecision::Admitted, "{identifier}");
        }
    }

    // A non-empty `connections.allow` refuses everything else, of every kind.
    #[test]
    fn test_allow_refuses_every_other_connection_of_every_kind() {
        for (request, identifier) in one_of_each_kind() {
            // GIVEN a policy allowing only the mailboxes
            let admission =
                PolicyAdmission::new(verdict_with_connections("allow = [\"native:imap\"]"));
            // WHEN the connection is submitted
            let decision = admit(&admission, &request);
            // THEN it is refused by the policy
            assert_eq!(
                decision,
                refused(CONNECTION_NOT_ALLOWED_REASON),
                "{identifier}"
            );
        }
    }

    // A selection server is allowed by its entry as well as by its package.
    #[test]
    fn test_allow_admits_a_selection_server_by_its_entry() {
        // GIVEN a policy allowing one selection entry
        let admission = PolicyAdmission::new(verdict_with_connections(
            "allow = [\"selection:memory-entry\"]",
        ));
        // WHEN the server made from it is submitted
        let decision = admit(&admission, &mcp_server());
        // THEN it is admitted
        assert_eq!(decision, AdmissionDecision::Admitted);
    }

    // `connections.remote = "deny"` refuses a remote server of every kind it
    // applies to, and leaves the local ones alone.
    #[test]
    fn test_remote_deny_by_kind() {
        // GIVEN a policy refusing remote connections
        let admission = PolicyAdmission::new(verdict_with_connections("remote = \"deny\""));
        let remote_mcp: apollia_mcp::config::McpServerConfig =
            serde_json::from_value(serde_json::json!({
                "name": "notion",
                "transport": "streamable-http",
                "url": "https://mcp.notion.com/mcp",
            }))
            .expect("config");

        // WHEN a remote MCP server, a native account, a local MCP server and a
        // folder are submitted
        let remote_mcp = admit(&admission, &AdmissionRequest::for_mcp_server(&remote_mcp));
        let native = admit(&admission, &google());
        let local_mcp = admit(&admission, &mcp_server());
        let folder = admit(&admission, &folder());

        // THEN the two remote ones are refused, the two local ones admitted
        assert_eq!(remote_mcp, refused(REMOTE_DENIED_REASON));
        assert_eq!(native, refused(REMOTE_DENIED_REASON));
        assert_eq!(local_mcp, AdmissionDecision::Admitted);
        assert_eq!(folder, AdmissionDecision::Admitted);
    }

    // Every refusal of the policy starts with the same words.
    #[test]
    fn test_every_refusal_starts_with_the_policy_prefix() {
        // GIVEN the three reasons of the policy
        for reason in [
            REMOTE_DENIED_REASON,
            CONNECTION_DENIED_REASON,
            CONNECTION_NOT_ALLOWED_REASON,
        ] {
            // WHEN its start is read
            // THEN it is the prefix the interface recognises
            assert!(reason.starts_with(POLICY_REFUSAL_PREFIX), "{reason}");
        }
    }
}
