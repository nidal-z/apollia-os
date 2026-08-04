use super::*;

/// Resolves the sandbox root for a chat session based on its project association.
///
/// Called once per message, inside the async tokio task spawned by `handle_send_message`.
/// Returns a [`NativeChatToolInvoker`] configured with:
/// - the project's `workspace_path` when the session is linked to a project;
/// - `~/.apollia/` for free-chat sessions (no project), if that directory exists;
/// - `None` as a last resort (bash will then inherit the process CWD).
///
/// When `hitl` is provided, HITL filesystem support is enabled on the returned invoker.
pub(in crate::chat::manager) async fn resolve_workspace_for_session(
    params: WorkspaceResolutionParams,
) -> Result<NativeChatToolInvoker, ChatError> {
    let WorkspaceResolutionParams {
        project_id,
        project_repo,
        hitl,
        pending_user_inputs,
        mcp_handle,
        chat_tools_config,
        session_id,
        mcp_loading,
        mcp_index,
        tool_search_limit,
    } = params;
    let session_id = session_id.as_str();
    let default_workspace = chat_tools_config
        .as_ref()
        .and_then(|c| c.default_workspace.clone());
    let workspace_path =
        resolve_workspace_path(&project_id, &project_repo, default_workspace.as_deref()).await?;
    let mut invoker = NativeChatToolInvoker::new_unrestricted(workspace_path.clone());
    if let Some(pending) = pending_user_inputs.clone() {
        invoker = invoker.with_ask_user_support(pending);
    }

    // Convergence point: every tool outside the native hardcoded fast path,
    // MCP, Google connectors, read-only natives, future providers, is
    // resolved through a single `ToolDispatcher`.
    // The dispatcher applies the same permission engine + audit trail as
    // the Agent-mode pipeline. HITL-sensitive natives (file_write/edit,
    // bash, python_executor, notebook_edit) stay in the fast path because
    // their inline approval flow is not yet ported to events.
    let mut extra_executors: Vec<Box<dyn apollia_tools::executor::ToolExecutor>> = Vec::new();

    // MCP executors. Eager: one per tool registered by a connected server.
    // Deferred: the synthetic `tool_search` tool plus one executor per indexed
    // tool, so a tool found via search is invocable without a preloaded schema.
    collect_mcp_executors(
        mcp_handle,
        mcp_loading,
        &mcp_index,
        tool_search_limit,
        &mut extra_executors,
    )
    .await;

    // SaaS connectors: Google + Microsoft 365.
    extra_executors.extend(crate::connectors_bridge::build_google_executors());
    extra_executors.extend(crate::connectors_bridge::build_microsoft_executors());

    // With no project open, the file tools are rooted at the user's home, not at
    // the system temp directory. Temp made the assistant unable to reach anything
    // the user actually owns, while protecting nothing: the trust model already
    // states that an installed agent runs with the user's rights, and the real
    // barrier on this path is the human approval on `file_write` / `file_edit`.
    // It also contradicted the prompt, which tells the model it may read and write
    // anywhere on the machine. Temp remains the last-resort fallback when the home
    // directory cannot be resolved.
    let sandbox_root = workspace_path
        .clone()
        .unwrap_or_else(apollia_core::paths::home_dir_or_temp);

    // When the supervisor handed us a `ChatToolsConfig`, build the full native
    // dispatcher (same factory the Agent-mode + Triggers pipeline uses). This
    // pulls in web_search, web_read, http_fetch, memory_search,
    // permission_rule_* and ask_user with the operator's `apollia.toml` cfg.
    // HITL-sensitive natives (bash, python_executor, file_write/edit,
    // notebook_edit) stay disabled from the dispatcher so the fast path
    // continues to drive their inline approval flow; migrating that flow to
    // EventBus events is tracked separately.
    let dispatcher = if let Some(cfg) = chat_tools_config.as_ref() {
        build_full_chat_dispatcher(FullDispatcherParams {
            cfg,
            session_id,
            sandbox_root: &sandbox_root,
            workspace_path: &workspace_path,
            pending_user_inputs: &pending_user_inputs,
            hitl: hitl.as_ref(),
            extra_executors,
        })
    } else {
        build_fallback_chat_dispatcher(&sandbox_root, extra_executors)
    };
    invoker = invoker.with_fallback_dispatcher(std::sync::Arc::new(
        apollia_tools::dispatcher_invoker::DispatcherToolInvoker::new(std::sync::Arc::new(
            dispatcher,
        )),
    ));
    if let Some(p) = hitl {
        Ok(invoker.with_hitl_support(p))
    } else {
        Ok(invoker)
    }
}

/// Resolve the sandbox root path for a chat session.
async fn resolve_workspace_path(
    project_id: &Option<String>,
    project_repo: &Option<Arc<ProjectRepository>>,
    default_workspace: Option<&std::path::Path>,
) -> Result<Option<std::path::PathBuf>, ChatError> {
    match project_id {
        None => {
            // Free chat: prefer the operator-configured default workspace
            // (`[chat] default_workspace`), then `~/.apollia/`, so bash and file
            // tools never silently inherit the Tauri process CWD.
            if let Some(p) = default_workspace.filter(|p| p.is_dir()) {
                return Ok(Some(p.to_path_buf()));
            }
            Ok(apollia_core::paths::home_dir()
                .map(|h| h.join(".apollia"))
                .filter(|p| p.is_dir()))
        }
        Some(pid) => {
            let repo = project_repo
                .as_ref()
                .ok_or_else(|| ChatError::ProjectNotFound(pid.clone()))?;
            let detail = repo
                .get_project_async(pid.clone())
                .await
                .map_err(|_| ChatError::ProjectNotFound(pid.clone()))?;
            if detail.workspace_path.is_none() {
                warn!(
                    project_id = %pid,
                    "project has no workspace_path configured - falling back to current_dir()"
                );
            }
            Ok(detail.workspace_path.map(std::path::PathBuf::from))
        }
    }
}

/// Register the MCP executors for a chat session.
///
/// In [`LoadingMode::Eager`] this pushes one [`McpToolExecutor`] per tool
/// advertised with a full schema by a connected server (the legacy behavior).
///
/// In [`LoadingMode::Deferred`] it pushes the synthetic `tool_search` executor
/// plus one [`McpToolExecutor`] per indexed tool, so a tool discovered through
/// `tool_search` can be invoked even though its schema was never preloaded. The
/// individual schemas are not sent to the LLM; they are fetched on demand at
/// call time.
///
/// [`McpToolExecutor`]: apollia_mcp::executor::McpToolExecutor
async fn collect_mcp_executors(
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    mcp_loading: LoadingMode,
    mcp_index: &[ToolIndexSnapshot],
    tool_search_limit: usize,
    extra_executors: &mut Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
) {
    let Some(handle) = mcp_handle else {
        return;
    };
    // The resource executors and one executor per connected-server tool are
    // shared with the desktop and CLI agent dispatchers. `build_agent_tool_executors`
    // covers both loading modes (`server_detail` surfaces the deferred index too).
    extra_executors.extend(apollia_mcp::executor::build_agent_tool_executors(&handle).await);

    // In Deferred mode, add the synthetic search tool: it is the single MCP entry
    // point exposed to the LLM prompt, registered even when the index is empty.
    if mcp_loading == LoadingMode::Deferred {
        extra_executors.push(Box::new(ToolSearchExecutor::new(
            mcp_index.to_vec(),
            tool_search_limit,
        )));
    }
}

/// Build the full chat dispatcher (every native flows through the dispatcher,
/// HITL-sensitive natives wrapped, dynamic http_fetch).
fn build_full_chat_dispatcher(
    params: FullDispatcherParams<'_>,
) -> apollia_tools::executor::ToolDispatcher {
    let FullDispatcherParams {
        cfg,
        session_id,
        sandbox_root,
        workspace_path,
        pending_user_inputs,
        hitl,
        mut extra_executors,
    } = params;
    // We disable the dispatcher's default versions of:
    //   - file_write / file_edit / notebook_edit / bash / python:
    //     re-added below wrapped in `HitlFilesystemGuard`.
    //   - http_fetch: re-added as `DynamicAllowlistHttpFetch`.
    //
    // The remaining natives (file_read/list/glob/grep, notebook_read,
    // web_search, web_read, memory_search, ask_user, permission_rule_*)
    // are built untouched by `build_dispatcher_with`.
    const WRAPPED_NATIVES: &[&str] = &[
        "bash_executor",
        "python_executor",
        "file_write",
        "file_edit",
        "notebook_edit",
        "http_fetch",
    ];
    let mut disabled = cfg.tools_config.disabled.clone();
    for name in WRAPPED_NATIVES {
        if !disabled.iter().any(|d| d == name) {
            disabled.push((*name).to_string());
        }
    }

    let native_cfg = apollia_tools::NativeDispatcherConfig {
        sandbox_root: sandbox_root.to_path_buf(),
        agent_id: format!("apollia:chat:{session_id}"),
        venv_base_dir: cfg.data_dir.join("venvs"),
        // Per-session memory namespace so `memory_search` reads/writes
        // the chat session's own slice. Other namespaces could be added
        // here as shared read-only later.
        memory_namespace: Some(format!("apollia:chat:{session_id}")),
        memory_shared_namespaces: Vec::new(),
        memory_base_dir: cfg.data_dir.join("memory"),
        // Native http_fetch uses None, the wrapper below provides
        // dynamic allowlist behaviour preserving Chat Libre UX.
        http_allowlist: None,
        pending_user_inputs: pending_user_inputs.clone(),
        disabled_tools: disabled,
        brave_api_key: cfg.brave_api_key.clone(),
        web_search_config: cfg.tools_config.web_search.clone(),
        web_read_config: cfg.tools_config.web_read.clone(),
        governance_db_path: Some(cfg.data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME)),
    };

    // Wrap the HITL-sensitive natives with the approval-flow guard.
    if let Some(p) = hitl {
        push_hitl_natives(
            &mut extra_executors,
            p,
            workspace_path,
            sandbox_root,
            &native_cfg,
        );
    }

    // Dynamic-allowlist http_fetch (preserves the per-call host
    // injection that the legacy fast path did).
    extra_executors.push(Box::new(
        crate::chat::native_wrappers::DynamicAllowlistHttpFetch::new(),
    ));

    apollia_tools::build_dispatcher_with(&native_cfg, extra_executors)
}

/// Virtualenv shared by every chat session's `python_executor`.
///
/// Deliberately not the session identifier: that one names a memory namespace
/// and would leave one ~15 MB interpreter tree behind per conversation. Chat
/// sessions declare no pip packages, so they have nothing to isolate from each
/// other, and the interpreter is created once on the first execution.
const CHAT_VENV_ID: &str = "apollia-chat";

/// Wrap and append the HITL-sensitive native executors (file write/edit,
/// notebook edit, bash, python) behind the filesystem approval guard.
fn push_hitl_natives(
    extra_executors: &mut Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
    p: &HitlInvokerParams,
    workspace_path: &Option<std::path::PathBuf>,
    sandbox_root: &std::path::Path,
    native_cfg: &apollia_tools::NativeDispatcherConfig,
) {
    let hitl_ctx = crate::chat::native_wrappers::HitlFilesystemContext {
        event_bus: p.event_bus.clone(),
        pending_fs: p.pending_fs.clone(),
        fs_allow_rules: p.fs_allow_rules.clone(),
        session_id: p.session_id.clone(),
        workspace_path: workspace_path.clone(),
        sandbox_root: sandbox_root.to_path_buf(),
        risk_config: p.risk_config.clone(),
    };

    let push_hitl = |execs: &mut Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
                     inner: Box<dyn apollia_tools::executor::ToolExecutor>,
                     op: apollia_tools::FilesystemOp| {
        execs.push(Box::new(
            crate::chat::native_wrappers::HitlFilesystemGuard::new(inner, op, hitl_ctx.clone()),
        ));
    };

    if let Ok(t) = apollia_tools::tools::file_write::FileWrite::new(sandbox_root.to_path_buf()) {
        push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        );
    }
    if let Ok(t) = apollia_tools::tools::file_edit::FileEdit::new(sandbox_root.to_path_buf()) {
        push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        );
    }
    if let Ok(t) =
        apollia_tools::tools::notebook_edit::NotebookEdit::new(sandbox_root.to_path_buf())
    {
        push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        );
    }
    push_hitl(
        extra_executors,
        Box::new(apollia_tools::tools::bash_executor::BashExecutor::new()),
        apollia_tools::FilesystemOp::Write,
    );
    match apollia_tools::tools::python_executor::PythonExecutor::new(
        CHAT_VENV_ID,
        &native_cfg.venv_base_dir,
    ) {
        Ok(t) => push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        ),
        // A host with no Python 3 must hear why. Dropping the executor here
        // left the descriptor advertised to the model and the call answered
        // with UnknownTool, which reads as a wiring bug rather than a missing
        // interpreter. Same stub the dispatcher itself installs.
        Err(e) => extra_executors.push(Box::new(apollia_tools::UnavailableTool::new(
            "python_executor",
            e.to_string(),
        ))),
    }
}

/// Build the fallback dispatcher used when no `ChatToolsConfig` was provided
/// (e.g. tests): connector + MCP + read-only file natives only.
fn build_fallback_chat_dispatcher(
    sandbox_root: &std::path::Path,
    mut extra_executors: Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
) -> apollia_tools::executor::ToolDispatcher {
    if let Ok(t) = apollia_tools::tools::file_read::FileRead::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_list::FileList::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_glob::FileGlob::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_grep::FileGrep::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) =
        apollia_tools::tools::notebook_read::NotebookRead::new(sandbox_root.to_path_buf())
    {
        extra_executors.push(Box::new(t));
    }
    apollia_tools::executor::ToolDispatcher::new(extra_executors)
}

/// Convert a [`ChatSession`] into a lightweight [`SessionInfo`].
pub(in crate::chat::manager) fn session_to_info(session: &ChatSession) -> SessionInfo {
    SessionInfo {
        id: session.id.clone(),
        mode: session.mode.clone(),
        agent_name: session.agent_name.clone(),
        status: session.status.clone(),
        created_at: session.created_at.clone(),
        title: session.title.clone(),
        project_id: session.project_id.clone(),
    }
}

/// Return the current time as an RFC-3339/ISO-8601 string.
pub(in crate::chat::manager) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HITL parameters that approve every filesystem risk level up front, so a
    /// test never waits on an approval nobody will answer.
    fn preapproved_hitl(session_id: &str) -> HitlInvokerParams {
        let (event_bus, _rx) = crate::eventbus::EventBus::new();
        let fs_allow_rules = std::sync::Arc::new(std::sync::Mutex::new(
            ["write:medium", "write:high", "write:critical"]
                .iter()
                .map(|s| (*s).to_string())
                .collect::<std::collections::HashSet<String>>(),
        ));
        HitlInvokerParams {
            session_id: session_id.to_string(),
            event_bus,
            pending_fs: crate::chat::types::PendingFilesystemApprovals::new(),
            fs_allow_rules,
            risk_config: apollia_core::FilesystemRiskConfig::default(),
        }
    }

    fn chat_tools_config(data_dir: &std::path::Path) -> std::sync::Arc<ChatToolsConfig> {
        std::sync::Arc::new(ChatToolsConfig {
            data_dir: data_dir.to_path_buf(),
            brave_api_key: None,
            tools_config: apollia_core::ToolsConfig::default(),
            default_workspace: None,
            tool_turn_temperature: None,
        })
    }

    // GIVEN the dispatcher a chat session really gets, over a data directory
    //       where no virtualenv was ever provisioned
    // WHEN python_executor is dispatched
    // THEN the code runs, and the interpreter was created under the shared chat
    //      virtualenv rather than a per-session one
    #[tokio::test]
    async fn test_chat_dispatcher_runs_python_without_prior_provisioning() {
        // GIVEN
        let data_dir = tempfile::tempdir().expect("tempdir");
        let sandbox = tempfile::tempdir().expect("tempdir");
        let cfg = chat_tools_config(data_dir.path());
        let hitl = preapproved_hitl("session-under-test");
        let dispatcher = build_full_chat_dispatcher(FullDispatcherParams {
            cfg: &cfg,
            session_id: "session-under-test",
            sandbox_root: sandbox.path(),
            workspace_path: &None,
            pending_user_inputs: &None,
            hitl: Some(&hitl),
            extra_executors: Vec::new(),
        });

        // WHEN
        let result = dispatcher
            .dispatch(
                "python_executor",
                serde_json::json!({ "code": "print('wired')", "timeout_secs": 120 }),
            )
            .await;

        // THEN
        let output = match result {
            Ok(v) => v,
            // A host with no Python 3 answers with the actionable stub instead.
            Err(apollia_tools::executor::ToolExecutionError::ExecutionFailed {
                ref code, ..
            }) if code == "tool_unavailable" || code == "python_unavailable" => return,
            Err(e) => panic!("python_executor failed: {e:?}"),
        };
        assert_eq!(
            output.get("stdout").and_then(|v| v.as_str()).map(str::trim),
            Some("wired")
        );
        assert!(
            data_dir
                .path()
                .join("venvs")
                .join(CHAT_VENV_ID)
                .join("venv")
                .is_dir(),
            "the chat virtualenv should have been created on first use"
        );
    }
}
