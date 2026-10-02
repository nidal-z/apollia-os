//! The admission call that precedes the addition of any connection.
//!
//! Before an MCP server, a native account or a folder is added, the place that
//! adds it describes the connection to [`admit`] and stops on a refusal. The
//! decision belongs to a deposited policy, read through [`ConnectionAdmission`];
//! [`NoPolicyAdmission`] is the implementation in force when none is deposited
//! and admits everything. A policy that cannot be read refuses: an admission
//! fails closed.

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::Provenance;
use apollia_tools::ActionFamily;
use serde::{Deserialize, Serialize};

use crate::connections::ConnectionKind;

/// What a policy is told about a connection about to be added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmissionRequest {
    /// Kind of connection.
    pub kind: ConnectionKind,
    /// Name of the connection: the server name, the provider, the folder path.
    pub name: String,
    /// Sends data off this machine (see [`crate::connections::is_remote`]).
    pub remote: bool,
    /// Where the connection was found before it was added.
    pub provenance: Provenance,
    /// Action families of the tools the connection brings, as far as they are
    /// known before it is added.
    pub families: Vec<ActionFamily>,
}

impl AdmissionRequest {
    /// The request describing the MCP server `config` declares: remote by its
    /// transport and address, provenance by its recorded origin (custom when
    /// it carries none), families by the classification it brings.
    #[must_use]
    pub fn for_mcp_server(config: &McpServerConfig) -> Self {
        let families = distinct(config.tool_families.values().copied());
        Self {
            kind: ConnectionKind::Mcp,
            name: config.name.clone(),
            remote: crate::connections::is_remote(&config.transport, config.url.as_deref()),
            provenance: config
                .origin
                .map_or(Provenance::Custom, |origin| origin.provenance),
            families,
        }
    }
}

impl AdmissionRequest {
    /// The request describing an account of the native connector `provider`:
    /// remote (the provider's service is online), Apollia's own code (the
    /// selection), and the families of the connector's tools.
    #[must_use]
    pub fn for_native_account(provider: apollia_auth::ConnectorProvider) -> Self {
        let descriptors = match provider {
            apollia_auth::ConnectorProvider::Google => {
                crate::connectors_bridge::google_tool_descriptors()
            }
            apollia_auth::ConnectorProvider::Microsoft => {
                crate::connectors_bridge::microsoft_tool_descriptors()
            }
        };
        Self {
            kind: ConnectionKind::Native,
            name: provider.id().to_owned(),
            remote: true,
            provenance: Provenance::Selection,
            families: distinct(descriptors.iter().map(|d| d.family)),
        }
    }

    /// The request describing the folder at `path`: on this machine, chosen by
    /// the user, its tools those of the native file tools.
    ///
    /// TEST-ONLY: the folder connection is not built yet; its addition, before
    /// anything is stored, is the production caller this is published for.
    #[must_use]
    pub fn for_folder(path: &str, families: Vec<ActionFamily>) -> Self {
        Self {
            kind: ConnectionKind::Folder,
            name: path.to_owned(),
            remote: false,
            provenance: Provenance::Custom,
            families: distinct(families),
        }
    }
}

/// `families` without repeats, in first-seen order.
fn distinct(families: impl IntoIterator<Item = ActionFamily>) -> Vec<ActionFamily> {
    let mut out: Vec<ActionFamily> = Vec::new();
    for family in families {
        if !out.contains(&family) {
            out.push(family);
        }
    }
    out
}

/// What a policy answers for one connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum AdmissionDecision {
    /// The connection may be added.
    Admitted,
    /// The connection may not be added.
    Refused {
        /// Why, shown to the user as is.
        reason: String,
    },
}

/// The deposited policy could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PolicyReadError {
    /// The policy exists but could not be read or understood.
    #[error("the connection policy could not be read: {0}")]
    Unreadable(String),
}

/// Reads the decision of the deposited policy for one connection.
///
/// Shared behind an `Arc<dyn ConnectionAdmission>`; the call is synchronous
/// and cheap, so it can sit in front of an HTTP route or a desktop command.
pub trait ConnectionAdmission: Send + Sync {
    /// The policy's decision for `request`, or why the policy could not be read.
    fn read_decision(
        &self,
        request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError>;
}

/// Admission in force when no policy is deposited: every connection is admitted.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPolicyAdmission;

impl ConnectionAdmission for NoPolicyAdmission {
    fn read_decision(
        &self,
        _request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        Ok(AdmissionDecision::Admitted)
    }
}

/// Decides whether `request` may be added, failing closed: a policy that cannot
/// be read refuses, with its error as the reason.
#[must_use]
pub fn admit(policy: &dyn ConnectionAdmission, request: &AdmissionRequest) -> AdmissionDecision {
    let decision = match policy.read_decision(request) {
        Ok(decision) => decision,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                connection = %request.name,
                error = %e,
                detail = "the connection is refused",
                "governance.admission.policy_unreadable"
            );
            AdmissionDecision::Refused {
                reason: e.to_string(),
            }
        }
    };
    if let AdmissionDecision::Refused { reason } = &decision {
        tracing::event!(
            tracing::Level::INFO,
            connection = %request.name,
            reason = %reason,
            "governance.admission.refused"
        );
    }
    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Unreadable;

    impl ConnectionAdmission for Unreadable {
        fn read_decision(
            &self,
            _request: &AdmissionRequest,
        ) -> Result<AdmissionDecision, PolicyReadError> {
            Err(PolicyReadError::Unreadable("policy.toml: invalid".into()))
        }
    }

    fn request(kind: ConnectionKind, remote: bool) -> AdmissionRequest {
        AdmissionRequest {
            kind,
            name: "notion".into(),
            remote,
            provenance: Provenance::Registry,
            families: vec![ActionFamily::SendExternal, ActionFamily::Unclassified],
        }
    }

    // Without a policy every kind of connection is admitted, remote or not.
    #[test]
    fn test_no_policy_admits_every_connection() {
        // GIVEN no deposited policy and one request of each kind
        let cases = [
            request(ConnectionKind::Mcp, true),
            request(ConnectionKind::Mcp, false),
            request(ConnectionKind::Native, true),
            request(ConnectionKind::Folder, false),
        ];
        for req in cases {
            // WHEN the admission is asked
            let decision = admit(&NoPolicyAdmission, &req);
            // THEN the connection is admitted
            assert_eq!(decision, AdmissionDecision::Admitted);
        }
    }

    // A policy that cannot be read refuses, naming why.
    #[test]
    fn test_unreadable_policy_refuses() {
        // GIVEN a policy whose read fails
        let req = request(ConnectionKind::Mcp, true);

        // WHEN the admission is asked
        let decision = admit(&Unreadable, &req);

        // THEN the connection is refused with the read error as its reason
        assert_eq!(
            decision,
            AdmissionDecision::Refused {
                reason: "the connection policy could not be read: policy.toml: invalid".into(),
            }
        );
    }

    // An MCP server is described by its transport, origin and classification.
    #[test]
    fn test_request_for_a_remote_mcp_server() {
        // GIVEN a remote server from the registry with two classified tools
        let mut config: McpServerConfig = serde_json::from_value(serde_json::json!({
            "name": "notion",
            "transport": "streamable-http",
            "url": "https://mcp.notion.com/mcp",
            "tool_families": { "search": "read", "create_page": "write_or_delete", "get": "read" },
        }))
        .expect("config");
        config.origin = Some(apollia_mcp::ConnectionOrigin {
            provenance: Provenance::Registry,
            official: true,
        });

        // WHEN the admission request is built
        let req = AdmissionRequest::for_mcp_server(&config);

        // THEN it names the server, remote, from the registry, each family once
        assert_eq!(req.kind, ConnectionKind::Mcp);
        assert_eq!(req.name, "notion");
        assert!(req.remote);
        assert_eq!(req.provenance, Provenance::Registry);
        assert_eq!(req.families.len(), 2);
        assert!(req.families.contains(&ActionFamily::Read));
        assert!(req.families.contains(&ActionFamily::WriteOrDelete));
    }

    // A native account is remote, from the selection, with its connector's families.
    #[test]
    fn test_request_for_a_native_account() {
        // GIVEN the Google connector
        let provider = apollia_auth::ConnectorProvider::Google;
        // WHEN the admission request is built
        let req = AdmissionRequest::for_native_account(provider);
        // THEN it is a remote native connection carrying the send family
        assert_eq!(req.kind, ConnectionKind::Native);
        assert_eq!(req.name, "google");
        assert!(req.remote);
        assert_eq!(req.provenance, Provenance::Selection);
        assert!(req.families.contains(&ActionFamily::SendExternal));
    }

    // A folder is local, chosen by the user, each family once.
    #[test]
    fn test_request_for_a_folder() {
        // GIVEN a folder whose file tools read and write
        let families = vec![
            ActionFamily::Read,
            ActionFamily::WriteOrDelete,
            ActionFamily::Read,
        ];
        // WHEN the admission request is built
        let req = AdmissionRequest::for_folder("/Users/ada/Documents", families);
        // THEN it is a local folder connection with two families
        assert_eq!(req.kind, ConnectionKind::Folder);
        assert!(!req.remote);
        assert_eq!(req.provenance, Provenance::Custom);
        assert_eq!(
            req.families,
            vec![ActionFamily::Read, ActionFamily::WriteOrDelete]
        );
    }

    // The decision travels as a tagged object.
    #[test]
    fn test_decision_wire_shape() {
        // GIVEN a refusal
        let decision = AdmissionDecision::Refused {
            reason: "remote servers are not allowed".into(),
        };
        // WHEN it is serialized
        let json = serde_json::to_value(&decision).expect("serialize");
        // THEN the tag and the reason are flat fields
        assert_eq!(
            json,
            serde_json::json!({"decision": "refused", "reason": "remote servers are not allowed"})
        );
    }
}
