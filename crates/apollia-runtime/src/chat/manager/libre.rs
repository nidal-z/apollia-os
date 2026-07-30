use super::*;
use tracing::Instrument;

/// Merge the session's authorized tools with the live Chat-Libre overrides
/// (additive: never removes an in-session authorization). Overrides are only
/// applied for [`ChatMode::Libre`]; other modes return the base set as-is.
pub(in crate::chat::manager) fn merge_live_authorized_tools(
    base: &std::collections::HashSet<String>,
    mode: &ChatMode,
) -> std::collections::HashSet<String> {
    let mut authorized_tools = base.clone();
    if *mode == ChatMode::Libre {
        let live = load_chat_libre_overrides();
        for tool in live.pre_authorized_tools {
            authorized_tools.insert(tool);
        }
    }
    // A code executor is never blanket-authorized by name: whatever the source
    // (legacy chat.db entry, live override), it must still go through
    // per-invocation approval. This is the consumption-side backstop for the
    // "always allow bash = blank check" finding.
    authorized_tools.retain(|tool| !apollia_permissions::is_code_executor(tool));
    authorized_tools
}

pub(in crate::chat::manager) fn load_chat_libre_overrides() -> ChatLibreOverrides {
    let mut out = ChatLibreOverrides::default();
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return out,
    };
    let base_dir = std::path::PathBuf::from(home).join(".apollia");
    let db_path = base_dir.join(GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return out;
    }

    apply_chat_libre_config(&mut out, &db_path);
    apply_chat_prefix_allow_rules(&mut out, &db_path);

    out
}

/// Apply the free-chat config (`chat_libre_config`) to the overrides.
/// Silent fallback if the database is absent or unreadable.
fn apply_chat_libre_config(out: &mut ChatLibreOverrides, db_path: &std::path::Path) {
    let Ok(repo) = ChatLibreConfigRepository::open(db_path) else {
        return;
    };
    let Ok(cfg) = repo.load() else {
        return;
    };
    if !cfg.system_prompt.trim().is_empty() {
        out.system_prompt = Some(cfg.system_prompt);
    }
    // chat_libre_config.allowed_tools are the auto-authorized tools (HITL
    // skipped), matching the UX label "tools allowed without confirmation".
    // They go into pre_authorized_tools, not available_tools: the LLM still
    // sees the whole registry, but these tools no longer trigger a popup.
    for tool in cfg.allowed_tools {
        if apollia_permissions::is_code_executor(&tool) {
            warn!(
                tool = %tool,
                "skipping pre-authorization of a code executor from chat config: per-invocation approval required"
            );
            continue;
        }
        out.pre_authorized_tools.insert(tool);
    }
    if let Some(b) = cfg.llm_backend {
        if !b.trim().is_empty() {
            out.llm_backend = Some(b);
        }
    }
}

/// Add to the overrides the tools auto-authorized via `allow` rules scoped to
/// the `apollia:chat` agent. Silent fallback on error.
fn apply_chat_prefix_allow_rules(out: &mut ChatLibreOverrides, db_path: &std::path::Path) {
    let Ok(engine) = PrefixRuleEngine::new(db_path) else {
        return;
    };
    let Ok(rules) = engine.list_rules_for_agent(APOLLIA_CHAT_AGENT_ID) else {
        return;
    };
    for r in rules {
        if matches!(r.action, RuleAction::Allow) {
            if apollia_permissions::is_code_executor(&r.tool_name) {
                warn!(
                    tool = %r.tool_name,
                    "ignoring persisted blanket allow-rule for a code executor: per-invocation approval required"
                );
                continue;
            }
            out.pre_authorized_tools.insert(r.tool_name);
        }
    }

    // Global allow-rules (e.g. authorized during onboarding, persisted with
    // scope='global' and no agent_id) are honored in libre chat too, not just
    // agent-scoped ones. Without this pass such tools would keep triggering a
    // HITL prompt every turn. Same safety filters as above: only name-only
    // allow-rules, never code executors (they always require per-invocation
    // approval), and arg-prefix rules are skipped because the name-only
    // HashSet cannot represent them.
    let Ok(global_rules) =
        engine.list_rules_filtered(Some(apollia_permissions::PermissionScope::Global), None)
    else {
        return;
    };
    for r in global_rules {
        if matches!(r.action, RuleAction::Allow) && r.arg_prefix.is_none() {
            if apollia_permissions::is_code_executor(&r.tool_name) {
                warn!(
                    tool = %r.tool_name,
                    "ignoring persisted global allow-rule for a code executor: per-invocation approval required"
                );
                continue;
            }
            out.pre_authorized_tools.insert(r.tool_name);
        }
    }
}

/// Apply Libre-mode governance overrides to the session prompt and return the
/// derived session defaults.
///
/// When `is_libre` is `false`, leaves `prompt` untouched and returns the empty
/// defaults (legacy behavior).
///
/// - `system_prompt` : prepended to the caller's prompt when set.
/// - `llm_backend`   : recorded on the session for downstream routing.
/// - `pre_authorized`: agent-scoped allow rules seed authorized_tools so the
///   chat ReAct loop skips HITL for them.
pub(in crate::chat::manager) fn apply_libre_overrides(
    is_libre: bool,
    prompt: &mut String,
) -> LibreSessionDefaults {
    if !is_libre {
        return LibreSessionDefaults::default();
    }
    let overrides = load_chat_libre_overrides();
    if let Some(sp) = overrides.system_prompt {
        if prompt.trim().is_empty() {
            *prompt = sp;
        } else {
            *prompt = format!("{sp}\n\n{prompt}");
        }
    }
    LibreSessionDefaults {
        llm_backend: overrides.llm_backend,
        pre_authorized: overrides.pre_authorized_tools,
    }
}

/// Accumulate the metrics of one completed exchange into `entry`.
///
/// `context_window_tokens` is the model's real context window in tokens (`None`
/// when the backend cannot report it) and `context_tokens_used` is the prompt
/// size of the exchange's last LLM call, i.e. the current context occupancy.
/// Both feed the token-based context gauge; a `None` window is stored as `0` so
/// the gauge renders as unknown rather than a misleading fill.
#[allow(clippy::too_many_arguments)]
pub(in crate::chat::manager) fn accumulate_exchange_metrics(
    entry: &mut SessionMetrics,
    tokens_used: &apollia_llm::types::TokenUsage,
    session: &ChatSession,
    max_steps: u32,
    context_window_tokens: Option<u32>,
    context_tokens_used: u32,
) {
    let now_ts = now_rfc3339();
    if entry.started_at.is_none() {
        entry.started_at = Some(now_ts.clone());
    }
    entry.updated_at = Some(now_ts);
    entry.prompt_tokens = entry
        .prompt_tokens
        .saturating_add(tokens_used.prompt_tokens);
    entry.completion_tokens = entry
        .completion_tokens
        .saturating_add(tokens_used.completion_tokens);
    entry.cache_read_input_tokens = entry
        .cache_read_input_tokens
        .saturating_add(tokens_used.cache_read_input_tokens);
    entry.cache_write_input_tokens = entry
        .cache_write_input_tokens
        .saturating_add(tokens_used.cache_write_input_tokens);
    entry.cost_usd = match (entry.cost_usd, tokens_used.cost_usd) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    entry.budget_max_steps = max_steps;
    entry.steps_used = entry.steps_used.saturating_add(1);
    // Token-based context gauge: real model window vs current occupancy. A
    // `None` window (unknown) is stored as `0` so the UI shows an unknown gauge
    // rather than the old message-count-over-20 saturation.
    entry.context_window_tokens = context_window_tokens.unwrap_or(0);
    entry.context_tokens_used = context_tokens_used;
    entry.exchanges_count = entry.exchanges_count.saturating_add(1);
    entry.record_tool_calls(
        &session
            .history
            .last()
            .and_then(|m| m.tool_calls.clone())
            .unwrap_or_default(),
    );
    if entry.llm_backend.is_none() {
        entry.llm_backend = session.llm_backend.clone();
    }
}

/// Enrich `system_prompt` with the project context block when injection is
/// enabled and both a project id and a provider are available.
async fn maybe_inject_project_context(
    system_prompt: String,
    should_inject: bool,
    session_project_id: &Option<String>,
    project_ctx: &Option<Arc<dyn ProjectContextProvider>>,
) -> String {
    if !should_inject {
        return system_prompt;
    }
    let (Some(pid), Some(provider)) = (session_project_id, project_ctx) else {
        return system_prompt;
    };
    match provider.build_context(pid).await {
        Some(ctx) => {
            let mut enriched = system_prompt;
            enriched.push_str("\n\n");
            enriched.push_str(&ctx);
            enriched
        }
        None => system_prompt,
    }
}

/// Resolve the conversation summary for the exchange.
///
/// Reuses `stored_summary` when present or the history fits the context
/// window. Otherwise summarizes the overflow via the LLM and persists the
/// result (best-effort) through `tx`.
async fn resolve_exchange_summary(params: ExchangeSummaryParams<'_>) -> Option<String> {
    let ExchangeSummaryParams {
        history,
        context_window_size,
        stored_summary,
        llm_for_summarize,
        tx,
        sid,
    } = params;
    if history.len() <= context_window_size || stored_summary.is_some() {
        return stored_summary;
    }
    let Some(llm) = llm_for_summarize else {
        return None;
    };
    let older = &history[..history.len() - context_window_size];
    match super::super::summarizer::summarize(older, llm).await {
        Ok(s) => {
            let _ = tx
                .send(ChatCommand::PersistSummary {
                    session_id: sid.to_string(),
                    summary: s.clone(),
                })
                .await;
            Some(s)
        }
        Err(e) => {
            warn!(error = %e, "Context window summarization failed, proceeding without summary");
            None
        }
    }
}

/// Run a spawned Libre/Companion exchange end-to-end and report the result back
/// to the actor via `tx`.
///
/// Extracted as a free `async fn` (rather than an inline `async move` block) so
/// it captures only owned `Send` data and keeps `dispatch_libre_exchange`
/// linear.
pub(in crate::chat::manager) async fn run_libre_exchange(params: LibreExchangeParams) {
    let LibreExchangeParams {
        llm_router,
        tool_registry,
        event_bus,
        a2a_for_agent,
        session_user_memory,
        pending_approvals,
        budget,
        autonomy_level,
        level_config,
        verification,
        critic,
        history,
        available_tools,
        authorized_tools,
        system_prompt,
        inject_project_context,
        is_companion,
        context_window_size,
        stored_summary,
        llm_for_summarize,
        project_ctx,
        session_project_id,
        project_repo_for_session,
        pending_user_inputs_for_session,
        mcp_handle_for_session,
        chat_tools_config_for_session,
        mcp_loading,
        tool_search_limit,
        session_id_str,
        hitl_params,
        sid,
        mid,
        run_id,
        user_msg,
        tx,
        todo,
        plan,
        session_plan_mode,
        session_plan_phase,
        hook_executor,
        cancel,
        pending_injection,
    } = params;

    // In deferred mode, snapshot the aggregated tool index once. The synthetic
    // `tool_search` tool is built from it twice: as a dispatcher executor (so a
    // discovered tool can be invoked) and as an LLM spec (so the agent sees it
    // instead of every MCP schema). Eager mode keeps an empty index.
    let mcp_index: Vec<ToolIndexSnapshot> = match (mcp_loading, &mcp_handle_for_session) {
        (LoadingMode::Deferred, Some(handle)) => handle.get_tool_index().await,
        (LoadingMode::Deferred, None) => {
            warn!(
                mode = "deferred",
                "MCP deferred mode active but no manager handle is configured; \
                 tool_search will be exposed over an empty index"
            );
            Vec::new()
        }
        (LoadingMode::Eager, _) => Vec::new(),
    };

    // Read the tool-turn temperature before the config is moved into the
    // session invoker below; it is applied to the agent further down.
    let tool_turn_temperature = chat_tools_config_for_session
        .as_ref()
        .and_then(|c| c.tool_turn_temperature);

    // Resolve per-session sandbox root from project workspace_path.
    // On error (project not found) surface as ExchangeError, no panic.
    let session_invoker = match build_session_invoker(
        WorkspaceResolutionParams {
            project_id: session_project_id.clone(),
            project_repo: project_repo_for_session,
            hitl: Some(hitl_params),
            pending_user_inputs: Some(pending_user_inputs_for_session),
            mcp_handle: mcp_handle_for_session,
            chat_tools_config: chat_tools_config_for_session,
            session_id: session_id_str,
            mcp_loading,
            mcp_index: mcp_index.clone(),
            tool_search_limit,
        },
        &a2a_for_agent,
        hook_executor.clone(),
    )
    .await
    {
        Ok(inv) => inv,
        Err(e) => {
            let _ = tx
                .send(ChatCommand::ExchangeError {
                    session_id: sid,
                    message_id: mid,
                    error: e.to_string(),
                })
                .await;
            return;
        }
    };

    // `Some` only in deferred mode, so `build_tool_specs` injects `tool_search`.
    // Eager keeps `None` and the legacy spec path.
    let agent_mcp_index = match mcp_loading {
        LoadingMode::Deferred => Some(mcp_index),
        LoadingMode::Eager => None,
    };
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router,
        tool_registry,
        tool_invoker: session_invoker.invoker,
        event_bus,
        user_memory: session_user_memory,
        a2a_invoker: a2a_for_agent,
        todo,
        plan,
    })
    .with_workspace_path(session_invoker.workspace)
    .with_mcp_index(agent_mcp_index, tool_search_limit)
    .with_plan_mode(session_plan_mode)
    .with_plan_phase_start(session_plan_phase)
    .with_hook_executor(hook_executor)
    .with_pending_injection(pending_injection)
    .with_tool_turn_temperature(tool_turn_temperature);

    // Inject project context on first message OR right after the
    // session was linked to a project (consumed flag).
    tracing::info!(
        session_id = %sid,
        inject_project_context,
        has_project = session_project_id.is_some(),
        has_provider = project_ctx.is_some(),
        "Chat send: project-context gate"
    );
    let system_prompt = maybe_inject_project_context(
        system_prompt,
        inject_project_context && !is_companion,
        &session_project_id,
        &project_ctx,
    )
    .await;
    let summary = resolve_exchange_summary(ExchangeSummaryParams {
        history: &history,
        context_window_size,
        stored_summary,
        llm_for_summarize: &llm_for_summarize,
        tx: &tx,
        sid: &sid,
    })
    .await;

    // One user-visible turn. The span groups every completion and tool call it
    // contains; the recorder decomposes its wall-clock once it completes.
    let turn_span = tracing::info_span!("chat.react.turn", session_id = %sid, run_id = %run_id);
    let result = crate::perf_trace::instrument_turn(
        &sid,
        agent.execute(
            &sid,
            &mid,
            &run_id,
            &user_msg,
            &history,
            &system_prompt,
            &available_tools,
            &authorized_tools,
            &pending_approvals,
            &budget,
            summary.as_deref(),
            context_window_size,
            Some(&autonomy_level),
            verification.as_ref(),
            critic.as_ref(),
            Some(&level_config),
            cancel,
        ),
    )
    .instrument(turn_span)
    .await;

    let cmd = match result {
        Ok(response) => ChatCommand::ExchangeComplete {
            session_id: sid,
            message_id: mid,
            response,
        },
        Err(err) => ChatCommand::ExchangeError {
            session_id: sid,
            message_id: mid,
            error: err.to_string(),
        },
    };
    let _ = tx.send(cmd).await;
}

/// Resolve the native invoker for a session and wrap it in a composite invoker
/// when an A2A invoker is configured.
async fn build_session_invoker(
    workspace_params: WorkspaceResolutionParams,
    a2a_for_agent: &Option<Arc<A2AInvoker>>,
    hook_executor: Option<Arc<HookExecutor>>,
) -> Result<ResolvedSessionInvoker, ChatError> {
    let session_id = workspace_params.session_id.to_string();
    let native_invoker = resolve_workspace_for_session(workspace_params).await?;
    let workspace = native_invoker.workspace_path().map(|p| p.to_path_buf());
    let invoker: Arc<dyn ToolInvoker> = if let Some(a2a) = a2a_for_agent {
        Arc::new(CompositeToolInvoker::with_hooks(
            native_invoker,
            a2a.clone(),
            hook_executor,
            session_id,
        ))
    } else {
        Arc::new(native_invoker)
    };
    Ok(ResolvedSessionInvoker { invoker, workspace })
}

/// Trace-log the enriched approval metadata (reject reason / always-accept
/// scope) without touching the `log_tool_approval` SQL schema.
pub(in crate::chat::manager) fn log_resolution_metadata(
    decision: &ToolDecision,
    session_id: &str,
    message_id: &str,
    tool_name: &str,
) {
    match decision {
        ToolDecision::Refuse { reason: Some(r) } => {
            tracing::info!(
                session_id,
                message_id,
                tool_name,
                reject_reason = %r,
                "chat tool rejected with reason"
            );
        }
        ToolDecision::AlwaysAccept { scope } => {
            tracing::info!(
                session_id,
                message_id,
                tool_name,
                always_accept_scope = ?scope,
                "chat tool always-accept rule installed"
            );
        }
        _ => {}
    }
}

/// Persist a scoped `allow` rule in `governance.db` for `tool_name`.
///
/// Driven by the chat "Always allow" button according to the scope the
/// operator picked. Best-effort: logs and continues on failure (the in-memory
/// authorization in `session.authorized_tools` stays in place).
pub(in crate::chat::manager) fn persist_chat_allow_rule(
    scope: apollia_permissions::PermissionScope,
    project_path: Option<std::path::PathBuf>,
    agent_id: Option<String>,
    tool_name: &str,
) {
    // A code executor (bash/python) is never blanket-authorized: persisting an
    // arg-prefix-less allow rule would grant a permanent blank check over the
    // whole interpreter. Refuse it; each invocation keeps its per-call approval.
    if apollia_permissions::is_code_executor(tool_name) {
        warn!(
            tool = %tool_name,
            "refusing to persist a blanket allow-rule for a code executor; each invocation requires approval"
        );
        return;
    }

    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(e) => {
            warn!(error = %e, "HOME not set; skipping governance.db rule persistence");
            return;
        }
    };
    let base_dir = std::path::PathBuf::from(home).join(".apollia");

    if let Err(e) = apollia_tools::governance_db::GovernanceDb::open(&base_dir) {
        warn!(error = %e, "failed to open governance.db for chat rule persistence");
        return;
    }
    let db_path = base_dir.join(GOVERNANCE_DB_FILENAME);

    let mut engine = match PrefixRuleEngine::new(&db_path) {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, "failed to open prefix rule engine for chat rule persistence");
            return;
        }
    };

    let rule = apollia_permissions::PrefixRule {
        tool_name: tool_name.to_string(),
        arg_prefix: None,
        action: RuleAction::Allow,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        scope,
        project_path,
        agent_id,
        ..apollia_permissions::PrefixRule::default()
    };

    match engine.add_rule(&rule) {
        Ok(rule_id) => info!(
            rule_id,
            scope = %scope.as_str(),
            tool = %tool_name,
            "persisted scoped allow rule from chat AlwaysAccept"
        ),
        Err(e) => warn!(error = %e, "failed to persist scoped allow rule"),
    }
}

#[cfg(test)]
mod code_executor_guard_tests {
    use super::*;
    use apollia_permissions::{PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction};

    #[test]
    fn merge_live_authorized_tools_filters_code_executors() {
        // GIVEN a base authorization set holding a code executor and a normal tool
        let mut base = std::collections::HashSet::new();
        base.insert("bash_executor".to_string());
        base.insert("web_read".to_string());
        // WHEN the effective set is assembled (non-Libre mode: no HOME lookup)
        let merged = merge_live_authorized_tools(&base, &ChatMode::Agent);
        // THEN the code executor is dropped, the normal tool is kept
        assert!(!merged.contains("bash_executor"));
        assert!(merged.contains("web_read"));
    }

    #[test]
    fn chat_prefix_allow_seeding_excludes_code_executors() {
        // GIVEN a governance.db with agent-scoped Allow rules for a code
        // executor and a normal tool
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("governance.db");
        {
            let mut engine = PrefixRuleEngine::new(&db_path).expect("engine");
            for tool in ["bash_executor", "web_read"] {
                engine
                    .add_rule(&PrefixRule {
                        tool_name: tool.to_string(),
                        arg_prefix: None,
                        action: RuleAction::Allow,
                        scope: PermissionScope::Agent,
                        agent_id: Some(APOLLIA_CHAT_AGENT_ID.to_string()),
                        ..PrefixRule::default()
                    })
                    .expect("add_rule");
            }
        }
        // WHEN the chat pre-authorization seeding runs
        let mut out = ChatLibreOverrides::default();
        apply_chat_prefix_allow_rules(&mut out, &db_path);
        // THEN the code executor is not pre-authorized, the normal tool is
        assert!(!out.pre_authorized_tools.contains("bash_executor"));
        assert!(out.pre_authorized_tools.contains("web_read"));
    }
}

#[cfg(test)]
mod context_gauge_tests {
    use super::*;
    use apollia_llm::types::TokenUsage;

    /// Build a minimal session with `n` empty user messages in history.
    fn session_with_history(n: usize) -> ChatSession {
        let history = (0..n)
            .map(|i| ChatMessage {
                id: format!("m{i}"),
                role: ChatRole::User,
                content: String::new(),
                tool_calls: None,
                tool_name: None,
                created_at: "2026-03-20T10:00:00Z".into(),
                seq: i as u32,
                metadata: None,
            })
            .collect();
        ChatSession {
            id: "sess-gauge".into(),
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: String::new(),
            status: SessionStatus::Active,
            history,
            authorized_tools: std::collections::HashSet::new(),
            available_tools: vec![],
            created_at: "2026-03-20T10:00:00Z".into(),
            active_exchange: None,
            llm_backend: None,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id: None,
            force_project_context_inject: false,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            plan_mode: false,
            plan_phase: PlanPhase::Done,
        }
    }

    #[test]
    fn context_gauge_uses_token_window_and_occupancy() {
        // GIVEN a fresh metrics entry and a session with 40 messages in history
        let mut entry = SessionMetrics::new("sess-gauge");
        let session = session_with_history(40);
        let usage = TokenUsage::default();

        // WHEN an exchange completes reporting a 4096-token window and 1024
        // tokens of current occupancy
        accumulate_exchange_metrics(&mut entry, &usage, &session, 12, Some(4096), 1024);

        // THEN the gauge fields carry token units (not the 40-vs-20 message count):
        // a 25% fill, never saturated at 100%
        assert_eq!(entry.context_window_tokens, 4096);
        assert_eq!(entry.context_tokens_used, 1024);
    }

    #[test]
    fn context_gauge_unknown_window_stored_as_zero() {
        // GIVEN a metrics entry and a session whose backend reports no window
        let mut entry = SessionMetrics::new("sess-gauge");
        let session = session_with_history(3);
        let usage = TokenUsage::default();

        // WHEN an exchange completes with an unknown context window
        accumulate_exchange_metrics(&mut entry, &usage, &session, 12, None, 512);

        // THEN the window is stored as 0 so the UI renders an unknown gauge
        // (pct guard) rather than a misleading fill
        assert_eq!(entry.context_window_tokens, 0);
        assert_eq!(entry.context_tokens_used, 512);
    }
}
