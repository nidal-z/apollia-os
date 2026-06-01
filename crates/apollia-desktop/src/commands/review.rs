//! Tauri IPC commands for automated code review.
//!
//! Triggers the `apollia-review` agent via the TaskRouter to run an automated
//! code review from the Desktop interface.

use apollia_core::{AIPInput, AIPPart, TextPart};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Parameters to start a code review.
#[derive(Debug, Deserialize)]
pub struct ReviewParams {
    /// Path or Git URL of the target to review.
    pub target: String,
    /// Review type: `"diff"`, `"pr"`, or `"task"`.
    pub review_type: String,
    /// Additional instructions for the review (optional).
    pub instructions: Option<String>,
}

/// Result of starting a code review.
#[derive(Debug, Serialize)]
pub struct ReviewStartResult {
    /// ID of the task created for the review.
    pub task_id: String,
    /// Name of the agent in charge of the review.
    pub agent_id: String,
}

/// Starts an automated code review via the apollia-review agent.
///
/// Looks up an agent named `"apollia-review"` in the registry and submits a
/// task with the review parameters. The task can then be tracked via
/// `get_task_timeline`.
///
/// Returns an error if the `"apollia-review"` agent is not installed.
#[tauri::command]
pub async fn start_code_review(
    state: State<'_, RuntimeHandle>,
    params: ReviewParams,
) -> Result<ReviewStartResult, String> {
    // Find the apollia-review agent in the registry.
    let agents = state
        .registry_handle
        .list_agents()
        .await
        .map_err(|e| e.to_string())?;

    let review_agent = agents
        .iter()
        .find(|a| a.manifest.name == "apollia-review")
        .ok_or_else(|| "agent 'apollia-review' not found - please install it first".to_string())?;

    let agent_id = review_agent.id.to_string();

    // Build the review prompt.
    let instructions = params.instructions.as_deref().unwrap_or("");
    let prompt = format!(
        "Review type: {}\nTarget: {}\n{}",
        params.review_type, params.target, instructions
    );

    let aip_input = AIPInput {
        parts: vec![AIPPart::Text(TextPart { text: prompt })],
    };

    let task_id = state
        .router_handle
        .submit(&agent_id, aip_input)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ReviewStartResult {
        task_id: task_id.to_string(),
        agent_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_params_deserializes_full() {
        // GIVEN a JSON with all review params
        let json = serde_json::json!({
            "target": "https://github.com/apollia/os/pull/42",
            "review_type": "pr",
            "instructions": "Focus on performance"
        });

        // WHEN deserialized
        let params: ReviewParams = serde_json::from_value(json).expect("deserialize");

        // THEN all fields are populated
        assert_eq!(params.target, "https://github.com/apollia/os/pull/42");
        assert_eq!(params.review_type, "pr");
        assert_eq!(params.instructions.as_deref(), Some("Focus on performance"));
    }

    #[test]
    fn test_review_params_deserializes_minimal() {
        // GIVEN a JSON with only required fields
        let json = serde_json::json!({
            "target": "./src/main.rs",
            "review_type": "diff"
        });

        // WHEN deserialized
        let params: ReviewParams = serde_json::from_value(json).expect("deserialize");

        // THEN instructions is None
        assert_eq!(params.target, "./src/main.rs");
        assert!(params.instructions.is_none());
    }

    #[test]
    fn test_review_start_result_serializes() {
        // GIVEN a ReviewStartResult
        let result = ReviewStartResult {
            task_id: "task-review-001".to_string(),
            agent_id: "agent-uuid-abc123".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["task_id"], "task-review-001");
        assert_eq!(json["agent_id"], "agent-uuid-abc123");
    }
}
