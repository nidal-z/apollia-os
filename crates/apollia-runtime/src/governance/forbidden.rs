//! The existing connections the organisation policy forbids, computed at boot.
//!
//! A connection added before the policy changed is never deleted. At every
//! boot each one is submitted to the same admission an addition goes through;
//! one the policy now refuses is forbidden for the life of the runtime: it is
//! not started, its tools are neither offered nor run, and the connection list
//! shows it as forbidden by the policy. Nothing is written: lift the rule and
//! the next boot finds the connection admitted, and it starts again.
//!
//! The mailbox tools over IMAP are shared by every mailbox and name theirs in
//! their input: they are not registered when every connected mailbox is
//! forbidden, and their executor refuses a forbidden mailbox at each call,
//! through [`mailbox_refusal`].
//!
//! A policy that cannot be read forbids nothing here: what an unreadable
//! policy shuts down is decided where the policy fails closed.

use std::collections::HashSet;

use apollia_connectors::imap::ImapAccount;
use apollia_mcp::config::McpServerConfig;

use crate::connections::{imap_connection_id, NativeAccounts};
use crate::governance::admission::{AdmissionDecision, AdmissionRequest, ConnectionAdmission};

/// Reason given when a connection forbidden at boot is asked to start again.
pub const FORBIDDEN_CONNECTION_REASON: &str =
    "refused by the organisation policy: this connection is forbidden by the policy";

/// The connections the policy forbade at boot, by connection identifier
/// (`mcp:<server>`, `native:<provider>:<account>`, `imap:<address>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForbiddenConnections {
    ids: HashSet<String>,
    /// Mailboxes are connected and every one of them is forbidden.
    no_mailbox_admitted: bool,
}

impl ForbiddenConnections {
    /// Submits every existing connection to `admission` and keeps the ones it
    /// refuses.
    #[must_use]
    pub fn compute(
        admission: &dyn ConnectionAdmission,
        mcp_configs: &[McpServerConfig],
        native_accounts: &[NativeAccounts],
        imap_accounts: &[ImapAccount],
    ) -> Self {
        let mut ids = HashSet::new();
        let mut check = |id: String, request: AdmissionRequest| {
            if let Ok(AdmissionDecision::Refused { reason }) = admission.read_decision(&request) {
                tracing::event!(
                    tracing::Level::WARN,
                    connection_id = %id,
                    reason = %reason,
                    detail = "not started, its tools neither offered nor run",
                    "governance.connection.forbidden"
                );
                ids.insert(id);
            }
        };
        for config in mcp_configs {
            check(
                mcp_connection_id(&config.name),
                AdmissionRequest::for_mcp_server(config),
            );
        }
        for (provider, accounts) in native_accounts {
            for account in accounts {
                check(
                    native_connection_id(provider.id(), account),
                    AdmissionRequest::for_native_account(*provider),
                );
            }
        }
        for account in imap_accounts {
            check(
                imap_connection_id(&account.email),
                AdmissionRequest::for_imap_account(account),
            );
        }
        let no_mailbox_admitted = !imap_accounts.is_empty()
            && imap_accounts
                .iter()
                .all(|a| ids.contains(&imap_connection_id(&a.email)));
        Self {
            ids,
            no_mailbox_admitted,
        }
    }

    /// Whether the mailbox tools are offered: no mailbox is connected yet, or
    /// at least one connected mailbox is not forbidden.
    #[must_use]
    pub fn offers_mailbox_tools(&self) -> bool {
        !self.no_mailbox_admitted
    }

    /// Whether the connection `connection_id` is forbidden.
    #[must_use]
    pub fn contains(&self, connection_id: &str) -> bool {
        self.ids.contains(connection_id)
    }

    /// Whether the MCP server `name` is forbidden.
    #[must_use]
    pub fn contains_mcp_server(&self, name: &str) -> bool {
        self.contains(&mcp_connection_id(name))
    }

    /// The forbidden connections, by connection identifier.
    #[must_use]
    pub fn ids(&self) -> &HashSet<String> {
        &self.ids
    }

    /// `native_accounts` without the forbidden accounts.
    #[must_use]
    pub fn allowed_accounts(&self, native_accounts: &[NativeAccounts]) -> Vec<NativeAccounts> {
        native_accounts
            .iter()
            .map(|(provider, accounts)| {
                let kept = accounts
                    .iter()
                    .filter(|a| !self.contains(&native_connection_id(provider.id(), a)))
                    .cloned()
                    .collect();
                (*provider, kept)
            })
            .collect()
    }

    /// The forbidden connections as the tool identifiers name them: an MCP
    /// server's tools are `mcp:<server>/<tool>`, a native account's
    /// `conn:<provider>:<account>/<operation>`. A mailbox's tools are shared by
    /// every mailbox, so none is named here: their executor refuses a
    /// forbidden mailbox, see [`mailbox_refusal`].
    #[must_use]
    pub fn tool_connections(&self) -> HashSet<String> {
        self.ids
            .iter()
            .filter_map(|id| {
                if id.starts_with("mcp:") {
                    Some(id.clone())
                } else {
                    id.strip_prefix("native:")
                        .map(|rest| format!("conn:{rest}"))
                }
            })
            .collect()
    }
}

/// Why `admission` refuses the mailbox `account`, `None` when it admits it.
///
/// The same admission a mailbox goes through at boot; a policy that cannot be
/// read refuses nothing here, as at boot.
#[must_use]
pub fn mailbox_refusal(
    admission: &dyn ConnectionAdmission,
    account: &ImapAccount,
) -> Option<&'static str> {
    match admission.read_decision(&AdmissionRequest::for_imap_account(account)) {
        Ok(AdmissionDecision::Refused { .. }) => Some(FORBIDDEN_CONNECTION_REASON),
        _ => None,
    }
}

fn mcp_connection_id(name: &str) -> String {
    format!("mcp:{name}")
}

fn native_connection_id(provider: &str, account: &str) -> String {
    format!("native:{provider}:{account}")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use apollia_auth::ConnectorProvider;

    use super::*;
    use crate::governance::policy_admission::valid_verdict_with_connections;
    use crate::governance::{NoPolicyAdmission, PolicyAdmission};

    fn npx(name: &str, package: &str) -> McpServerConfig {
        serde_json::from_value(serde_json::json!({
            "name": name, "command": "npx", "args": ["-y", package],
        }))
        .expect("config")
    }

    fn mailbox(email: &str, host: &str) -> ImapAccount {
        ImapAccount {
            email: email.to_owned(),
            host: host.to_owned(),
            port: 993,
            username: email.to_owned(),
        }
    }

    fn sources() -> (Vec<McpServerConfig>, Vec<NativeAccounts>, Vec<ImapAccount>) {
        (
            vec![npx("memory", "server-memory"), npx("fetch", "server-fetch")],
            vec![
                (ConnectorProvider::Google, vec!["ada@example.com".into()]),
                (ConnectorProvider::Microsoft, vec!["bob@example.com".into()]),
            ],
            vec![mailbox("eve@example.com", "imap.example.com")],
        )
    }

    // Without a policy nothing existing is forbidden.
    #[test]
    fn test_no_policy_forbids_nothing() {
        // GIVEN connections of every kind and no policy
        let (mcp, native, imap) = sources();
        // WHEN the forbidden connections are computed
        let forbidden = ForbiddenConnections::compute(&NoPolicyAdmission, &mcp, &native, &imap);
        // THEN there are none
        assert!(forbidden.ids().is_empty());
    }

    // A connection the policy now refuses is forbidden, the others are not.
    #[test]
    fn test_a_connection_the_policy_now_refuses_is_forbidden() {
        // GIVEN a policy denying one npm package and the Microsoft accounts
        let (mcp, native, imap) = sources();
        let admission = PolicyAdmission::new(valid_verdict_with_connections(
            "deny = [\"package:npm:server-fetch\", \"native:microsoft\"]",
        ));

        // WHEN the forbidden connections are computed
        let forbidden = ForbiddenConnections::compute(&admission, &mcp, &native, &imap);

        // THEN exactly the server and the account it names are forbidden
        let mut ids: Vec<&String> = forbidden.ids().iter().collect();
        ids.sort();
        assert_eq!(ids, ["mcp:fetch", "native:microsoft:bob@example.com"]);
        assert!(forbidden.contains_mcp_server("fetch"));
        assert!(!forbidden.contains_mcp_server("memory"));
    }

    // Under a non-empty allow list, a mailbox it does not name is forbidden.
    #[test]
    fn test_allow_forbids_a_mailbox_it_does_not_name() {
        // GIVEN a policy allowing only the Google accounts
        let (mcp, native, imap) = sources();
        let admission = PolicyAdmission::new(valid_verdict_with_connections(
            "allow = [\"native:google\"]",
        ));

        // WHEN the forbidden connections are computed
        let forbidden = ForbiddenConnections::compute(&admission, &mcp, &native, &imap);

        // THEN everything but the Google account is forbidden
        assert!(forbidden.contains("imap:eve@example.com"));
        assert!(forbidden.contains("mcp:memory"));
        assert!(!forbidden.contains("native:google:ada@example.com"));
    }

    // The forbidden accounts are left out of the accounts whose tools register,
    // and the tool identifiers name the forbidden connections.
    #[test]
    fn test_forbidden_accounts_and_tool_connections() {
        // GIVEN the Microsoft account and one server forbidden
        let (mcp, native, imap) = sources();
        let admission = PolicyAdmission::new(valid_verdict_with_connections(
            "deny = [\"native:microsoft\", \"package:npm:server-fetch\"]",
        ));
        let forbidden = ForbiddenConnections::compute(&admission, &mcp, &native, &imap);

        // WHEN the accounts to register and the tool connections are read
        let allowed = forbidden.allowed_accounts(&native);
        let tools = forbidden.tool_connections();

        // THEN the Microsoft account is gone, and the tools of both are named
        assert_eq!(allowed[0].1, vec!["ada@example.com".to_owned()]);
        assert!(allowed[1].1.is_empty());
        assert_eq!(
            tools,
            HashSet::from([
                "mcp:fetch".to_owned(),
                "conn:microsoft:bob@example.com".to_owned()
            ])
        );
    }

    // Lifting the rule: the next computation forbids nothing.
    #[test]
    fn test_a_lifted_rule_forbids_nothing_at_the_next_boot() {
        // GIVEN a boot under a policy denying a server, then one without the rule
        let (mcp, native, imap) = sources();
        let before = PolicyAdmission::new(valid_verdict_with_connections(
            "deny = [\"package:npm:server-fetch\"]",
        ));
        let after = PolicyAdmission::new(valid_verdict_with_connections(""));

        // WHEN each boot computes the forbidden connections
        let first = ForbiddenConnections::compute(&before, &mcp, &native, &imap);
        let second = ForbiddenConnections::compute(&after, &mcp, &native, &imap);

        // THEN the server is forbidden only under the rule
        assert!(first.contains_mcp_server("fetch"));
        assert!(second.ids().is_empty());
    }

    // The mailbox tools are offered unless every connected mailbox is forbidden.
    #[test]
    fn test_mailbox_tools_are_withdrawn_when_every_mailbox_is_forbidden() {
        // GIVEN a policy denying the mailboxes, a policy denying remote
        //   connections, and one mailbox on this machine beside a remote one
        let deny = PolicyAdmission::new(valid_verdict_with_connections("deny = [\"native:imap\"]"));
        let remote = PolicyAdmission::new(valid_verdict_with_connections("remote = \"deny\""));
        let both = [
            mailbox("eve@example.com", "imap.example.com"),
            mailbox("me@localhost", "127.0.0.1"),
        ];

        // WHEN the forbidden connections are computed
        let all_denied = ForbiddenConnections::compute(&deny, &[], &[], &both);
        let one_remote = ForbiddenConnections::compute(&remote, &[], &[], &both);
        let none_connected = ForbiddenConnections::compute(&deny, &[], &[], &[]);
        let unpoliced = ForbiddenConnections::compute(&NoPolicyAdmission, &[], &[], &both);

        // THEN the tools are withdrawn only when no mailbox is left
        assert!(!all_denied.offers_mailbox_tools());
        assert!(one_remote.contains("imap:eve@example.com"));
        assert!(!one_remote.contains("imap:me@localhost"));
        assert!(one_remote.offers_mailbox_tools());
        assert!(none_connected.offers_mailbox_tools());
        assert!(unpoliced.offers_mailbox_tools());
    }

    // A call on a mailbox goes through the admission the boot runs.
    #[test]
    fn test_mailbox_refusal_follows_the_admission() {
        // GIVEN a remote and a local mailbox, under a policy denying remote
        //   connections, and an unreadable policy
        let remote = PolicyAdmission::new(valid_verdict_with_connections("remote = \"deny\""));
        let unreadable = PolicyAdmission::new(Arc::new(
            apollia_permissions::PolicyVerdict::Invalid(apollia_permissions::PolicyInvalid {
                reason: apollia_permissions::PolicyInvalidReason::Signature,
                detail: "the signature does not hold".into(),
            }),
        ));
        let far = mailbox("eve@example.com", "imap.example.com");
        let near = mailbox("me@localhost", "127.0.0.1");

        // WHEN each mailbox is submitted
        // THEN only the remote one is refused, with the policy's reason, and
        //   an unreadable policy refuses nothing here, as at boot
        assert_eq!(
            mailbox_refusal(&remote, &far),
            Some(FORBIDDEN_CONNECTION_REASON)
        );
        assert_eq!(mailbox_refusal(&remote, &near), None);
        assert_eq!(mailbox_refusal(&unreadable, &far), None);
    }

    // A policy that cannot be read forbids nothing at boot.
    #[test]
    fn test_an_invalid_policy_forbids_nothing_here() {
        // GIVEN a present but refused policy
        let (mcp, native, imap) = sources();
        let admission = PolicyAdmission::new(Arc::new(
            apollia_permissions::PolicyVerdict::Invalid(apollia_permissions::PolicyInvalid {
                reason: apollia_permissions::PolicyInvalidReason::Signature,
                detail: "the signature does not hold".into(),
            }),
        ));
        // WHEN the forbidden connections are computed
        let forbidden = ForbiddenConnections::compute(&admission, &mcp, &native, &imap);
        // THEN there are none
        assert!(forbidden.ids().is_empty());
    }
}
