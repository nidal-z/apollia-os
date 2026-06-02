//! In-crate health classifier for MCP server operations.
//!
//! Turns a single operation outcome (a `tools/call` result or an
//! [`McpSessionError`]) into the next [`McpHealth`] for a server, given its
//! previous state. No LLM, no I/O: a substring table plus the session error
//! shape.
//!
//! This is intentionally separate from
//! `apollia-runtime::analyzers::classify_tool_error`, which produces the
//! richer LLM-facing `ErrorAnalysis` and depends on `apollia-llm`. Reusing it
//! here would invert the dependency graph (apollia-mcp must not depend on
//! apollia-runtime/apollia-llm). The two classifiers are small and serve
//! different consumers, so the few shared needles are duplicated on purpose.

use apollia_core::error_analysis::ErrorCategory;
use apollia_core::McpHealth;

use crate::session::McpSessionError;

/// Maximum length of a stored `last_error` string. Tool error bodies can be
/// large; we keep a readable head for builder mode without bloating events.
const MAX_LAST_ERROR_LEN: usize = 1000;

/// Substrings (lowercased) that signal an authorization failure.
const AUTH_NEEDLES: &[&str] = &[
    "401",
    "unauthorized",
    "403",
    "forbidden",
    "invalid_grant",
    "invalid token",
    "expired token",
    "token expired",
    "authentication",
    "permission denied",
];

/// Substrings (lowercased) that signal a missing/inaccessible resource. This is
/// the Notion `object_not_found` / wrong-workspace / missing-grant case.
const NOT_FOUND_NEEDLES: &[&str] = &[
    "object_not_found",
    "not_found",
    "not found",
    "could not find",
    "no access",
    "restricted",
    "wrong workspace",
];

/// Substrings (lowercased) that signal a transient timeout.
const TIMEOUT_NEEDLES: &[&str] = &["timeout", "timed out"];

/// Substrings (lowercased) that signal a transient network / rate condition.
const NETWORK_NEEDLES: &[&str] = &["rate limit", "rate-limit", "connection", "network", "503", "502"];

/// One operation outcome fed to [`next_health`].
pub(crate) enum OpOutcome<'a> {
    /// The call returned a non-error result.
    Success,
    /// A `tools/call` returned `is_error = true` with this joined text body.
    ToolError(&'a str),
    /// The session layer returned a hard error.
    SessionError(&'a McpSessionError),
}

/// Internal classification signal, independent of the previous state.
enum Signal {
    Success,
    Auth { reason: String },
    Degraded { category: ErrorCategory, last_error: String },
    Unavailable { reason: String },
    /// Outcome carries no health information (e.g. pending approval).
    Unchanged,
}

/// Compute the next health for a server given its previous health and one
/// operation outcome. `now_iso` stamps the `since` of a freshly degraded state.
pub(crate) fn next_health(prev: &McpHealth, outcome: OpOutcome<'_>, now_iso: &str) -> McpHealth {
    match classify(outcome) {
        Signal::Success => McpHealth::Healthy { verified: true },
        Signal::Auth { reason } => McpHealth::NeedsReauth { reason },
        Signal::Unavailable { reason } => McpHealth::Unavailable { reason },
        Signal::Unchanged => prev.clone(),
        Signal::Degraded {
            category,
            last_error,
        } => match prev {
            // Already degraded: keep the original `since`, bump the counter.
            McpHealth::Degraded {
                consecutive_failures,
                since,
                ..
            } => McpHealth::Degraded {
                category,
                last_error,
                consecutive_failures: consecutive_failures.saturating_add(1),
                since: since.clone(),
            },
            _ => McpHealth::Degraded {
                category,
                last_error,
                consecutive_failures: 1,
                since: now_iso.to_string(),
            },
        },
    }
}

/// Classify the health a server should adopt when its session fails to start.
///
/// An auth signal becomes [`McpHealth::NeedsReauth`] (actionable: re-authorize);
/// everything else is [`McpHealth::Unavailable`] (the handshake never landed).
pub(crate) fn from_start_error(err: &McpSessionError) -> McpHealth {
    match err {
        McpSessionError::Unauthorized { .. } => McpHealth::NeedsReauth {
            reason: "unauthorized".to_string(),
        },
        McpSessionError::StdinClosed { cause, .. } if is_auth_text(&cause.to_lowercase()) => {
            McpHealth::NeedsReauth {
                reason: "unauthorized".to_string(),
            }
        }
        _ => McpHealth::Unavailable {
            reason: "handshake_failed".to_string(),
        },
    }
}

fn classify(outcome: OpOutcome<'_>) -> Signal {
    match outcome {
        OpOutcome::Success => Signal::Success,
        OpOutcome::ToolError(text) => classify_text(text),
        OpOutcome::SessionError(e) => classify_session_error(e),
    }
}

fn classify_session_error(err: &McpSessionError) -> Signal {
    match err {
        McpSessionError::Unauthorized { .. } => Signal::Auth {
            reason: "unauthorized".to_string(),
        },
        McpSessionError::ServerExited { .. } => Signal::Unavailable {
            reason: "process_exited".to_string(),
        },
        McpSessionError::StdinClosed { cause, .. } => {
            if is_auth_text(&cause.to_lowercase()) {
                Signal::Auth {
                    reason: "unauthorized".to_string(),
                }
            } else {
                Signal::Unavailable {
                    reason: "transport_closed".to_string(),
                }
            }
        }
        McpSessionError::SpawnFailed { .. } => Signal::Unavailable {
            reason: "spawn_failed".to_string(),
        },
        McpSessionError::InitializeFailed { .. } | McpSessionError::InitializeTimeout { .. } => {
            Signal::Unavailable {
                reason: "handshake_failed".to_string(),
            }
        }
        McpSessionError::ToolCallTimeout { .. } => Signal::Degraded {
            category: ErrorCategory::Timeout,
            last_error: truncate(&err.to_string()),
        },
        McpSessionError::ToolCallFailed { cause, .. } => classify_text(cause),
        McpSessionError::JsonRpcError { message, .. } => classify_text(message),
        McpSessionError::SerdeError(_) => Signal::Degraded {
            category: ErrorCategory::MalformedOutput,
            last_error: truncate(&err.to_string()),
        },
        // No health signal: the operation never reached the server.
        McpSessionError::PendingApproval { .. }
        | McpSessionError::ServerReloading { .. }
        | McpSessionError::ConfigReload { .. } => Signal::Unchanged,
    }
}

fn classify_text(text: &str) -> Signal {
    let lower = text.to_lowercase();
    if is_auth_text(&lower) {
        return Signal::Auth {
            reason: auth_reason(&lower),
        };
    }
    let category = if NOT_FOUND_NEEDLES.iter().any(|n| lower.contains(n)) {
        ErrorCategory::ToolFailure
    } else if TIMEOUT_NEEDLES.iter().any(|n| lower.contains(n)) {
        ErrorCategory::Timeout
    } else if NETWORK_NEEDLES.iter().any(|n| lower.contains(n)) {
        ErrorCategory::NetworkError
    } else {
        ErrorCategory::Unknown
    };
    Signal::Degraded {
        category,
        last_error: truncate(text),
    }
}

fn is_auth_text(lower: &str) -> bool {
    AUTH_NEEDLES.iter().any(|n| lower.contains(n))
}

fn auth_reason(lower: &str) -> String {
    if lower.contains("invalid_grant") {
        "invalid_grant".to_string()
    } else if lower.contains("expired") {
        "expired_token".to_string()
    } else if lower.contains("403") || lower.contains("forbidden") {
        "forbidden".to_string()
    } else {
        "unauthorized".to_string()
    }
}

fn truncate(text: &str) -> String {
    if text.len() <= MAX_LAST_ERROR_LEN {
        return text.to_string();
    }
    let mut end = MAX_LAST_ERROR_LEN;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-06-02T12:00:00Z";

    #[test]
    fn auth_tool_error_maps_to_needs_reauth() {
        // GIVEN a healthy server
        let prev = McpHealth::Healthy { verified: true };
        // WHEN a tool call reports 401 unauthorized
        let next = next_health(&prev, OpOutcome::ToolError("HTTP 401 unauthorized"), NOW);
        // THEN it needs re-authorization
        assert!(matches!(next, McpHealth::NeedsReauth { .. }));
    }

    #[test]
    fn notion_object_not_found_maps_to_degraded_tool_failure() {
        // GIVEN a healthy server
        let prev = McpHealth::Healthy { verified: false };
        // WHEN a tool call returns object_not_found
        let next = next_health(
            &prev,
            OpOutcome::ToolError("{\"code\":\"object_not_found\"}"),
            NOW,
        );
        // THEN it is degraded with a single failure stamped now
        match next {
            McpHealth::Degraded {
                category,
                consecutive_failures,
                since,
                ..
            } => {
                assert_eq!(category, ErrorCategory::ToolFailure);
                assert_eq!(consecutive_failures, 1);
                assert_eq!(since, NOW);
            }
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    #[test]
    fn degraded_increments_then_success_resets() {
        // GIVEN a server already degraded twice at an earlier timestamp
        let prev = McpHealth::Degraded {
            category: ErrorCategory::ToolFailure,
            last_error: "not found".into(),
            consecutive_failures: 2,
            since: "2026-06-02T11:00:00Z".into(),
        };
        // WHEN another not-found error arrives
        let next = next_health(&prev, OpOutcome::ToolError("could not find page"), NOW);
        // THEN the counter increments and `since` is preserved
        match &next {
            McpHealth::Degraded {
                consecutive_failures,
                since,
                ..
            } => {
                assert_eq!(*consecutive_failures, 3);
                assert_eq!(since, "2026-06-02T11:00:00Z");
            }
            other => panic!("expected Degraded, got {other:?}"),
        }
        // WHEN a later call succeeds
        let healed = next_health(&next, OpOutcome::Success, NOW);
        // THEN health is verified-healthy again
        assert_eq!(healed, McpHealth::Healthy { verified: true });
    }

    #[test]
    fn session_unauthorized_overrides_to_needs_reauth() {
        // GIVEN a degraded server
        let prev = McpHealth::Degraded {
            category: ErrorCategory::ToolFailure,
            last_error: "x".into(),
            consecutive_failures: 1,
            since: NOW.into(),
        };
        let err = McpSessionError::Unauthorized {
            server: "notion".into(),
            www_authenticate: String::new(),
        };
        // WHEN a session-level Unauthorized arrives
        let next = next_health(&prev, OpOutcome::SessionError(&err), NOW);
        // THEN re-auth always wins over degraded
        assert!(matches!(next, McpHealth::NeedsReauth { .. }));
    }

    #[test]
    fn tool_timeout_is_degraded_timeout() {
        let prev = McpHealth::Healthy { verified: true };
        let err = McpSessionError::ToolCallTimeout {
            server: "notion".into(),
            tool: "search".into(),
            timeout_secs: 60,
        };
        let next = next_health(&prev, OpOutcome::SessionError(&err), NOW);
        match next {
            McpHealth::Degraded { category, .. } => assert_eq!(category, ErrorCategory::Timeout),
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    #[test]
    fn pending_approval_leaves_health_unchanged() {
        let prev = McpHealth::Healthy { verified: true };
        let err = McpSessionError::PendingApproval {
            server: "notion".into(),
            tool: "search".into(),
            approval_id: "id".into(),
        };
        let next = next_health(&prev, OpOutcome::SessionError(&err), NOW);
        assert_eq!(next, prev);
    }

    #[test]
    fn start_error_classification() {
        // Initialize timeout -> Unavailable
        let timeout = McpSessionError::InitializeTimeout {
            server: "notion".into(),
            timeout_secs: 30,
            stderr_hint: String::new(),
        };
        assert!(matches!(
            from_start_error(&timeout),
            McpHealth::Unavailable { .. }
        ));
        // Unauthorized at start -> NeedsReauth
        let unauth = McpSessionError::Unauthorized {
            server: "notion".into(),
            www_authenticate: String::new(),
        };
        assert!(matches!(
            from_start_error(&unauth),
            McpHealth::NeedsReauth { .. }
        ));
    }
}
