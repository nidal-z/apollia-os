//! Plan-mode system-prompt fragment.
//!
//! Holds the static instruction block injected into the chat system prompt when
//! a session runs in plan mode. The block is constant (no session content is
//! interpolated), so it can never carry user-supplied prompt injection. It only
//! references tools that the runtime actually registers: the `plan_*` surface
//! and the blocking `ask_user` tool.

/// System-prompt fragment injected when a chat session runs in plan mode.
///
/// The block instructs the model to discover missing or ambiguous inputs first
/// (via the blocking `ask_user` tool), draft a plan with a rationale per step,
/// submit it with `plan_submit`, then execute while keeping step statuses
/// current and justifying any later change with a reason. It never invites
/// bypassing the step budget or the human-in-the-loop safeguards.
pub const PLAN_MODE_BLOCK: &str = "\
You are operating in plan mode for this session.

1. Discovery first. If any required input is missing or ambiguous, ask the user \
with the `ask_user` tool before drafting. Do not invent assumptions.
2. Draft the plan. Use `plan_propose` then `plan_add_step` to lay out ordered, \
dependency-aware steps. Give every step a short rationale explaining why it exists.
3. Submit. Call `plan_submit` once the plan is complete and coherent.
4. Execute. After approval, work the steps in order. Keep each step status \
current with `plan_set_step_status` (in_progress, completed, skipped, failed).
5. Adjust with reason. If you must add, modify, remove, or reorder a step, do it \
through the matching plan tool (`plan_add_step`, `plan_modify_step`, \
`plan_remove_step`, `plan_reorder`) and provide a reason. Never silently diverge \
from the submitted plan.

Respect the step budget and all approval gates at all times.";

/// Returns the plan-mode system-prompt block.
pub fn plan_mode_block() -> &'static str {
    PLAN_MODE_BLOCK
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
    fn test_block_instructs_discovery_not_assumptions() {
        // GIVEN the static block (no session content interpolated)
        let block = plan_mode_block();

        // WHEN inspecting the safety wording
        // THEN it tells the model not to invent assumptions and to respect gates
        assert!(block.contains("Do not invent assumptions"));
        assert!(block.contains("step budget"));
    }
}
