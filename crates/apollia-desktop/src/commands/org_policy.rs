//! Tauri IPC command for the organisation policy screen.
//!
//! The screen shows the policy the runtime applies, read only: the window
//! asks the runtime over `GET /api/v1/governance/policy`, in both modes, so an
//! attached window shows the daemon's policy rather than the one of its own
//! machine. The answer is the machine form `apollia-os policy show --json`
//! prints. No command here writes a policy: IT writes and signs it on the
//! command line, outside this application.

use serde_json::Value;
use tauri::State;

use crate::attached::RuntimeLink;

/// Route of the organisation policy on the runtime.
pub(crate) const POLICY_ROUTE: &str = "/api/v1/governance/policy";

/// Returns the organisation policy the runtime read at startup: its state
/// (`none`, `active`, `invalid`), and the key, the signing date and the rules
/// of an active one, or the reason of an invalid one.
#[tauri::command]
pub async fn get_org_policy(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(POLICY_ROUTE).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    #[tokio::test]
    async fn test_the_policy_is_read_from_the_daemon() {
        // GIVEN a daemon that applies an invalid policy
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(POLICY_ROUTE))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "state": "invalid", "dir": "/etc/apollia/policy",
                "reason": "signature", "detail": "the signature does not hold",
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the window reads the policy
        let policy = link.get_json(POLICY_ROUTE).await.unwrap();

        // THEN it is the daemon's, reason included
        assert_eq!(policy["state"], "invalid");
        assert_eq!(policy["reason"], "signature");
    }

    #[test]
    fn test_the_policy_command_is_routed_in_an_attached_window() {
        // GIVEN the command of the policy screen
        // WHEN it passes the gate of an attached window
        let gated = gate(LinkMode::Attached, "get_org_policy");

        // THEN it is not refused
        assert!(gated.is_ok());
    }
}
