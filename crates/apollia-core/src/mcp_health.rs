//! Operational health of a single MCP server.
//!
//! Lives in `apollia-core` so that [`crate::RuntimeEvent`] and the MCP status
//! views can carry it, and so both the producer (`apollia-mcp`) and the
//! consumers (`apollia-runtime`, `apollia-desktop`) share one definition.
//!
//! Health is orthogonal to process liveness. A session can be `connected`
//! (process alive, handshake done) yet [`McpHealth::Degraded`] (e.g. a Notion
//! `object_not_found` on a real call) or [`McpHealth::NeedsReauth`] (an expired
//! token). The frontend dot colour is driven by [`McpHealth::severity`], never
//! by the `connected` flag alone.

use serde::{Deserialize, Serialize};

use crate::error_analysis::ErrorCategory;

/// Operational health of an MCP server, independent of process liveness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum McpHealth {
    /// Handshake passed and no operation is currently failing.
    ///
    /// `verified` is `false` until a real operation (tool call or Test probe)
    /// has succeeded. A freshly started session is `Healthy { verified: false }`:
    /// reachable, but its operational access (scopes, workspace grants) is not
    /// yet proven.
    Healthy {
        /// `true` once a real operation has succeeded since the session started.
        verified: bool,
    },
    /// Operations are failing without an auth/transport hard failure.
    ///
    /// This is the Notion `object_not_found` / wrong-workspace / missing-grant
    /// case: the handshake works but real calls fail.
    Degraded {
        /// Coarse category from the in-crate classifier.
        category: ErrorCategory,
        /// Last error message (already redacted, safe for builder mode).
        last_error: String,
        /// Consecutive failed operations since the last success.
        consecutive_failures: u32,
        /// ISO 8601 timestamp of when this degraded state began.
        since: String,
    },
    /// An auth signal was detected (401/403/invalid_grant). User must re-auth.
    NeedsReauth {
        /// Short machine reason, e.g. `"unauthorized"`, `"invalid_grant"`.
        reason: String,
    },
    /// The server cannot be reached (process exited, handshake never succeeded).
    Unavailable {
        /// Short machine reason, e.g. `"process_exited"`, `"handshake_failed"`.
        reason: String,
    },
}

/// UI severity bucket. Drives the status dot colour and the sidebar filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpHealthSeverity {
    /// Healthy: green.
    Ok,
    /// Degraded: amber, operations failing but reachable.
    Warn,
    /// Needs re-authorization: amber, distinct actionable state.
    Reauth,
    /// Unavailable: red.
    Error,
}

impl McpHealth {
    /// Map the health to its UI severity bucket.
    pub fn severity(&self) -> McpHealthSeverity {
        match self {
            Self::Healthy { .. } => McpHealthSeverity::Ok,
            Self::Degraded { .. } => McpHealthSeverity::Warn,
            Self::NeedsReauth { .. } => McpHealthSeverity::Reauth,
            Self::Unavailable { .. } => McpHealthSeverity::Error,
        }
    }

    /// `true` only when [`McpHealth::Healthy`], regardless of `verified`.
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Healthy { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_maps_each_state() {
        // GIVEN one value per health state
        // WHEN mapping to severity
        // THEN each maps to its bucket
        assert_eq!(
            McpHealth::Healthy { verified: true }.severity(),
            McpHealthSeverity::Ok
        );
        assert_eq!(
            McpHealth::Degraded {
                category: ErrorCategory::ToolFailure,
                last_error: "object_not_found".into(),
                consecutive_failures: 1,
                since: "2026-06-02T00:00:00Z".into(),
            }
            .severity(),
            McpHealthSeverity::Warn
        );
        assert_eq!(
            McpHealth::NeedsReauth {
                reason: "unauthorized".into()
            }
            .severity(),
            McpHealthSeverity::Reauth
        );
        assert_eq!(
            McpHealth::Unavailable {
                reason: "process_exited".into()
            }
            .severity(),
            McpHealthSeverity::Error
        );
    }

    #[test]
    fn serde_roundtrip_uses_state_discriminant() {
        // GIVEN a degraded health
        let health = McpHealth::Degraded {
            category: ErrorCategory::ToolFailure,
            last_error: "could not find page".into(),
            consecutive_failures: 3,
            since: "2026-06-02T10:00:00Z".into(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&health).expect("serialize");

        // THEN the discriminant is the snake_case `state` tag
        assert_eq!(json["state"], "degraded");
        assert_eq!(json["category"], "tool_failure");
        assert_eq!(json["consecutive_failures"], 3);

        // AND it roundtrips
        let back: McpHealth = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, health);
    }

    #[test]
    fn is_ok_only_for_healthy() {
        // GIVEN healthy regardless of verified
        // THEN is_ok holds, and not for the others
        assert!(McpHealth::Healthy { verified: false }.is_ok());
        assert!(McpHealth::Healthy { verified: true }.is_ok());
        assert!(!McpHealth::NeedsReauth {
            reason: "x".into()
        }
        .is_ok());
    }
}
