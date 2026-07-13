use super::*;

/// Response produced by a complete chat exchange.
#[derive(Debug, Clone)]
pub struct ChatAgentResponse {
    /// Final text content from the LLM.
    pub content: String,
    /// All tool calls made during the exchange.
    pub tool_calls: Vec<ToolCallRecord>,
    /// Tool names newly added to the session allowlist (via AlwaysAccept).
    pub newly_authorized: Vec<String>,
    /// Cumulative token usage across all LLM calls in the exchange.
    pub tokens_used: TokenUsage,
    /// Concatenated thinking/reasoning blocks extracted from `<think>...</think>` tags.
    pub thinking_trace: Option<String>,
    /// Present when verification ran: at supervised and above, or at the assisted
    /// tier when the agent declares check commands. `None` when verification is
    /// skipped (assisted tier with no declared checks).
    pub verification_report: Option<ConsolidatedVerificationReport>,
    /// True when an escalation was requested during this exchange but the hybrid
    /// cost ceiling kept the step local. The caller may surface a notice to the
    /// user. Stays `false` when hybrid routing is not configured.
    pub frontier_ceiling_reached: bool,
    /// Terminal plan-mode phase of the exchange, when the turn ran in the plan
    /// flow (a substantive plan-mode turn). `None` for conversational turns and
    /// outside plan mode, so the caller persists a phase only when one moved.
    pub final_plan_phase: Option<PlanPhase>,
    /// True when the turn stopped cooperatively at a pause checkpoint rather than
    /// converging on its own terms. The manager uses this to move the session to
    /// [`PauseState::Paused`](crate::chat::types::PauseState::Paused) and keep the
    /// persisted partial step statuses as the source of truth for the resume.
    pub paused: bool,
}

impl ChatAgentResponse {
    /// Returns the terminal disposition of the turn: [`TurnOutcome::Paused`] when
    /// the loop stopped at a pause checkpoint, [`TurnOutcome::Completed`]
    /// otherwise.
    pub fn turn_outcome(&self) -> TurnOutcome {
        if self.paused {
            TurnOutcome::Paused
        } else {
            TurnOutcome::Completed
        }
    }
}

/// Consolidated result of the full post-run verification pass (checks + critic).
#[derive(Debug, Clone)]
pub struct ConsolidatedVerificationReport {
    /// True when all checks passed and the critic found no corrections.
    pub passed: bool,
    /// Failures from the programmed check commands.
    pub check_failures: Vec<CheckFailure>,
    /// Corrections proposed by the LLM critic.
    pub corrections: Vec<Correction>,
    /// Number of retry iterations performed (0 when verification passed first time).
    pub retry_iterations: u32,
}

/// Owned state threaded through the verification retry loop.
///
/// Carrying the conversation buffer and the latest response by value (rather
/// than borrowing them in the retry closure) keeps the closure's future free of
/// borrowed locals, so the spawned execute future stays `Send`.
pub(in crate::chat::builtin_agent) struct RetryCarry {
    /// The running LLM message buffer, appended with each correction turn.
    pub(in crate::chat::builtin_agent) messages: Vec<LlmChatMessage>,
    /// The most recent terminal response from the ReAct loop.
    pub(in crate::chat::builtin_agent) last_response: ChatAgentResponse,
    /// Tools authorized so far, carried across correction turns.
    ///
    /// Seeded from the session's authorized set plus anything the first turn
    /// auto-authorized (an "always accept" HITL decision), then extended after
    /// each retry. Without this, a verification retry would re-prompt for a tool
    /// the user already approved earlier in the same turn.
    pub(in crate::chat::builtin_agent) authorized: HashSet<String>,
}

/// A [`CheckInvoker`] that never executes anything.
///
/// Chat Libre declares no manifest check commands, so the [`VerificationLoop`]
/// resolves an empty command list and never calls the invoker. This placeholder
/// satisfies the generic bound without spawning processes.
pub(in crate::chat::builtin_agent) struct NoopCheckInvoker;

impl CheckInvoker for NoopCheckInvoker {
    async fn invoke_check(&self, _command: &str) -> Result<CheckOutcome, String> {
        Ok(CheckOutcome {
            exit_code: 0,
            stderr: String::new(),
        })
    }
}

/// Tracks the plan-mode phase across one plan-flow turn.
///
/// A substantive plan-mode turn opens in [`PlanPhase::Discovery`]: the agent
/// uses the blocking `ask_user` tool to gather real inputs. The first
/// plan-construction tool call (`plan_propose` or `plan_add_step`) means the
/// agent has enough information and starts drafting, so the phase advances to
/// [`PlanPhase::Drafting`]. If the user cancels every pending question (an
/// all-skipped `ask_user` answer), discovery is abandoned and the phase returns
/// to the safe [`PlanPhase::Done`] state rather than staying stuck in discovery.
///
/// The tracker is `None` for conversational turns and outside plan mode, so a
/// non-plan turn behaves exactly as before with zero overhead.
pub(in crate::chat::builtin_agent) struct PlanPhaseTracker {
    pub(in crate::chat::builtin_agent) phase: PlanPhase,
}

impl PlanPhaseTracker {
    /// Returns whether `name` is a plan-construction tool whose first call ends
    /// discovery and begins drafting.
    pub(in crate::chat::builtin_agent) fn is_construction_tool(name: &str) -> bool {
        name == PLAN_PROPOSE_TOOL_NAME || name == PLAN_ADD_STEP_TOOL_NAME
    }

    /// Returns whether `record` is an `ask_user` result the user fully cancelled
    /// (every answer carries `skipped: true`). A dropped or errored answer is not
    /// treated as a cancellation: it surfaces through the tool error path instead.
    pub(in crate::chat::builtin_agent) fn is_cancelled_ask_user(record: &ToolCallRecord) -> bool {
        if record.tool_name != "ask_user" {
            return false;
        }
        let Some(output) = record.output.as_deref() else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
            return false;
        };
        let Some(answers) = value.get("answers").and_then(|a| a.as_array()) else {
            return false;
        };
        !answers.is_empty()
            && answers
                .iter()
                .all(|a| a.get("skipped").and_then(|s| s.as_bool()) == Some(true))
    }
}
