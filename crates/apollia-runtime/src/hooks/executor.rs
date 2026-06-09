//! The [`HookExecutor`]: delivers lifecycle events to registered handlers.
//!
//! The executor is stateless per invocation and carries its own per-handler
//! timeout. The blocking `PreToolUse` path applies the returned
//! [`HookDecision`]; every other lifecycle hook is best-effort and never blocks
//! or interrupts the agent loop.

use std::process::Stdio;
use std::sync::Arc;

use apollia_core::HookEventKind;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::hooks::registry::{HookRegistry, ResolvedHandler};

/// Decision returned by a `PreToolUse` hook handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    /// The tool call proceeds with its original arguments.
    Allow,
    /// The tool call is blocked. The reason is injected as a synthetic tool
    /// result so the model can react to the refusal.
    Deny {
        /// Operator-facing reason surfaced to the model.
        reason: String,
    },
    /// The tool call proceeds with the provided replacement arguments.
    Rewrite {
        /// Replacement arguments applied before the tool runs.
        arguments: serde_json::Value,
    },
}

/// Payload sent to every `PreToolUse` handler (JSON-serialized).
#[derive(Debug, Serialize)]
struct PreToolUsePayload<'a> {
    event: &'static str,
    tool_name: &'a str,
    arguments: &'a serde_json::Value,
    session_id: &'a str,
}

/// Wire shape of a handler response, parsed from stdout or the HTTP body.
#[derive(Debug, Deserialize)]
struct HookDecisionDto {
    decision: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
}

/// Runs lifecycle hook handlers at defined execution points.
///
/// Built once from a [`HookRegistry`] at startup and shared as a read-only
/// reference (typically an `Arc`) by the execution loop. Each invocation is
/// independent and bounded by the per-handler timeout.
#[derive(Debug)]
pub struct HookExecutor {
    registry: Arc<HookRegistry>,
    /// Shared HTTP client for `http` handlers. `None` when the TLS backend
    /// failed to initialise; `http` handlers then degrade to a safe default.
    http: Option<reqwest::Client>,
}

impl HookExecutor {
    /// Builds an executor over the given registry.
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self {
            registry,
            http: reqwest::Client::builder().build().ok(),
        }
    }

    /// Returns the registry backing this executor.
    pub fn registry(&self) -> &HookRegistry {
        &self.registry
    }

    /// Runs all `PreToolUse` handlers for `tool_name` with `arguments`.
    ///
    /// Handlers are invoked in declaration order. The first `Deny` decision
    /// short-circuits: remaining handlers are not invoked. A handler that times
    /// out, fails to deliver, or returns an unparseable response falls back to
    /// `Allow` (the safe permissive default) and emits a warn-level trace event;
    /// the loop is never blocked. A `Rewrite` is applied unless a later handler
    /// denies.
    ///
    /// Returns `Allow` immediately, without any I/O, when no `PreToolUse`
    /// handler is registered.
    pub async fn run_pre_tool_use(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        session_id: &str,
    ) -> HookDecision {
        let handlers = self.registry.handlers_for(HookEventKind::PreToolUse);
        if handlers.is_empty() {
            return HookDecision::Allow;
        }
        let payload = serde_json::to_string(&PreToolUsePayload {
            event: HookEventKind::PreToolUse.as_str(),
            tool_name,
            arguments,
            session_id,
        })
        .unwrap_or_default();

        let mut effective = HookDecision::Allow;
        for handler in handlers {
            let body = self
                .fetch_response(handler, HookEventKind::PreToolUse.as_str(), &payload)
                .await;
            let Some(body) = body else {
                // Delivery failed; the warn was already emitted. Safe default.
                continue;
            };
            match parse_decision(&body) {
                Some(HookDecision::Deny { reason }) => return HookDecision::Deny { reason },
                Some(rewrite @ HookDecision::Rewrite { .. }) => effective = rewrite,
                Some(HookDecision::Allow) => {}
                None => {
                    tracing::warn!(
                        tool_name = %tool_name,
                        handler_kind = %handler.kind.type_str(),
                        session_id = %session_id,
                        "hook.pretooluse.parse_error: defaulting to allow"
                    );
                }
            }
        }
        effective
    }

    /// Delivers `payload` to a single handler and returns the raw response body.
    ///
    /// Bounds the delivery by `handler.timeout`. Returns `None` on timeout,
    /// spawn/transport failure, or a non-success HTTP status, after emitting a
    /// structured warn-level trace event. Callers treat `None` as the hook's
    /// safe default.
    async fn fetch_response(
        &self,
        handler: &ResolvedHandler,
        event: &str,
        payload: &str,
    ) -> Option<String> {
        let timeout_ms = handler.timeout.as_millis() as u64;
        let result = tokio::time::timeout(handler.timeout, self.deliver(handler, payload)).await;
        match result {
            Ok(Ok(body)) => Some(body),
            Ok(Err(reason)) => {
                tracing::warn!(
                    event = %event,
                    handler_kind = %handler.kind.type_str(),
                    timeout_ms,
                    reason = %reason,
                    "hook.delivery_error: defaulting to safe behavior"
                );
                None
            }
            Err(_elapsed) => {
                tracing::warn!(
                    event = %event,
                    handler_kind = %handler.kind.type_str(),
                    timeout_ms,
                    "hook.timeout: defaulting to safe behavior"
                );
                None
            }
        }
    }

    /// Dispatches one delivery to the handler's transport, returning the raw
    /// response body on success or a human-readable reason on failure.
    async fn deliver(&self, handler: &ResolvedHandler, payload: &str) -> Result<String, String> {
        use apollia_core::HookHandlerKind;
        match &handler.kind {
            HookHandlerKind::Command { command } => Self::run_command(command, payload).await,
            HookHandlerKind::Http { url } => self.run_http(url, payload).await,
        }
    }

    /// Spawns a command handler, writes `payload` to its stdin, and returns its
    /// stdout as a string.
    async fn run_command(argv: &[String], payload: &str) -> Result<String, String> {
        let (exe, args) = argv
            .split_first()
            .ok_or_else(|| "empty command argv".to_string())?;
        let mut child = tokio::process::Command::new(exe)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn failed: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| format!("stdin write failed: {e}"))?;
            // Drop stdin to send EOF so the handler can finish reading.
            drop(stdin);
        }
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| format!("process wait failed: {e}"))?;
        String::from_utf8(output.stdout).map_err(|e| format!("stdout is not utf-8: {e}"))
    }

    /// POSTs `payload` as JSON to an HTTP handler and returns the response body.
    async fn run_http(&self, url: &str, payload: &str) -> Result<String, String> {
        let client = self
            .http
            .as_ref()
            .ok_or_else(|| "http client unavailable".to_string())?;
        let resp = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload.to_string())
            .send()
            .await
            .map_err(|e| format!("http request failed: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("http status {status}"));
        }
        resp.text()
            .await
            .map_err(|e| format!("http body read failed: {e}"))
    }
}

/// Parses a handler response body into a [`HookDecision`].
///
/// Returns `None` when the body is not valid JSON, the `decision` field is
/// unknown, or a `rewrite` decision omits its `arguments`. Callers treat `None`
/// as a parse failure and fall back to the safe default.
fn parse_decision(body: &str) -> Option<HookDecision> {
    let dto: HookDecisionDto = serde_json::from_str(body).ok()?;
    match dto.decision.as_str() {
        "allow" => Some(HookDecision::Allow),
        "deny" => Some(HookDecision::Deny {
            reason: dto.reason.unwrap_or_default(),
        }),
        "rewrite" => dto
            .arguments
            .map(|arguments| HookDecision::Rewrite { arguments }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{HookHandlerConfig, HookHandlerKind, HooksConfig};
    use serde_json::json;

    /// Writes an executable shell script to a tempdir and returns its path.
    fn write_script(dir: &std::path::Path, name: &str, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write script");
        let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
        path.to_string_lossy().into_owned()
    }

    fn executor_with(handlers: Vec<HookHandlerConfig>) -> HookExecutor {
        HookExecutor::new(Arc::new(HookRegistry::from_config(&HooksConfig {
            handlers,
        })))
    }

    fn command_handler(argv: Vec<String>, timeout_ms: u64) -> HookHandlerConfig {
        HookHandlerConfig {
            events: vec![HookEventKind::PreToolUse],
            kind: HookHandlerKind::Command { command: argv },
            timeout_ms,
        }
    }

    #[tokio::test]
    async fn test_allow_decision_returned() {
        // GIVEN a handler that echoes an allow decision
        let dir = tempfile::tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            "allow.sh",
            "#!/bin/sh\nprintf '{\"decision\":\"allow\"}'\n",
        );
        let exec = executor_with(vec![command_handler(vec![script], 5_000)]);

        // WHEN run_pre_tool_use is called
        let decision = exec
            .run_pre_tool_use("bash", &json!({"cmd": "ls"}), "sess-1")
            .await;

        // THEN the decision is Allow
        assert_eq!(decision, HookDecision::Allow);
    }

    #[tokio::test]
    async fn test_deny_decision_blocks_tool() {
        // GIVEN a handler that returns a deny decision with a reason
        let dir = tempfile::tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            "deny.sh",
            "#!/bin/sh\nprintf '{\"decision\":\"deny\",\"reason\":\"policy\"}'\n",
        );
        let exec = executor_with(vec![command_handler(vec![script], 5_000)]);

        // WHEN run_pre_tool_use is called
        let decision = exec
            .run_pre_tool_use("write_file", &json!({}), "sess-1")
            .await;

        // THEN the decision is Deny carrying the reason
        assert_eq!(
            decision,
            HookDecision::Deny {
                reason: "policy".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_rewrite_decision_replaces_arguments() {
        // GIVEN a handler that returns a rewrite decision
        let dir = tempfile::tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            "rewrite.sh",
            "#!/bin/sh\nprintf '{\"decision\":\"rewrite\",\"arguments\":{\"path\":\"/safe/x\"}}'\n",
        );
        let exec = executor_with(vec![command_handler(vec![script], 5_000)]);

        // WHEN run_pre_tool_use is called
        let decision = exec
            .run_pre_tool_use("write_file", &json!({"path": "/etc/passwd"}), "sess-1")
            .await;

        // THEN the decision is Rewrite with the replacement arguments
        assert_eq!(
            decision,
            HookDecision::Rewrite {
                arguments: json!({"path": "/safe/x"})
            }
        );
    }

    #[tokio::test]
    async fn test_timeout_defaults_to_allow() {
        // GIVEN a handler that never responds within the timeout
        let dir = tempfile::tempdir().expect("tempdir");
        let script = write_script(dir.path(), "slow.sh", "#!/bin/sh\nsleep 5\n");
        let exec = executor_with(vec![command_handler(vec![script], 50)]);

        // WHEN run_pre_tool_use is called
        let decision = exec.run_pre_tool_use("bash", &json!({}), "sess-1").await;

        // THEN the decision defaults to Allow after the timeout
        assert_eq!(decision, HookDecision::Allow);
    }

    #[tokio::test]
    async fn test_invalid_response_defaults_to_allow() {
        // GIVEN a handler that returns non-JSON output
        let dir = tempfile::tempdir().expect("tempdir");
        let script = write_script(dir.path(), "junk.sh", "#!/bin/sh\nprintf 'not json'\n");
        let exec = executor_with(vec![command_handler(vec![script], 5_000)]);

        // WHEN run_pre_tool_use is called
        let decision = exec.run_pre_tool_use("bash", &json!({}), "sess-1").await;

        // THEN the decision defaults to Allow
        assert_eq!(decision, HookDecision::Allow);
    }

    #[tokio::test]
    async fn test_missing_binary_defaults_to_allow() {
        // GIVEN a command handler pointing at a non-existent binary
        let exec = executor_with(vec![command_handler(
            vec!["/nonexistent/hook-binary".to_string()],
            5_000,
        )]);

        // WHEN run_pre_tool_use is called
        let decision = exec.run_pre_tool_use("bash", &json!({}), "sess-1").await;

        // THEN the spawn failure falls back to Allow
        assert_eq!(decision, HookDecision::Allow);
    }

    #[tokio::test]
    async fn test_first_deny_wins_across_handlers() {
        // GIVEN two handlers: H1 allows, H2 denies
        let dir = tempfile::tempdir().expect("tempdir");
        let allow = write_script(
            dir.path(),
            "h1.sh",
            "#!/bin/sh\nprintf '{\"decision\":\"allow\"}'\n",
        );
        let deny = write_script(
            dir.path(),
            "h2.sh",
            "#!/bin/sh\nprintf '{\"decision\":\"deny\",\"reason\":\"H2 veto\"}'\n",
        );
        let exec = executor_with(vec![
            command_handler(vec![allow], 5_000),
            command_handler(vec![deny], 5_000),
        ]);

        // WHEN run_pre_tool_use is called
        let decision = exec.run_pre_tool_use("bash", &json!({}), "sess-1").await;

        // THEN the deny from the second handler wins
        assert_eq!(
            decision,
            HookDecision::Deny {
                reason: "H2 veto".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_empty_registry_returns_allow_immediately() {
        // GIVEN an executor over an empty registry
        let exec = executor_with(vec![]);

        // WHEN run_pre_tool_use is called
        let decision = exec.run_pre_tool_use("bash", &json!({}), "sess-1").await;

        // THEN it returns Allow without any I/O
        assert_eq!(decision, HookDecision::Allow);
    }
}
