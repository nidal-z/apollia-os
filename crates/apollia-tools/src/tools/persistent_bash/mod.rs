//! `persistent_bash`: stateful shell executor.
//!
//! Maintains one `/bin/sh -l` process per `session_id`. The working directory and
//! exported variables persist across successive ORIA steps, enabling patterns like
//! `cd /repo && make && cargo test` without losing context between tool calls.
//!
//! # Architecture
//!
//! ```text
//! PersistentBashExecutor   (implements ToolExecutor, is_read_only = false)
//!   └── ShellSessionRegistry  (Arc, one registry per executor instance)
//!         └── ShellSession    (Arc<Mutex<>>, one /bin/sh -l per session_id)
//! ```

pub mod registry;
pub mod session;

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use apollia_core::SandboxProfile;

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::executor::{ToolExecutionError, ToolExecutor};

use self::registry::ShellSessionRegistry;
use self::session::{PersistentBashError, SessionId, ShellOutput};

/// Stateful shell executor: executes commands in a long-lived `/bin/sh -l` process.
///
/// A `PersistentBashExecutor` wraps a [`ShellSessionRegistry`] and implements
/// [`ToolExecutor`]. The registry is shared via `Arc` so a single executor instance
/// can serve all agents running on the same runtime.
///
/// `is_read_only` returns `false`: this tool is never parallelised by
/// [`crate::executor::ToolDispatcher::execute_batch`].
pub struct PersistentBashExecutor {
    registry: Arc<ShellSessionRegistry>,
}

impl PersistentBashExecutor {
    /// Creates a new executor backed by the given `registry`.
    pub fn new(registry: Arc<ShellSessionRegistry>) -> Self {
        Self { registry }
    }

    /// Returns the [`ToolDescriptor`] for registration in [`crate::registry::ToolRegistry`].
    pub fn descriptor() -> ToolDescriptor {
        Self::build_descriptor()
    }

    fn build_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "persistent_bash".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute a shell command in a persistent session that preserves \
                          the working directory and environment variables between calls. \
                          Use the same session_id across steps to chain commands."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "required": ["command", "session_id"],
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute. CWD and env vars from previous \
                                        calls in the same session are preserved."
                    },
                    "session_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "UUID identifying the shell session. Use one UUID per agent \
                                        to keep sessions isolated."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 600,
                        "default": 30,
                        "description": "Hard timeout in seconds. The session is restarted at the \
                                        last known CWD if the command exceeds this limit."
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "stdout":    { "type": "string" },
                    "stderr":    { "type": "string" },
                    "exit_code": { "type": "integer" },
                    "cwd":       { "type": "string" }
                }
            })),
            sandbox_profile: SandboxProfile::Full,
            tags: vec!["shell".to_string(), "stateful".to_string()],
            // Full profile requires dangerous=true (the session is long-lived and mutable).
            dangerous: true,
            // A persistent shell mutates external state and must never be parallelised.
            is_read_only: false,
            risk_score: 8,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    /// Formats a [`ShellOutput`] as the JSON value returned by [`execute`].
    fn output_to_json(output: ShellOutput) -> serde_json::Value {
        json!({
            "stdout":    output.stdout,
            "stderr":    output.stderr,
            "exit_code": output.exit_code,
            "cwd":       output.cwd,
        })
    }

    /// Maps a [`PersistentBashError`] to a [`ToolExecutionError`].
    fn map_error(e: PersistentBashError) -> ToolExecutionError {
        let (code, message) = match &e {
            PersistentBashError::SyntaxError(m) => ("syntax_error", m.clone()),
            PersistentBashError::Timeout => ("timeout", "command timed out".into()),
            PersistentBashError::Cancelled => ("cancelled", "command was cancelled".into()),
            PersistentBashError::SpawnFailed(m) => ("spawn_failed", m.clone()),
            PersistentBashError::Io(m) => ("io_error", m.clone()),
            PersistentBashError::SyntaxCheckTimeout => {
                ("syntax_check_timeout", "syntax check timed out".into())
            }
        };
        ToolExecutionError::ExecutionFailed {
            code: code.to_string(),
            message,
        }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for PersistentBashExecutor {
    fn name(&self) -> &str {
        "persistent_bash"
    }

    /// Returns `false`: this tool mutates external state and is never parallelised.
    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolExecutionError> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| ToolExecutionError::InvalidInput {
                message: "required field 'command' is missing or not a string".into(),
            })?
            .to_string();

        let session_id_str =
            input["session_id"]
                .as_str()
                .ok_or_else(|| ToolExecutionError::InvalidInput {
                    message: "required field 'session_id' is missing or not a string".into(),
                })?;

        let session_id: SessionId =
            session_id_str
                .parse()
                .map_err(|_| ToolExecutionError::InvalidInput {
                    message: format!("'session_id' must be a valid UUID - got: '{session_id_str}'"),
                })?;

        let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(30);

        // Use the process's current directory as the initial CWD for new sessions.
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "/tmp".to_string());

        tracing::debug!(
            session_id = %session_id,
            command = %command,
            timeout_secs = timeout_secs,
            "persistent_bash: dispatching command"
        );

        let session_arc = self
            .registry
            .get_or_create(session_id, &cwd)
            .await
            .map_err(Self::map_error)?;

        // Each call creates its own CancellationToken. External cancellation from
        // the ORIA step budget is wired in at the ORIAEngine level (future story).
        let cancel = CancellationToken::new();
        let timeout = Duration::from_secs(timeout_secs);

        let output = {
            let mut session = session_arc.lock().await;
            session.exec(&command, timeout, cancel).await
        }
        .map_err(Self::map_error)?;

        tracing::debug!(
            session_id = %session_id,
            exit_code = output.exit_code,
            cwd = %output.cwd,
            "persistent_bash: command finished"
        );

        Ok(Self::output_to_json(output))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn make_executor() -> PersistentBashExecutor {
        PersistentBashExecutor::new(Arc::new(ShellSessionRegistry::new()))
    }

    #[test]
    fn test_descriptor_is_valid() {
        let d = PersistentBashExecutor::descriptor();
        assert_eq!(d.name, "persistent_bash");
        assert!(d.validate().is_ok(), "descriptor failed validation");
    }

    #[test]
    fn test_is_read_only_false() {
        let exec = make_executor();
        assert!(!exec.is_read_only());
    }

    #[tokio::test]
    async fn test_execute_missing_command_returns_invalid_input() {
        // GIVEN
        let exec = make_executor();

        // WHEN: input has no "command" field
        let result = exec
            .execute(json!({ "session_id": Uuid::new_v4().to_string() }))
            .await;

        // THEN
        assert!(
            matches!(result, Err(ToolExecutionError::InvalidInput { .. })),
            "expected InvalidInput - got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_execute_invalid_session_id_returns_invalid_input() {
        // GIVEN
        let exec = make_executor();

        // WHEN: session_id is not a UUID
        let result = exec
            .execute(json!({ "command": "echo hi", "session_id": "not-a-uuid" }))
            .await;

        // THEN
        assert!(
            matches!(result, Err(ToolExecutionError::InvalidInput { .. })),
            "expected InvalidInput - got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_execute_echo_returns_stdout() {
        // GIVEN
        let exec = make_executor();
        let session_id = Uuid::new_v4().to_string();

        // WHEN
        let result = exec
            .execute(json!({
                "command": "echo hello_world",
                "session_id": session_id,
                "timeout_secs": 10
            }))
            .await
            .expect("execute failed");

        // THEN
        assert!(
            result["stdout"]
                .as_str()
                .unwrap_or("")
                .contains("hello_world"),
            "stdout: {}",
            result["stdout"]
        );
        assert_eq!(result["exit_code"], 0);
    }

    #[tokio::test]
    async fn test_execute_cwd_persists_across_calls() {
        // GIVEN
        let exec = make_executor();
        let session_id = Uuid::new_v4().to_string();

        // WHEN: cd then pwd in the same session
        exec.execute(json!({
            "command": "cd /var",
            "session_id": session_id,
            "timeout_secs": 5
        }))
        .await
        .expect("cd failed");

        let result = exec
            .execute(json!({
                "command": "pwd",
                "session_id": session_id,
                "timeout_secs": 5
            }))
            .await
            .expect("pwd failed");

        // THEN
        let cwd = result["cwd"].as_str().unwrap_or("");
        let stdout = result["stdout"].as_str().unwrap_or("");
        assert!(
            cwd.contains("/var") || stdout.contains("/var"),
            "expected /var - cwd='{cwd}' stdout='{stdout}'"
        );
    }

    #[tokio::test]
    async fn test_execute_syntax_error_maps_to_execution_failed() {
        // GIVEN
        let exec = make_executor();

        // WHEN
        let result = exec
            .execute(json!({
                "command": "if then fi",
                "session_id": Uuid::new_v4().to_string()
            }))
            .await;

        // THEN
        match result {
            Err(ToolExecutionError::ExecutionFailed { code, .. }) => {
                assert_eq!(code, "syntax_error");
            }
            other => panic!("expected ExecutionFailed(syntax_error) - got: {other:?}"),
        }
    }
}
