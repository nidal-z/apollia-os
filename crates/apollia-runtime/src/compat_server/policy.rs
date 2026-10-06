//! What the organisation policy fixes of the compatible server.
//!
//! A policy closes three things, each through the `[settings]` table that
//! fixes any named setting: the local network (`server.serve_on_lan`), a
//! server without a token (`server.require_auth`), the MCP servers a request
//! brings (`server.allow_request_mcp`). It only closes: a policy that would
//! open one of them is refused whole where it is read.
//!
//! The runtime reads the policy once, at startup, before the listener opens.
//! A server the user's file had left open against it is therefore opened
//! closed, and the journal says so, one entry per setting.

use std::path::Path;

use apollia_core::config::{resolve_setting, PolicySettings, ALLOW_REQUEST_MCP_KEY};
use serde::Serialize;

pub(super) const REQUIRE_AUTH_KEY: &str = "server.require_auth";
pub(super) const SERVE_ON_LAN_KEY: &str = "server.serve_on_lan";

/// The settings of the server a policy fixes.
const FIXABLE: [&str; 3] = [SERVE_ON_LAN_KEY, REQUIRE_AUTH_KEY, ALLOW_REQUEST_MCP_KEY];

/// Which settings of the compatible server the organisation policy imposes.
/// An imposed setting cannot be written: the value in force is the policy's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerPolicyLocks {
    /// `server.serve_on_lan`: the policy keeps the server on the machine.
    pub serve_on_lan: bool,
    /// `server.require_auth`: the policy requires a token on every request.
    pub require_auth: bool,
    /// `server.allow_request_mcp`: the policy refuses the MCP servers a
    /// request brings.
    pub allow_request_mcp: bool,
}

impl ServerPolicyLocks {
    /// What `policy` imposes of the server.
    pub(super) fn of(policy: &PolicySettings) -> Self {
        Self {
            serve_on_lan: policy.locks(SERVE_ON_LAN_KEY),
            require_auth: policy.locks(REQUIRE_AUTH_KEY),
            allow_request_mcp: policy.locks(ALLOW_REQUEST_MCP_KEY),
        }
    }
}

/// The settings `policy` fixes at another value than the one the file at
/// `settings` gives them, its default included: what the policy closes on
/// this machine. A file that cannot be read contradicts nothing here; opening
/// the listener reports it.
pub(super) fn closed_against_the_file(
    settings: &Path,
    policy: &PolicySettings,
) -> Vec<&'static str> {
    let free = PolicySettings::none();
    FIXABLE
        .into_iter()
        .filter(|key| {
            let Some(imposed) = policy.imposed(key) else {
                return false;
            };
            resolve_setting(settings, key, &free).is_ok_and(|written| written.value != *imposed)
        })
        .collect()
}
