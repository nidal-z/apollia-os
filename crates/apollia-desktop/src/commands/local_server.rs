//! Tauri IPC commands for the local server of the developer screen: the
//! listener other applications reach with the OpenAI and Anthropic formats.
//!
//! - [`get_local_server`]: `GET /api/v1/server`
//! - [`start_local_server`]: `POST /api/v1/server/start`
//! - [`stop_local_server`]: `POST /api/v1/server/stop`
//! - [`restart_local_server`]: `POST /api/v1/server/restart`
//!
//! The server lives in the runtime, never in this process: each command is a
//! call to a runtime route, so it serves the embedded runtime and the daemon
//! alike, and a server started here keeps answering once the window is closed
//! when the runtime is the daemon.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::attached::RuntimeLink;
use crate::commands::HttpFailure;

const SERVER_PATH: &str = "/api/v1/server";
const START_PATH: &str = "/api/v1/server/start";
const STOP_PATH: &str = "/api/v1/server/stop";
const RESTART_PATH: &str = "/api/v1/server/restart";

/// The status the runtime answers when another program holds the port.
const PORT_IN_USE_STATUS: u16 = 409;

/// The local server as the developer screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServerView {
    /// Whether the server listens.
    pub running: bool,
    /// The port it listens on, or the configured port when it is stopped.
    pub port: u16,
    /// Whether it is reachable from the local network.
    pub serve_on_lan: bool,
    /// The address a client copies.
    pub address: String,
    /// Why the last start failed, absent after a start that succeeded.
    #[serde(default)]
    pub last_error: Option<String>,
    /// How many tokens of the server are in use.
    #[serde(default)]
    pub active_tokens: u64,
    /// The settings of the server the organisation policy imposes.
    #[serde(default)]
    pub imposed_by_policy: PolicyLocks,
}

/// For each setting of the server a policy can fix, whether it does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyLocks {
    /// The policy keeps the server on the machine.
    #[serde(default)]
    pub serve_on_lan: bool,
    /// The policy requires a token on every request.
    #[serde(default)]
    pub require_auth: bool,
    /// The policy refuses the MCP servers a request brings.
    #[serde(default)]
    pub allow_request_mcp: bool,
}

/// Why the server did not start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServerRefusal {
    /// `port_in_use` when another program holds the port, `other` otherwise.
    pub kind: &'static str,
    /// The runtime's own words.
    pub message: String,
}

fn parse_view(json: serde_json::Value) -> Result<LocalServerView, String> {
    serde_json::from_value(json).map_err(|e| format!("invalid local server response: {e}"))
}

fn refusal(failure: HttpFailure) -> LocalServerRefusal {
    let kind = if failure.status == Some(PORT_IN_USE_STATUS) {
        "port_in_use"
    } else {
        "other"
    };
    LocalServerRefusal {
        kind,
        message: failure.message,
    }
}

/// Reads the state of the local server.
///
/// Delegates to `GET /api/v1/server`.
#[tauri::command]
pub async fn get_local_server(state: State<'_, RuntimeLink>) -> Result<LocalServerView, String> {
    parse_view(state.get_json(SERVER_PATH).await?)
}

/// Starts the local server on its configured port.
///
/// Delegates to `POST /api/v1/server/start`. Rejected with a
/// [`LocalServerRefusal`] that names a port held by another program apart
/// from every other failure.
#[tauri::command]
pub async fn start_local_server(
    state: State<'_, RuntimeLink>,
) -> Result<LocalServerView, LocalServerRefusal> {
    let json = state
        .post_json_refusable(START_PATH, &serde_json::json!({}))
        .await
        .map_err(refusal)?;
    parse_view(json).map_err(|message| LocalServerRefusal {
        kind: "other",
        message,
    })
}

/// Stops the local server. The runtime and its own API keep running.
///
/// Delegates to `POST /api/v1/server/stop`.
#[tauri::command]
pub async fn stop_local_server(state: State<'_, RuntimeLink>) -> Result<LocalServerView, String> {
    parse_view(state.post_json(STOP_PATH, &serde_json::json!({})).await?)
}

/// Closes and reopens the local server so it applies its settings as they
/// stand (port, local network, token check, CORS). A stopped server stays
/// stopped.
///
/// Delegates to `POST /api/v1/server/restart`. Rejected as
/// [`start_local_server`] is; a refused reopen leaves the server stopped.
#[tauri::command]
pub async fn restart_local_server(
    state: State<'_, RuntimeLink>,
) -> Result<LocalServerView, LocalServerRefusal> {
    let json = state
        .post_json_refusable(RESTART_PATH, &serde_json::json!({}))
        .await
        .map_err(refusal)?;
    parse_view(json).map_err(|message| LocalServerRefusal {
        kind: "other",
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_runtime_state_parses_into_the_view_the_screen_reads() {
        // GIVEN the body the runtime answers for a server that listens, with
        // fields this screen does not show
        let json = serde_json::json!({
            "running": true,
            "port": 7772,
            "serveOnLan": false,
            "address": "http://127.0.0.1:7772",
            "lastError": null,
            "activeTokens": 2,
            "imposedByPolicy": {
                "serveOnLan": false,
                "requireAuth": true,
                "allowRequestMcp": false
            }
        });

        // WHEN the command parses it
        let view = parse_view(json).unwrap();

        // THEN the state, the port and the address to copy are carried
        assert!(view.running);
        assert_eq!(view.port, 7772);
        assert_eq!(view.address, "http://127.0.0.1:7772");
        assert_eq!(view.last_error, None);
        // AND the webview reads them in camel case
        let sent = serde_json::to_value(&view).unwrap();
        assert_eq!(sent["serveOnLan"], false);
        assert_eq!(sent["lastError"], serde_json::Value::Null);
        // AND the count of tokens and what the policy imposes come with them
        assert_eq!(view.active_tokens, 2);
        assert!(view.imposed_by_policy.require_auth);
        assert_eq!(sent["activeTokens"], 2);
        assert_eq!(sent["imposedByPolicy"]["requireAuth"], true);
        assert_eq!(sent["imposedByPolicy"]["serveOnLan"], false);
    }

    #[test]
    fn test_a_state_from_a_runtime_without_tokens_or_policy_reads_as_none() {
        // GIVEN the body of a runtime that reports neither tokens nor policy
        let json = serde_json::json!({
            "running": false,
            "port": 7772,
            "serveOnLan": false,
            "address": "http://127.0.0.1:7772"
        });

        // WHEN the command parses it
        let view = parse_view(json).unwrap();

        // THEN no token is counted and nothing is imposed
        assert_eq!(view.active_tokens, 0);
        assert_eq!(view.imposed_by_policy, PolicyLocks::default());
    }

    #[test]
    fn test_a_state_without_its_address_is_an_error() {
        // GIVEN a body missing the address
        let json = serde_json::json!({ "running": false, "port": 7772, "serveOnLan": false });

        // WHEN the command parses it
        let result = parse_view(json);

        // THEN the command rejects with a readable message
        assert!(result
            .unwrap_err()
            .contains("invalid local server response"));
    }

    #[test]
    fn test_a_conflict_is_a_taken_port_and_any_other_failure_is_not() {
        // GIVEN the refusal of a start on a taken port, and two other failures
        let taken = HttpFailure {
            status: Some(409),
            message: "port 7772 is already in use by another program".to_owned(),
        };
        let broken = HttpFailure {
            status: Some(500),
            message: "failed to listen on port 7772".to_owned(),
        };
        let silent = HttpFailure {
            status: None,
            message: "request failed".to_owned(),
        };

        // WHEN each becomes the rejection the webview reads
        let taken = refusal(taken);
        let broken = refusal(broken);
        let silent = refusal(silent);

        // THEN only the conflict is named a taken port, with the runtime's words
        assert_eq!(taken.kind, "port_in_use");
        assert_eq!(
            taken.message,
            "port 7772 is already in use by another program"
        );
        assert_eq!(broken.kind, "other");
        assert_eq!(silent.kind, "other");
        // AND the webview reads the kind under its own key
        assert_eq!(serde_json::to_value(&taken).unwrap()["kind"], "port_in_use");
    }
}
