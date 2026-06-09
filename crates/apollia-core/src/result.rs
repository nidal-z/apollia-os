use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::task::{AIPPart, DataPart, TextPart};

// ─────────────────────────────────────────────
// Human-in-the-Loop types
// ─────────────────────────────────────────────

/// Data carried by [`AIPResult`] when `status == InputRequired`.
///
/// Persisted to SQLite by the runtime and restored into
/// [`InputResponseData::context`] on resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputRequiredData {
    /// Prompt shown to the user to make their decision.
    pub prompt: String,
    /// JSON context serialized by the agent at suspension time.
    ///
    /// The runtime stores it verbatim in SQLite and restores it into
    /// [`InputResponseData::context`] on resume.
    pub context: serde_json::Value,
}

/// Human response received after an `input_required` suspension.
///
/// Populated by `TaskRepository::rebuild_for_resume()` and injected into
/// [`crate::task::AIPTask::input_response`] on resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputResponseData {
    /// `true` if the user approved, `false` if rejected.
    pub approved: bool,
    /// Reason passed to the agent: `None` if approved, possibly populated if rejected.
    pub reason: Option<String>,
    /// JSON context serialized by the agent at suspend time, restored verbatim into [`InputResponseData::context`].
    pub context: serde_json::Value,
    /// ISO 8601 timestamp of the human decision.
    pub responded_at: String,
}

/// State machine for an individual task, aligned with A2A TaskState.
///
/// Valid transitions:
/// `Submitted` → `Working` → `Completed`
///                    ↓           ↑ (resume after input)
///                `InputRequired` → `Working`
///                    ↓
///              `Failed` | `Canceled`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task received, waiting for an available agent.
    Submitted,
    /// Task currently being executed by the agent.
    Working,
    /// Task finished successfully.
    Completed,
    /// The agent hit a non-recoverable error.
    Failed,
    /// The agent is waiting for human input to continue (Human-in-the-Loop).
    InputRequired,
    /// Task canceled by the operator or timed out.
    Canceled,
}

/// Result returned by the agent to the runtime through the AIP bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPResult {
    /// Identifier of the corresponding task.
    #[serde(default)]
    pub task_id: String,
    /// Final status of the task.
    pub status: TaskStatus,
    /// Parts of the response produced by the agent.
    #[serde(default)]
    pub output: Vec<AIPPart>,
    /// Structured error if `status == Failed`.
    #[serde(default)]
    pub error: Option<AIPError>,
    /// Artifacts produced by the task (generated files, reports, etc.).
    #[serde(default)]
    pub artifacts: Vec<AIPArtifact>,
    /// Approval request data if `status == InputRequired`.
    ///
    /// Populated by [`AIPResult::input_required`].
    /// Persisted to SQLite by the runtime.
    /// `None` for all other statuses.
    #[serde(default)]
    pub input_required_data: Option<InputRequiredData>,
}

/// Binary artifact produced by a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPArtifact {
    /// Name of the file or artifact.
    pub name: String,
    /// MIME type (e.g. "application/pdf", "text/plain").
    pub mime_type: String,
    /// Binary content of the artifact.
    pub data: Vec<u8>,
}

/// Structured error returned by the agent on failure.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AIPError {
    /// Machine error code (e.g. "TIMEOUT", "TOOL_NOT_FOUND").
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Additional structured details (optional).
    pub details: Option<serde_json::Value>,
}

impl AIPResult {
    /// Builds a result requesting human approval (Human-in-the-Loop).
    ///
    /// The runtime detects this variant via `status == InputRequired`, suspends
    /// the task, persists `prompt` and `context` to SQLite, then notifies the
    /// user over the configured channels.
    ///
    /// On resume, `context` is restored into [`InputResponseData::context`]
    /// injected into [`crate::task::AIPTask::input_response`].
    pub fn input_required(prompt: &str, context: serde_json::Value) -> Self {
        Self {
            task_id: String::new(),
            status: TaskStatus::InputRequired,
            output: vec![],
            error: None,
            artifacts: vec![],
            input_required_data: Some(InputRequiredData {
                prompt: prompt.to_string(),
                context,
            }),
        }
    }

    /// Builds a success result with a response text.
    ///
    /// Shortcut used by `execute_orchestrated` to auto-concatenate step outputs
    /// (fallback when `on_plan_complete()` is absent).
    pub fn completed(text: &str) -> Self {
        Self {
            task_id: String::new(),
            status: TaskStatus::Completed,
            output: vec![AIPPart::Text(TextPart {
                text: text.to_string(),
            })],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        }
    }

    /// Builds a failure result with a structured code and message.
    ///
    /// Shortcut used by `ActorLoop` for budget, step, or plan errors.
    pub fn failed(code: &str, message: &str) -> Self {
        Self {
            task_id: String::new(),
            status: TaskStatus::Failed,
            output: vec![],
            error: Some(AIPError {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            }),
            artifacts: vec![],
            input_required_data: None,
        }
    }

    /// Builds a success result with each step's output serialized to JSON.
    ///
    /// Used by `ActorLoop` at the end of orchestrated execution to pass the
    /// per-step results to the `on_plan_complete()` hook.
    ///
    /// The `HashMap<step_id, output>` is serialized into `output[0]` as
    /// `AIPPart::Data`, falling back to `AIPPart::Text` if serialization fails.
    pub fn completed_with_steps(steps: HashMap<String, String>) -> Self {
        let part = match serde_json::to_value(&steps) {
            Ok(val) => AIPPart::Data(DataPart { data: val }),
            Err(_) => AIPPart::Text(TextPart {
                text: steps
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            }),
        };
        Self {
            task_id: String::new(),
            status: TaskStatus::Completed,
            output: vec![part],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_failed_round_trip() {
        // GIVEN
        let result = AIPResult {
            task_id: "task-123".into(),
            status: TaskStatus::Failed,
            output: vec![],
            error: Some(AIPError {
                code: "TIMEOUT".into(),
                message: "Agent timed out".into(),
                details: None,
            }),
            artifacts: vec![],
            input_required_data: None,
        };
        // WHEN
        let json = serde_json::to_string(&result).expect("serialize failed");
        let restored: AIPResult = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        assert_eq!(restored.status, TaskStatus::Failed);
        assert!(restored.error.is_some());
        assert_eq!(restored.error.unwrap().code, "TIMEOUT");
    }

    #[test]
    fn test_task_status_serialization() {
        // GIVEN
        let status = TaskStatus::Completed;
        // WHEN
        let json = serde_json::to_string(&status).expect("serialize failed");
        let restored: TaskStatus = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        assert_eq!(restored, TaskStatus::Completed);
        assert_eq!(json, "\"completed\"");
    }

    #[test]
    fn test_aip_error_display() {
        // GIVEN
        let err = AIPError {
            code: "TOOL_NOT_FOUND".into(),
            message: "Tool 'bash_executor' not registered".into(),
            details: None,
        };
        // WHEN
        let display = err.to_string();
        // THEN
        assert!(display.contains("TOOL_NOT_FOUND"));
        assert!(display.contains("Tool 'bash_executor' not registered"));
    }
}
