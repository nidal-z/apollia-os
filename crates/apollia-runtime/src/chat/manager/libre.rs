use super::*;
use tracing::Instrument;

/// The tools the session granted by name, kept by an older binary, which a
/// run counts as granted for the run. Governance decides every call and reads
/// such a grant only for a classified family other than executing code; a
/// code executor is left out here as well, so it never reaches the run as
/// granted.
pub(in crate::chat::manager) fn session_run_grants(
    base: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    base.iter()
        .filter(|tool| !apollia_permissions::is_code_executor(tool))
        .cloned()
        .collect()
}

/// Load the persisted Chat-Libre overrides: the settings of the Apollia agent,
/// beside the injected governance database. `None`, or a database that does
/// not exist yet, yields the empty overrides: this function never resolves a
/// path from the process home directory, so a mount without a governance path
/// reads nothing.
pub(in crate::chat::manager) fn load_chat_libre_overrides(
    governance_db: Option<&std::path::Path>,
) -> ChatLibreOverrides {
    let mut out = ChatLibreOverrides::default();
    let db_path = match governance_db {
        Some(p) => p,
        None => return out,
    };
    if !db_path.exists() {
        return out;
    }

    super::apollia_agent::apply_apollia_agent(&mut out, db_path);

    out
}

/// The defaults a free chat is born with: the model assigned to the Apollia
/// agent, recorded on the session for downstream routing.
///
/// When `is_libre` is `false`, returns the empty defaults. The mission of
/// Apollia is not copied onto the session: each message reads it from the
/// agent ([`apollia_mission`]), and what the session adds to it lives in its
/// settings.
pub(in crate::chat::manager) fn libre_session_defaults(
    is_libre: bool,
    governance_db: Option<&std::path::Path>,
) -> LibreSessionDefaults {
    if !is_libre {
        return LibreSessionDefaults::default();
    }
    LibreSessionDefaults {
        llm_backend: load_chat_libre_overrides(governance_db).llm_backend,
    }
}

/// The mission of the Apollia agent as resolved now, empty when it has none:
/// the run then starts from the built-in prompt of its tier.
pub(in crate::chat::manager) fn apollia_mission(governance_db: Option<&std::path::Path>) -> String {
    load_chat_libre_overrides(governance_db)
        .system_prompt
        .unwrap_or_default()
}

/// Accumulate the metrics of one completed exchange into `entry`.
///
/// `context_window_tokens` is the model's real context window in tokens (`None`
/// when the backend cannot report it) and `context_tokens_used` is the prompt
/// size of the exchange's last LLM call, i.e. the current context occupancy.
/// Both feed the token-based context gauge; a `None` window is stored as `0` so
/// the gauge renders as unknown rather than a misleading fill.
// REASON: each argument is one metric of the exchange summary being accumulated.
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
            warn!(
                error = %e,
                detail = "proceeding without a summary",
                "chat.context.summarization.failed"
            );
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
        agent_id,
        llm_router,
        tool_registry,
        event_bus,
        a2a_for_agent,
        apollia_context,
        pending_approvals,
        autonomy_level,
        runtime_budget,
        history,
        available_tools,
        authorized_tools,
        system_prompt,
        session_settings,
        context_window_size,
        stored_summary,
        llm_for_summarize,
        pending_user_inputs_for_session,
        mcp_handle_for_session,
        mut package_tools,
        composer,
        governance,
        chat_tools_config_for_session,
        mcp_loading,
        tool_search_limit,
        session_id_str,
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
        answer_prefill,
        preferred_backend,
        turn_id,
        turn_images,
    } = params;

    let available_tools = available_tools
        .tools(
            &tool_registry,
            &super::default_access_of(chat_tools_config_for_session.as_deref()),
        )
        .await;
    // The conversation takes accesses out of the agent's list and may propose
    // some the agent does not hold, for itself alone: nothing is written to
    // the agent or to governance.
    let proposed_accesses = session_settings.proposed_beyond(&available_tools);
    let available_tools = session_settings.apply_to_accesses(available_tools);
    // A conversation limited to a list of accesses is offered nothing a
    // source of its agent would add.
    let available_tools = if session_settings.is_limited() {
        available_tools
    } else {
        super::code_context::with_code_context(
            chat_tools_config_for_session.as_deref(),
            &agent_id,
            available_tools,
            &mut package_tools,
        )
        .await
    };

    // In deferred mode, snapshot the aggregated tool index once. The synthetic
    // `tool_search` tool is built from it twice: as a dispatcher executor (so a
    // discovered tool can be invoked) and as an LLM spec (so the agent sees it
    // instead of every MCP schema). Eager mode keeps an empty index.
    let mcp_index: Vec<ToolIndexSnapshot> = match (mcp_loading, &mcp_handle_for_session) {
        (LoadingMode::Deferred, Some(handle)) => handle.get_tool_index().await,
        (LoadingMode::Deferred, None) => {
            warn!(
                mode = "deferred",
                detail = "tool_search exposed over an empty index",
                "mcp.deferred.manager.missing"
            );
            Vec::new()
        }
        (LoadingMode::Eager, _) => Vec::new(),
    };

    // A disabled tool is neither offered nor findable through `tool_search`,
    // MCP and connector tools as much as natives.
    let disabled: Vec<String> = chat_tools_config_for_session
        .as_deref()
        .map(super::dispatcher::effective_disabled_tools)
        .unwrap_or_default();
    // An access to a connector tool without its account names the single
    // connected account, or nothing.
    let available_tools =
        crate::connectors_bridge::resolve_connector_access(&agent_id, available_tools).await;
    // A proposed access is asked about under the name its calls carry.
    let proposed_accesses =
        crate::connectors_bridge::resolve_connector_access(&agent_id, proposed_accesses).await;
    // The folders granted to the agent are the roots of its file tools.
    let (available_tools, folders) = super::dispatcher::granted_folders(
        available_tools,
        chat_tools_config_for_session.as_deref(),
    );
    // The documents attached to this conversation are read by its file tools,
    // and by those of no other conversation.
    let planning_folders = folders.read.clone();
    let folders = super::documents::with_session_documents(
        folders,
        chat_tools_config_for_session
            .as_deref()
            .map(|cfg| cfg.data_dir.as_path()),
        &session_id_str,
    );
    let mcp_index = super::disabled::only_granted(mcp_index, &available_tools);
    let (available_tools, mcp_index) =
        super::disabled::without_disabled(available_tools, mcp_index, &disabled);
    // The source tools are offered only when their source is checked.
    let sources =
        super::sources::source_access(chat_tools_config_for_session.as_deref(), &agent_id).await;
    let available_tools = super::sources::offered(available_tools, &sources);
    let memory_namespace = super::sources::memory_namespace(&agent_id);

    // The request settings of the conversation, above the three levels read
    // beside the data directory: the model, the package, and the preset of
    // the conversation or of the agent.
    let session = crate::chat::builtin_agent::SessionOverrides {
        instructions: session_settings.instructions.clone(),
        request: session_settings.request_level(),
        request_levels: chat_tools_config_for_session.as_deref().map(|cfg| {
            apollia_llm::AgentModelBinding::new(
                agent_id.clone(),
                None,
                Some(apollia_core::paths::DataFile::System.path(&cfg.data_dir)),
            )
            .with_session_preset(session_settings.preset)
        }),
        thinking: session_settings.thinking,
        reasoning_effort: session_settings.reasoning_effort,
        preserve_thinking: session_settings.preserve_thinking,
        overflow: session_settings.overflow(),
        proposed_accesses,
    };

    // Read the tool-turn temperature before the config is moved into the
    // session invoker below; it is applied to the agent further down.
    let tool_turn_temperature = chat_tools_config_for_session
        .as_ref()
        .and_then(|c| c.tool_turn_temperature);

    // The session invoker, its file tools rooted at the granted folders.
    let session_invoker = match build_session_invoker(
        WorkspaceResolutionParams {
            folders,
            pending_user_inputs: Some(pending_user_inputs_for_session),
            mcp_handle: mcp_handle_for_session,
            chat_tools_config: chat_tools_config_for_session,
            session_id: session_id_str,
            mcp_loading,
            mcp_index: mcp_index.clone(),
            tool_search_limit,
            package_tools,
            sources,
            memory_namespace,
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
    // The session runs its agent: Apollia, whose stored prompt is the mission,
    // whose session tools are its granted tools and whose stored backend is
    // its model; or a declarative agent, as resolved when the message is sent.
    // In a conversation the plan of a task is drafted and approved through the
    // plan mode, so the agent never plans first there.
    let spec = AgentRunSpec {
        agent_id,
        mission: system_prompt,
        granted_tools: available_tools,
        model: preferred_backend,
        autonomy: autonomy_level,
        planning: apollia_tools::PlanningMode::Never,
        version: String::new(),
    };
    let executor = AgentExecutor::new(
        spec,
        AgentExecutorDeps {
            llm_router,
            tool_registry,
            tool_invoker: session_invoker.invoker,
            event_bus,
            todo,
            plan,
            plan_cache: None,
        },
        apollia_context,
        SessionWiring {
            planning_folders,
            plan_template: None,
            workspace_path: session_invoker.workspace,
            mcp_index: agent_mcp_index,
            tool_search_limit,
            plan_mode: session_plan_mode,
            plan_phase: session_plan_phase,
            hook_executor,
            pending_injection,
            turn_images,
            tool_turn_temperature,
            governance,
            unattended: false,
            approval_timeout: Some(crate::chat::builtin_agent::CHAT_APPROVAL_TIMEOUT),
            tool_output_dir: crate::chat::builtin_agent::default_tool_output_dir(),
            shared_budget: None,
            composition: composer.map(|composer| Composition { composer, depth: 1 }),
            turn_id,
            session,
        },
        &runtime_budget,
        cancel,
    )
    .continuing(answer_prefill);
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
        executor.run(RunInput {
            session_id: &sid,
            message_id: &mid,
            run_id: &run_id,
            user_message: &user_msg,
            history: &history,
            authorized_tools: &authorized_tools,
            pending_approvals: &pending_approvals,
            summary: summary.as_deref(),
            context_window_size,
        }),
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
pub(in crate::chat::manager) async fn build_session_invoker(
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

/// Trace-log the enriched approval metadata (reject reason, always-accept)
/// without touching the `log_tool_approval` SQL schema.
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
                "chat.tool.rejected"
            );
        }
        ToolDecision::AlwaysAccept => {
            tracing::info!(
                session_id,
                message_id,
                tool_name,
                "chat.tool.always_accept.installed"
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod code_executor_guard_tests {
    use super::*;

    #[test]
    fn session_run_grants_filters_code_executors() {
        // GIVEN session grants holding a code executor and a normal tool
        let mut base = std::collections::HashSet::new();
        base.insert("bash_executor".to_string());
        base.insert("web_read".to_string());
        // WHEN the grants of the run are read from them
        let grants = session_run_grants(&base);
        // THEN the code executor is dropped, the normal tool is kept
        assert!(!grants.contains("bash_executor"));
        assert!(grants.contains("web_read"));
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
            plan_mode: false,
            plan_phase: PlanPhase::Done,
            profile_id: None,
            use_profile: true,
            settings: crate::chat::SessionSettings::default(),
            notes: String::new(),
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
