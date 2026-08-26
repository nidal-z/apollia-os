use super::*;

/// Run the optional critic pass, treating an absent critic as a skipped success.
async fn run_critic_pass(
    critic: Option<&CriticPass>,
    objective: &str,
    output: &str,
) -> CriticReport {
    match critic {
        Some(critic) => critic.run(objective, output).await,
        None => CriticReport {
            passed: true,
            corrections: Vec::new(),
            skipped: true,
        },
    }
}

/// Build the correction message injected into the LLM context for a retry turn.
///
/// Emits an XML-like, English block listing the failed checks and the critic
/// corrections, followed by an instruction to address them. The format is meant
/// to be parsed by the model, not displayed to the user.
pub(in crate::chat::builtin_agent) fn correction_message(
    check_failures: &[CheckFailure],
    corrections: &[Correction],
) -> String {
    let mut msg = String::from("<verification_feedback>\n  <check_failures>\n");
    for failure in check_failures {
        msg.push_str(&format!(
            "    <check command=\"{}\" exit_code=\"{}\">{}</check>\n",
            failure.command, failure.exit_code, failure.stderr
        ));
    }
    msg.push_str("  </check_failures>\n  <corrections>\n");
    for correction in corrections {
        msg.push_str(&format!(
            "    <correction kind=\"{}\">\n      <description>{}</description>\n      \
             <suggestion>{}</suggestion>\n    </correction>\n",
            correction.kind, correction.description, correction.suggestion
        ));
    }
    msg.push_str("  </corrections>\n");
    msg.push_str(
        "  <instruction>Please address the issues above and provide a corrected \
         output.</instruction>\n",
    );
    msg.push_str("</verification_feedback>");
    msg
}

/// Run the post-loop verification (checks + critic) with a bounded retry.
///
/// Returns `None` when the tier is assisted or when no [`VerificationLoop`] is
/// configured. Otherwise it runs the checks and the optional critic on the
/// initial output; on failure it injects a correction and re-runs the loop via
/// `retry_fn`, up to `max_retries` times, stopping early when the budget is
/// exhausted. The budget is the hard ceiling: no retry starts once it is spent.
///
/// The retry state `state` is threaded by value through `retry_fn`, which always
/// returns it back alongside the new output (or an error). Owning the state
/// avoids capturing borrowed locals in the retry closure, which keeps the
/// returned future `Send` for `tokio::spawn`. The second tuple element is the
/// final state so the caller can recover the latest run's response.
// REASON: generic retry driver: the arguments are the closures and bounds of one verification pass.
#[allow(clippy::too_many_arguments)]
pub(in crate::chat::builtin_agent) async fn run_verification_with_retry<I, S, F, Fut>(
    autonomy: &AutonomyLevel,
    verification: Option<&VerificationLoop>,
    critic: Option<&CriticPass>,
    invoker: &I,
    objective: &str,
    agent_output: &str,
    budget: &StepBudget,
    max_retries: u32,
    initial_state: S,
    mut retry_fn: F,
) -> (Option<ConsolidatedVerificationReport>, S)
where
    I: CheckInvoker,
    F: FnMut(S, String) -> Fut,
    Fut: std::future::Future<Output = (Result<String, ChatError>, S)>,
{
    let Some(verification) = verification else {
        return (None, initial_state);
    };
    // At the assisted tier, run only the deterministic checks the agent declared,
    // with no LLM critic and no retries: declared checks count by default at no
    // extra cost, and an agent that declares none is left untouched.
    let assisted = matches!(autonomy, AutonomyLevel::Assisted);
    if assisted && !verification.has_commands() {
        return (None, initial_state);
    }
    let critic = if assisted { None } else { critic };
    let max_retries = if assisted { 0 } else { max_retries };

    let mut state = initial_state;
    let mut current_output = agent_output.to_string();
    let mut retry_iterations = 0;

    let mut check_report = verification.run(invoker).await;
    let mut critic_report = run_critic_pass(critic, objective, &current_output).await;
    let mut passed = check_report.passed && critic_report.passed;

    while !passed && retry_iterations < max_retries && !budget.is_exhausted() {
        let message = correction_message(&check_report.failures, &critic_report.corrections);
        let (result, next_state) = retry_fn(state, message).await;
        state = next_state;
        match result {
            Ok(new_output) => current_output = new_output,
            Err(error) => {
                tracing::warn!(error = %error, "chat.verification.retry_failed");
                break;
            }
        }
        retry_iterations += 1;
        check_report = verification.run(invoker).await;
        critic_report = run_critic_pass(critic, objective, &current_output).await;
        passed = check_report.passed && critic_report.passed;
    }

    let report = ConsolidatedVerificationReport {
        passed,
        check_failures: check_report.failures,
        corrections: critic_report.corrections,
        retry_iterations,
    };
    (Some(report), state)
}

/// Build LLM messages from system prompt, chat history, and current user message.
///
/// Applies a sliding window over history: only the last `context_window_size`
/// messages are included. When a conversation summary is available, it is
/// injected as a system message between the system prompt and the windowed
/// history to preserve context from older messages.
///
/// Message order: system prompt, [summary], windowed history, user message.
pub(in crate::chat::builtin_agent) fn build_llm_messages(
    system_prompt: &str,
    history: &[ChatMessage],
    user_message: &str,
    summary: Option<&str>,
    context_window_size: usize,
) -> Vec<LlmChatMessage> {
    let window_size = if context_window_size == 0 {
        DEFAULT_CONTEXT_WINDOW_SIZE
    } else {
        context_window_size
    };

    let windowed_history = if history.len() > window_size {
        let start = history.len() - window_size;
        &history[start..]
    } else {
        history
    };

    let mut messages = Vec::with_capacity(windowed_history.len() + 4);

    messages.push(LlmChatMessage::system(system_prompt));

    if let Some(summary_text) = summary {
        if !summary_text.is_empty() {
            messages.push(LlmChatMessage::system(format!(
                "Previous context summary:\n{summary_text}"
            )));
        }
    }

    for msg in windowed_history {
        match msg.role {
            ChatRole::User => messages.push(LlmChatMessage::user(&msg.content)),
            ChatRole::Assistant => messages.push(LlmChatMessage::assistant(&msg.content)),
            ChatRole::Tool => {
                let call_id = msg.tool_name.as_deref().unwrap_or("unknown");
                messages.push(LlmChatMessage::tool_result(call_id, &msg.content));
            }
            ChatRole::System => {
                // System messages from history are skipped, we already have the prompt
            }
        }
    }

    messages.push(LlmChatMessage::user(user_message));
    messages
}

/// Extract the hostname from a URL string for use as an http_fetch allowlist entry.
///
/// Handles common shapes: `https://host/path`, `http://host:port/path`.
/// Returns `None` for malformed URLs or those without a hostname.
pub(in crate::chat::builtin_agent) fn extract_hostname(url: &str) -> Option<String> {
    let rest = url.find("://").map(|i| &url[i + 3..])?;
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host_and_port = &rest[..host_end];
    // Strip port if present (skip IPv6 brackets)
    let host = if !host_and_port.starts_with('[') {
        if let Some(colon) = host_and_port.rfind(':') {
            if host_and_port[colon + 1..]
                .chars()
                .all(|c| c.is_ascii_digit())
            {
                &host_and_port[..colon]
            } else {
                host_and_port
            }
        } else {
            host_and_port
        }
    } else {
        host_and_port
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Convert available tool names to LLM-compatible [`ToolSpec`]s via the registry.
///
/// In eager mode (`mcp_index` is `None`) this resolves every entry in
/// `available_tools` from the registry, exactly as before this change.
///
/// In deferred mode (`mcp_index` is `Some`) the individual `mcp:` names are
/// skipped and a single synthetic `tool_search` spec is appended instead, so the
/// LLM discovers MCP tools by intent rather than receiving every schema up front.
/// `tool_search_limit` is the upper bound advertised in that spec's description.
/// Overrides the `provenance` field of the `step` object inside a `plan_add_step`
/// or `plan_modify_step` argument payload with `provenance`.
///
/// Returns a cloned argument value with the provenance forced; the original is
/// left untouched. When the payload has no object `step` field (a malformed call),
/// the args are returned unchanged so the downstream parser still reports the
/// error path rather than this helper masking it.
pub(in crate::chat::builtin_agent) fn stamp_inject_provenance(
    args: &serde_json::Value,
    provenance: &apollia_core::plan::StepProvenance,
) -> serde_json::Value {
    let mut out = args.clone();
    if let Some(step) = out
        .get_mut("step")
        .and_then(serde_json::Value::as_object_mut)
    {
        if let Ok(value) = serde_json::to_value(provenance) {
            step.insert("provenance".to_string(), value);
        }
    }
    out
}

/// Lowercase status token for a plan step, matching the vocabulary the plan
/// tools accept (`pending`, `in_progress`, `completed`, `skipped`, `failed`).
pub(in crate::chat::builtin_agent) fn step_status_token(
    status: &apollia_core::plan::StepStatus,
) -> &'static str {
    use apollia_core::plan::StepStatus;
    match status {
        StepStatus::Pending => "pending",
        StepStatus::InProgress => "in_progress",
        StepStatus::Completed => "completed",
        StepStatus::Skipped => "skipped",
        StepStatus::Failed => "failed",
    }
}

/// Returns `true` when `name` is one of the `plan_*` built-in tools.
pub(in crate::chat::builtin_agent) fn is_plan_tool(name: &str) -> bool {
    matches!(
        name,
        PLAN_PROPOSE_TOOL_NAME
            | PLAN_ADD_STEP_TOOL_NAME
            | PLAN_MODIFY_STEP_TOOL_NAME
            | PLAN_REMOVE_STEP_TOOL_NAME
            | PLAN_REORDER_TOOL_NAME
            | PLAN_SET_STEP_STATUS_TOOL_NAME
            | PLAN_SUBMIT_TOOL_NAME
    )
}

/// Whether the plan-mode hard gate refuses a single tool call.
///
/// When the gate is engaged (`gate_blocks`), the `plan_*` surface, `ask_user`,
/// and read-only tools are allowed (the agent may inspect to inform the plan);
/// only execution / write tools are refused, so side effects wait for approval.
pub(in crate::chat::builtin_agent) fn plan_gate_denies(
    gate_blocks: bool,
    name: &str,
    read_only: bool,
) -> bool {
    gate_blocks && !is_plan_tool(name) && name != "ask_user" && !read_only
}

/// Whether an in-loop plan call must be refused because the plan is already
/// approved and executing.
///
/// Only the proposal surface (`plan_propose`, `plan_submit`) is refused: the
/// step-status and amendment tools stay legitimate mid-run. This is the phase
/// check the in-loop plan dispatch itself does not perform; without it, the only
/// barrier against a post-approval `plan_propose` is the unadvertised-name
/// validation, and any session that carries the name in its authorized tools
/// would silently replace the approved plan.
pub(in crate::chat::builtin_agent) fn executing_denies_proposal(
    plan_mode_active: bool,
    phase: PlanPhase,
    name: &str,
) -> bool {
    plan_mode_active
        && phase == PlanPhase::Executing
        && matches!(name, PLAN_PROPOSE_TOOL_NAME | PLAN_SUBMIT_TOOL_NAME)
}

/// Builds a corrective refusal reason when the model calls a tool name that was
/// never advertised this turn, or `None` when the name is valid.
///
/// The message lists the callable tools and, when one is close enough, a
/// "did you mean" hint. The model reads it as a tool result and can recover on
/// its next turn instead of silently looping on a name that does not exist.
pub(in crate::chat::builtin_agent) fn unknown_tool_reason(
    name: &str,
    valid_tool_names: &HashSet<String>,
) -> Option<String> {
    if valid_tool_names.contains(name) {
        return None;
    }
    let mut available: Vec<&str> = valid_tool_names.iter().map(String::as_str).collect();
    available.sort_unstable();
    let suggestion = suggest_tool_name(name, &available)
        .map(|s| format!(" Did you mean `{s}`?"))
        .unwrap_or_default();
    Some(format!(
        "unknown tool `{name}`; it is not in your available tools. \
         Available tools: {}.{suggestion} Call one of the listed tools, or tell the user \
         this action is not possible with your current tools.",
        available.join(", ")
    ))
}

/// Returns the closest advertised tool name to `name` when one is near enough to
/// be a plausible typo or misremembering, using Levenshtein distance.
///
/// The threshold scales with the name length (at most a third of it, capped at
/// 5), so short names need a near-exact match while longer ones tolerate more
/// drift. Returns `None` when nothing is close enough to suggest confidently.
fn suggest_tool_name(name: &str, candidates: &[&str]) -> Option<String> {
    let threshold = (name.chars().count() / 3).clamp(1, 5);
    candidates
        .iter()
        .map(|c| (levenshtein(name, c), *c))
        .filter(|(d, _)| *d <= threshold)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c.to_string())
}

/// Levenshtein edit distance between two strings (insertions, deletions,
/// substitutions), over Unicode scalar values. Used only for short tool names,
/// so the simple two-row dynamic-programming table is more than fast enough.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

pub(in crate::chat::builtin_agent) async fn build_tool_specs(
    available_tools: &[String],
    tool_registry: &ToolRegistryHandle,
    mcp_index: Option<&[ToolIndexSnapshot]>,
    tool_search_limit: usize,
) -> Vec<ToolSpec> {
    let deferred = mcp_index.is_some();
    let mut specs = Vec::with_capacity(available_tools.len());
    for name in available_tools {
        // Deferred MCP tools are advertised from the index below, never from
        // the registry, which holds no `mcp:` descriptor in that mode anyway.
        // Skipping here keeps a stale registry entry from producing a duplicate
        // spec. Native tools are resolved normally in both modes.
        if deferred && name.starts_with("mcp:") {
            continue;
        }
        match tool_registry.get(name).await {
            Ok(Some(descriptor)) => {
                specs.push(ToolSpec {
                    name: descriptor.name,
                    description: descriptor.description,
                    parameters: descriptor.input_schema,
                });
            }
            Ok(None) => {
                info!(tool = %name, "tool.descriptor.missing");
            }
            Err(e) => {
                warn!(tool = %name, error = %e, "tool.descriptor.read.failed");
            }
        }
    }
    if deferred {
        specs.push(tool_search_spec(tool_search_limit));
        // Deferred mode used to stop here, and that made every MCP tool
        // unreachable from chat on the default setting.
        //
        // The runtime was ready: `valid_tool_names` accepts any indexed
        // `mcp:server/tool` and an executor is registered for each. Only the
        // model was never told. It saw `tool_search`, called it, got back
        // `mcp:notes/list_seed_notes`, and then had to emit a call for a name
        // absent from its declared tools. Providers constrain tool calls to the
        // declared set, so it could not: it answered that it had no way to
        // invoke MCP tools while three of them sat indexed and callable behind
        // it.
        //
        // So when the whole index fits inside the search limit, advertise it
        // with the schemas the discovery `tools/list` already brought back.
        // That is the small fixed set where deferring saved little anyway, and
        // it is what a two-server install looks like. Above that bound nothing
        // changes and `tool_search` stays the only path, which is the point of
        // deferring; the schemas the search result carries are what let the
        // model call what it finds.
        if let Some(index) = mcp_index {
            if index.len() <= tool_search_limit {
                for entry in index {
                    specs.push(ToolSpec {
                        name: format!("mcp:{}/{}", entry.server_name, entry.tool_name),
                        description: entry.description.clone().unwrap_or_else(|| {
                            format!(
                                "MCP tool `{}` from server `{}`.",
                                entry.tool_name, entry.server_name
                            )
                        }),
                        parameters: entry
                            .input_schema
                            .clone()
                            .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
                    });
                }
                tracing::info!(
                    indexed = index.len(),
                    limit = tool_search_limit,
                    "mcp.deferred.index_advertised"
                );
            } else {
                tracing::info!(
                    indexed = index.len(),
                    limit = tool_search_limit,
                    "mcp.deferred.index_reachable_through_search_only"
                );
            }
        }
    }
    specs
}

/// Build the synthetic `tool_search` [`ToolSpec`] injected in deferred MCP mode.
///
/// `max_limit` is the configured upper bound for the `limit` argument, surfaced
/// in the description so the model picks a valid value.
fn tool_search_spec(max_limit: usize) -> ToolSpec {
    ToolSpec {
        name: "tool_search".to_string(),
        description: format!(
            "Search the connected MCP tools by intent and return matching tools \
             with their fully qualified `mcp:server/tool` names. Call this before \
             invoking any MCP tool: the returned `full_name` is the exact name to \
             call. Takes an optional `query` substring (empty returns the top \
             results) and an optional `limit` between 1 and {max_limit}."
        ),
        parameters: tool_search_input_schema(),
    }
}

/// Truncate a string to a maximum length, appending "..." if truncated.
pub(in crate::chat::builtin_agent) fn truncate_preview(s: &str) -> String {
    truncate_to(s, PREVIEW_MAX_LEN)
}

/// Next value of the consecutive-tool-failure counter.
///
/// Increments on a failed call and resets to 0 on success, so a run of failures
/// accumulates toward [`ESCALATION_FAILURE_THRESHOLD`] while any success clears it.
pub(in crate::chat::builtin_agent) fn next_failure_count(current: u32, failed: bool) -> u32 {
    if failed {
        current.saturating_add(1)
    } else {
        0
    }
}

/// Truncate tool output for LLM context injection.
///
/// When the raw output exceeds [`TOOL_OUTPUT_MAX_LEN`], this function attempts
/// a smarter strategy: it parses the JSON result, prioritizes user-relevant
/// lines in stdout (lines under the user's home directory), and rebuilds a
/// compact result. Falls back to raw truncation if parsing fails.
pub(in crate::chat::builtin_agent) fn truncate_tool_output(s: &str) -> String {
    if s.len() <= TOOL_OUTPUT_MAX_LEN {
        return s.to_string();
    }

    // Try to parse as the JSON shape returned by bash_executor / file_io
    if let Some(compacted) = compact_json_stdout(s) {
        return compacted;
    }

    // Fallback: raw truncation
    let truncated = truncate_to(s, TOOL_OUTPUT_MAX_LEN);
    format!(
        "{truncated}\n\n[Output truncated - {total} chars total. \
         Refine the command to produce less output.]",
        total = s.len()
    )
}

/// Compact the `stdout` field of a JSON tool result, prioritizing user-space
/// lines. Returns `None` when `s` is not the expected JSON shape.
fn compact_json_stdout(s: &str) -> Option<String> {
    let mut val = serde_json::from_str::<serde_json::Value>(s).ok()?;
    let stdout = val
        .get("stdout")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let lines: Vec<&str> = stdout.lines().collect();
    let total_lines = lines.len();

    // Partition: user-space lines first, then the rest
    let home = apollia_core::paths::home_string().unwrap_or_default();
    let (user_lines, system_lines): (Vec<&str>, Vec<&str>) = if home.is_empty() {
        (lines.clone(), Vec::new())
    } else {
        lines.iter().partition(|l| l.starts_with(&home))
    };

    // Build compact output: user lines have priority, fill remaining budget
    let mut kept = Vec::new();
    let mut budget = TOOL_OUTPUT_MAX_LEN / 2; // reserve half for JSON overhead + notice
    for line in user_lines.iter().chain(system_lines.iter()) {
        if line.len() + 1 > budget {
            break;
        }
        budget -= line.len() + 1;
        kept.push(*line);
    }

    let compact_stdout = kept.join("\n");
    val["stdout"] = serde_json::Value::String(compact_stdout);

    let result = val.to_string();
    if kept.len() < total_lines {
        return Some(format!(
            "{result}\n\n[Output filtered - showing {kept}/{total} lines, \
             user paths prioritized. Refine the command for more precise results.]",
            kept = kept.len(),
            total = total_lines,
        ));
    }
    Some(result)
}

/// Truncate a string to `max_len` characters at a valid UTF-8 boundary.
fn truncate_to(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = s
            .char_indices()
            .take_while(|(i, _)| *i < max_len.saturating_sub(3))
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}...", &s[..boundary])
    }
}
