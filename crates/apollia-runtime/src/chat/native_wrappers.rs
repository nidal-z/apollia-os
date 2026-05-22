//! Wrapper [`ToolExecutor`] implementations that let the chat dispatcher
//! host every native tool — including HITL-sensitive ones and tools with
//! per-call side state — without keeping a parallel `invoke_*` fast path
//! on `NativeChatToolInvoker` (ADR-096 Phase 4 — full convergence).
//!
//! Two wrappers today:
//!
//! - [`HitlFilesystemGuard`] — runs the same Risk classification +
//!   approval-event flow that lived in `check_fs_hitl` inline, then
//!   delegates to the inner executor on approval.
//! - [`DynamicAllowlistHttpFetch`] — preserves Chat Libre's habit of
//!   adding the requested URL's host to the allowlist on the fly. The
//!   stock `HttpFetch` is constructed with a static allowlist; this
//!   wrapper builds a fresh one per call with `[host_from_url]`.
//!
//! The wrappers are constructed by `chat::manager::resolve_workspace_for_session`
//! when it builds the per-session [`ToolDispatcher`], so the dispatcher
//! owns every native tool uniformly — no fast path, no special cases.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use apollia_core::{FilesystemPreview, RuntimeEvent};
use apollia_tools::executor::{ToolExecutionError, ToolExecutor};
use apollia_tools::tools::file_edit::FileEditInput;
use apollia_tools::tools::file_write::FileWriteInput;
use apollia_tools::tools::http_fetch::{HttpFetch, HttpFetchInput};
use apollia_tools::{FilesystemOp, RiskClassifier, RiskLevel};
use async_trait::async_trait;
use serde_json::Value;

use crate::chat::types::{FsHitlDecision, PendingFilesystemApprovals};
use crate::eventbus::EventBusSender;

// ─── HitlFilesystemGuard ────────────────────────────────────────────────────

/// Per-session HITL filesystem state shared by every wrapped executor for a
/// given chat session. Cheap to clone (all fields are `Arc`/`Clone`).
#[derive(Clone)]
pub struct HitlFilesystemContext {
    /// EventBus used to emit `HitlFilesystemRequired`.
    pub event_bus: EventBusSender,
    /// Registry used to await the user's decision on a request_id.
    pub pending_fs: PendingFilesystemApprovals,
    /// Session-scoped in-memory allow rules (`<op>:<level>` keys). Mutated
    /// when the user picks `AlwaysAllow`.
    pub fs_allow_rules: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Chat session id used to scope events + persistence.
    pub session_id: String,
    /// Project workspace path for risk classification (paths outside the
    /// workspace are escalated by the classifier).
    pub workspace_path: Option<PathBuf>,
    /// Sandbox root joined with the input's relative path to obtain the
    /// resolved absolute path before classification.
    pub sandbox_root: PathBuf,
    /// Operator-configured high-risk path lists (system / credentials).
    pub risk_config: apollia_core::FilesystemRiskConfig,
}

/// Wraps a write/edit executor with the HITL approval flow that used to
/// live in `NativeChatToolInvoker::check_fs_hitl`. The wrapper:
///
/// 1. Parses the inner tool's input to compute the resolved absolute path.
/// 2. Builds a [`FilesystemPreview`] (diff for write/edit, `Open` for
///    file_read-style reads).
/// 3. Classifies the operation's risk level.
/// 4. For risk ≥ Medium: emits `HitlFilesystemRequired`, awaits the user's
///    decision (5 min timeout, default Deny).
/// 5. Delegates to the inner executor on Approve / AlwaysAllow.
///
/// Generic over the inner executor so a single wrapper covers file_write,
/// file_edit, notebook_edit, bash_executor, python_executor without forcing
/// a trait-object boxing at construction.
pub struct HitlFilesystemGuard {
    inner: Box<dyn ToolExecutor>,
    op: FilesystemOp,
    ctx: HitlFilesystemContext,
}

impl HitlFilesystemGuard {
    /// Wrap `inner` so every invocation routes through the HITL approval
    /// flow first. `op` must match the inner tool's logical filesystem
    /// operation — used to compute the rule key (`<op>:<level>`).
    pub fn new(inner: Box<dyn ToolExecutor>, op: FilesystemOp, ctx: HitlFilesystemContext) -> Self {
        Self { inner, op, ctx }
    }

    fn name_str(&self) -> &str {
        self.inner.name()
    }

    /// Extract the resolved target path from the input. Each wrapped tool
    /// expects a different field name (`path` for file ops, `notebook_path`
    /// for notebooks). Returns `None` for tools whose input has no obvious
    /// path field — those still run HITL with a synthetic "workspace" path
    /// so the classifier can decide based on the workspace as a whole.
    fn resolved_path(&self, input: &Value) -> PathBuf {
        let raw = input
            .get("path")
            .and_then(Value::as_str)
            .or_else(|| input.get("notebook_path").and_then(Value::as_str))
            .or_else(|| input.get("working_dir").and_then(Value::as_str))
            .unwrap_or("");
        let joined = if raw.is_empty() {
            self.ctx
                .workspace_path
                .clone()
                .unwrap_or_else(|| self.ctx.sandbox_root.clone())
        } else {
            self.ctx.sandbox_root.join(raw)
        };
        joined.canonicalize().unwrap_or(joined)
    }

    /// Build a per-tool preview shown in the approval modal. Diff for the
    /// two text-edit tools, generic `Open` for bash/python/notebook.
    async fn build_preview(&self, input: &Value, resolved: &std::path::Path) -> FilesystemPreview {
        match self.name_str() {
            "file_write" => {
                let after = serde_json::from_value::<FileWriteInput>(input.clone())
                    .map(|i| i.content)
                    .unwrap_or_default();
                let before = tokio::fs::read_to_string(resolved)
                    .await
                    .unwrap_or_default();
                truncate_diff(before, after)
            }
            "file_edit" => {
                let parsed = serde_json::from_value::<FileEditInput>(input.clone()).ok();
                let before = parsed
                    .as_ref()
                    .map(|p| p.old_text.clone())
                    .unwrap_or_default();
                let after = parsed.map(|p| p.new_text).unwrap_or_default();
                truncate_diff(before, after)
            }
            _ => FilesystemPreview::Content {
                content: format!("(action on {})", resolved.display()),
                size_bytes: 0,
                truncated: false,
            },
        }
    }

    async fn await_decision(
        &self,
        resolved: &std::path::Path,
        preview: FilesystemPreview,
    ) -> Result<(), ToolExecutionError> {
        let level = RiskClassifier::classify_filesystem(
            self.op,
            resolved,
            self.ctx.workspace_path.as_deref(),
            &self.ctx.risk_config,
        );
        if level < RiskLevel::Medium {
            return Ok(());
        }

        // In-memory session allow rules (`<op>:<level>` keys).
        let rule_key = format!("{}:{}", self.op.as_str(), level.as_str());
        {
            let guard = self
                .ctx
                .fs_allow_rules
                .lock()
                .expect("fs_allow_rules lock poisoned");
            if guard.contains(&rule_key) {
                return Ok(());
            }
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        let _ = self
            .ctx
            .event_bus
            .send(RuntimeEvent::HitlFilesystemRequired {
                request_id: request_id.clone(),
                session_id: self.ctx.session_id.clone(),
                level: level.as_str().to_string(),
                op: self.op.as_str().to_string(),
                path: resolved.to_string_lossy().to_string(),
                preview,
            });

        let rx = self.ctx.pending_fs.register(request_id);
        let decision = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_else(FsHitlDecision::deny);

        match decision {
            FsHitlDecision::Approve => Ok(()),
            FsHitlDecision::Deny { reason } => Err(ToolExecutionError::ExecutionFailed {
                code: "user_denied".into(),
                message: reason.unwrap_or_else(|| "User denied filesystem operation".to_string()),
            }),
            FsHitlDecision::AlwaysAllow {
                scope: _,
                op: rule_op,
                level: rule_level,
            } => {
                let mut guard = self
                    .ctx
                    .fs_allow_rules
                    .lock()
                    .expect("fs_allow_rules lock poisoned");
                guard.insert(format!("{rule_op}:{rule_level}"));
                Ok(())
            }
        }
    }
}

#[async_trait]
impl ToolExecutor for HitlFilesystemGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn is_read_only(&self) -> bool {
        // HITL-guarded tools are by definition mutating (or shell access).
        false
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let resolved = self.resolved_path(&input);
        let preview = self.build_preview(&input, &resolved).await;
        self.await_decision(&resolved, preview).await?;
        self.inner.execute(input).await
    }
}

/// Truncate `before` and `after` to 4 KiB each before wrapping in
/// [`FilesystemPreview::Diff`] so the approval modal isn't drowned in
/// gigabyte content previews.
fn truncate_diff(before: String, after: String) -> FilesystemPreview {
    const LIMIT: usize = 4096;
    let truncated = before.len() > LIMIT || after.len() > LIMIT;
    let before = before.chars().take(LIMIT).collect::<String>();
    let after = after.chars().take(LIMIT).collect::<String>();
    FilesystemPreview::Diff {
        before,
        after,
        truncated,
    }
}

// ─── DynamicAllowlistHttpFetch ──────────────────────────────────────────────

/// Wraps `http_fetch` to preserve the Chat Libre behaviour where the host
/// of the requested URL is injected into the allowlist for that single
/// call. The stock [`HttpFetch`] executor is built once with a static
/// allowlist — in Chat Libre, every tool call is HITL-approved upstream,
/// so allowing the call-specific host is the right default.
pub struct DynamicAllowlistHttpFetch;

impl DynamicAllowlistHttpFetch {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ToolExecutor for DynamicAllowlistHttpFetch {
    fn name(&self) -> &str {
        "http_fetch"
    }

    fn is_read_only(&self) -> bool {
        // GET-style fetch is read-only from the agent's perspective even
        // though network I/O happens — keeps batching semantics intact.
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let parsed: HttpFetchInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: format!("http_fetch: invalid arguments: {e}"),
            })?;
        let hostname =
            extract_hostname(&parsed.url).ok_or_else(|| ToolExecutionError::InvalidInput {
                message: "http_fetch: cannot parse hostname from URL".into(),
            })?;

        let tool = HttpFetch::new(Some(vec![hostname]));
        let output = tool
            .run(parsed)
            .await
            .map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "http_fetch".into(),
                message: e.to_string(),
            })?;
        serde_json::to_value(&output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialise".into(),
            message: e.to_string(),
        })
    }
}

impl Default for DynamicAllowlistHttpFetch {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the hostname from a URL string. Minimal best-effort parser —
/// strips `scheme://`, takes everything before the next `/` or `:`.
fn extract_hostname(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host_with_port = after_scheme.split('/').next()?;
    let host = host_with_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_hostname_strips_scheme() {
        assert_eq!(
            extract_hostname("https://api.example.com/foo").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(
            extract_hostname("http://localhost:8080/x").as_deref(),
            Some("localhost")
        );
        assert_eq!(
            extract_hostname("api.example.com").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(extract_hostname("").as_deref(), None);
    }

    #[test]
    fn truncate_diff_marks_truncation() {
        let big = "x".repeat(8000);
        let preview = truncate_diff(big.clone(), big);
        match preview {
            FilesystemPreview::Diff { truncated, .. } => assert!(truncated),
            _ => panic!("expected Diff"),
        }
    }

    #[test]
    fn truncate_diff_keeps_small_intact() {
        let preview = truncate_diff("a".into(), "b".into());
        match preview {
            FilesystemPreview::Diff {
                before,
                after,
                truncated,
            } => {
                assert_eq!(before, "a");
                assert_eq!(after, "b");
                assert!(!truncated);
            }
            _ => panic!("expected Diff"),
        }
    }
}
