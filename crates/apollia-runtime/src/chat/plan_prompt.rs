//! Plan-mode system-prompt accessors.
//!
//! The block text lives in `apollia_prompts::blocks` (single source of truth,
//! English). This module keeps the existing accessor API for the chat agent.

/// Returns the plan-mode system-prompt block (discovery / draft / submit).
pub fn plan_mode_block() -> &'static str {
    apollia_prompts::blocks::PLAN_MODE_BLOCK
}

/// Returns the plan-execution system-prompt block (post-approval).
pub fn plan_execute_block() -> &'static str {
    apollia_prompts::blocks::PLAN_EXECUTE_BLOCK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::plan_tool::{
        PLAN_ADD_STEP_TOOL_NAME, PLAN_MODIFY_STEP_TOOL_NAME, PLAN_PROPOSE_TOOL_NAME,
        PLAN_REMOVE_STEP_TOOL_NAME, PLAN_REORDER_TOOL_NAME, PLAN_SET_STEP_STATUS_TOOL_NAME,
        PLAN_SUBMIT_TOOL_NAME,
    };

    #[test]
    fn test_block_only_cites_existing_plan_tools() {
        // GIVEN the plan-mode block text
        let block = plan_mode_block();

        // WHEN checking the cited tool names against the registered surface
        // THEN every cited plan tool exists as a registered constant
        for tool in [
            PLAN_PROPOSE_TOOL_NAME,
            PLAN_ADD_STEP_TOOL_NAME,
            PLAN_MODIFY_STEP_TOOL_NAME,
            PLAN_REMOVE_STEP_TOOL_NAME,
            PLAN_REORDER_TOOL_NAME,
            PLAN_SET_STEP_STATUS_TOOL_NAME,
            PLAN_SUBMIT_TOOL_NAME,
        ] {
            assert!(block.contains(tool), "block must cite {tool}");
        }

        // AND ask_user is cited for the discovery phase
        assert!(block.contains("ask_user"));
    }

    #[test]
    fn test_block_does_not_cite_unregistered_tool() {
        // GIVEN the static block
        let block = plan_mode_block();

        // WHEN inspecting for a tool name that does not exist in the surface
        // THEN the deprecated `plan_status` alias is never mentioned
        assert!(!block.contains("plan_status"));
    }

    #[test]
    fn test_execute_block_instructs_execution_not_replanning() {
        // GIVEN the execution-phase block
        let block = plan_execute_block();
        // WHEN inspecting its wording
        // THEN it tells the model the plan is approved, to track step status, and
        // not to re-propose or wait for approval again
        assert!(block.contains("approved"));
        assert!(block.contains("plan_set_step_status"));
        assert!(block.contains("Do not re-propose"));
    }

    #[test]
    fn test_block_instructs_discovery_and_respects_gates() {
        // GIVEN the static block (no session content interpolated)
        let block = plan_mode_block();

        // WHEN inspecting the safety wording
        // THEN it steers discovery decisively while respecting the step budget
        // and all approval gates.
        assert!(block.contains("Discovery first"));
        assert!(block.contains("step budget"));
        assert!(block.contains("approval gates"));
    }
}
