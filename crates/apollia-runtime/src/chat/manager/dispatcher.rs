use super::*;

/// Builds the tool invoker of one message of a chat session, or of one agent
/// run outside a conversation.
///
/// The file tools reach the folders granted to the agent and nothing else
/// (`params.folders`): reading tools the union of them, writing tools those
/// granted for writing. The first folder granted, by the date it was added,
/// anchors a relative path. Neither the data directory, the home folder nor
/// the volume is a root. An agent granted no folder has
/// file tools that refuse with a message saying how to grant one.
///
/// Consent for each call is decided by the agent loop, by action family, before
/// the invoker is reached; nothing here asks the user anything.
pub(in crate::chat::manager) async fn resolve_workspace_for_session(
    params: WorkspaceResolutionParams,
) -> Result<NativeChatToolInvoker, ChatError> {
    let WorkspaceResolutionParams {
        folders,
        pending_user_inputs,
        mcp_handle,
        chat_tools_config,
        session_id,
        mcp_loading,
        mcp_index,
        tool_search_limit,
        package_tools,
        sources,
        memory_namespace,
    } = params;
    let session_id = session_id.as_str();
    let anchor = folders.anchor().map(std::path::Path::to_path_buf);
    let mut invoker = NativeChatToolInvoker::new_with_workspace(anchor);
    if let Some(pending) = pending_user_inputs.clone() {
        invoker = invoker.with_ask_user_support(pending, session_id);
    }

    // Convergence point: every tool outside the native hardcoded fast path,
    // MCP, Google connectors, read-only natives, future providers, is
    // resolved through a single `ToolDispatcher`.
    // The dispatcher applies the same governed path and audit trail as
    // the Agent-mode pipeline.
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

    // SaaS connectors: Google + Microsoft 365, and mailboxes over IMAP.
    extra_executors.extend(crate::connectors_bridge::build_connector_executors().await);
    extra_executors.extend(package_tools);

    // When the supervisor handed us a `ChatToolsConfig`, build the full native
    // dispatcher (same factory the Agent-mode + Triggers pipeline uses). This
    // pulls in web_search, web_read, http_fetch, memory_search and ask_user
    // with the operator's `apollia.toml` cfg.
    let dispatcher = if let Some(cfg) = chat_tools_config.as_ref() {
        build_full_chat_dispatcher(FullDispatcherParams {
            cfg,
            session_id,
            folders: &folders,
            pending_user_inputs: &pending_user_inputs,
            extra_executors,
            sources,
            memory_namespace,
        })
    } else {
        build_fallback_chat_dispatcher(&folders.read, extra_executors)
    };
    invoker = invoker.with_fallback_dispatcher(std::sync::Arc::new(
        apollia_tools::dispatcher_invoker::DispatcherToolInvoker::new(std::sync::Arc::new(
            dispatcher,
        )),
    ));
    Ok(invoker)
}

/// The access an executor checks and the folders the file tools reach, from
/// the granted `tools` and the folders stored beside `cfg`'s data directory,
/// a folder whose connection is in pause left out. Without a configuration
/// no folder is reachable.
pub(in crate::chat::manager) fn granted_folders(
    tools: Vec<String>,
    cfg: Option<&ChatToolsConfig>,
) -> (Vec<String>, apollia_tools::FolderRoots) {
    match cfg {
        Some(cfg) => apollia_tools::granted_folders(
            tools,
            &cfg.data_dir,
            &super::sources::paused_connections(cfg),
        ),
        None => apollia_tools::resolve_folder_access(tools, &[], &Default::default()),
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
/// the writing and code natives re-added with their own roots, dynamic
/// http_fetch).
fn build_full_chat_dispatcher(
    params: FullDispatcherParams<'_>,
) -> apollia_tools::executor::ToolDispatcher {
    let FullDispatcherParams {
        cfg,
        session_id,
        folders,
        pending_user_inputs,
        mut extra_executors,
        sources,
        memory_namespace,
    } = params;
    // We disable the dispatcher's default versions of:
    //   - file_write / file_edit / notebook_edit / bash / python:
    //     re-added below, rooted at the session's anchor.
    //   - http_fetch: re-added as `DynamicAllowlistHttpFetch`.
    //
    // The remaining natives (file_read/list/glob/grep, notebook_read,
    // web_search, web_read, memory_search, ask_user) are built untouched by `build_dispatcher_with`.
    const WRAPPED_NATIVES: &[&str] = &[
        "bash_executor",
        "python_executor",
        "file_write",
        "file_edit",
        "notebook_edit",
        "http_fetch",
    ];
    // `[tools] disabled` in `apollia.toml` is only half the contract: a tool
    // switched off at runtime (`apollia-os tools disable`, the desktop tool
    // page) lands in `governance.db`, and the agent-mode runners already read
    // the union of the two. Reading only the static list here left a
    // governance-disabled tool live on the conversation path.
    let effective_disabled = effective_disabled_tools(cfg);
    let mut disabled = effective_disabled.clone();
    for name in WRAPPED_NATIVES {
        if !disabled.iter().any(|d| d == name) {
            disabled.push((*name).to_string());
        }
    }

    let native_cfg = apollia_tools::NativeDispatcherConfig {
        sandbox_roots: folders.read.clone(),
        write_roots: folders.write.clone(),
        agent_id: format!("apollia:chat:{session_id}"),
        venv_base_dir: cfg.data_dir.join("venvs"),
        // The memory of the agent of the run; `memory_search` reads it only
        // when that memory is checked for the agent (`sources`).
        memory_namespace: Some(memory_namespace),
        memory_shared_namespaces: Vec::new(),
        memory_base_dir: cfg.data_dir.join("memory"),
        // Native http_fetch uses None, the wrapper below provides
        // dynamic allowlist behaviour preserving Chat Libre UX.
        http_allowlist: None,
        pending_user_inputs: pending_user_inputs.clone(),
        session_id: Some(session_id.to_string()),
        disabled_tools: disabled,
        brave_api_key: cfg.brave_api_key.clone(),
        web_search_config: cfg.tools_config.web_search.clone(),
        web_read_config: cfg.tools_config.web_read.clone(),
        python_interpreter: cfg.tools_config.python_interpreter.clone(),
        max_output_chars: cfg.tools_config.max_output_chars,
        sources,
    };

    // Re-add the writing and code natives. Their consent is the family
    // decision of the loop, on the same card as every other tool; no second
    // prompt sits in front of them. The disabled set is handed down because
    // these executors are re-added after `build_dispatcher_with` was told to
    // drop them: without the filter, a disabled `bash_executor` came straight
    // back through this door.
    push_writing_natives(
        &mut extra_executors,
        WritingNativesParams {
            folders,
            venv_base_dir: &native_cfg.venv_base_dir,
            disabled: &effective_disabled,
        },
    );

    // Dynamic-allowlist http_fetch (preserves the per-call host
    // injection that the legacy fast path did). Same re-add caveat as above.
    if !effective_disabled.iter().any(|d| d == "http_fetch") {
        extra_executors.push(Box::new(
            crate::chat::native_wrappers::DynamicAllowlistHttpFetch::new(),
        ));
    }

    apollia_tools::build_dispatcher_with(&native_cfg, extra_executors)
}

/// Union of the operator's static `[tools] disabled` list with the tools
/// carrying `enabled = FALSE` in `governance.db`.
///
/// Either source deactivates the tool, which is the contract
/// `ToolsConfig::disabled` states and the contract the agent-mode runners
/// already honour. The state is read here rather than at boot so a tool
/// disabled mid-session takes effect on the next message, without a restart.
/// A governance database that stops being readable keeps the tools it last
/// disabled switched off, and the decision of every sensitive call refuses
/// it: an unreadable database never re-enables a tool.
pub(in crate::chat::manager) fn effective_disabled_tools(cfg: &ChatToolsConfig) -> Vec<String> {
    crate::governance::tool_gate(
        cfg.governance.as_ref(),
        &cfg.data_dir,
        &cfg.tools_config.disabled,
    )
    .disabled_tools
}

/// Virtualenv shared by every chat session's `python_executor`.
///
/// Deliberately not the session identifier: that one names a memory namespace
/// and would leave one ~15 MB interpreter tree behind per conversation. Chat
/// sessions declare no pip packages, so they have nothing to isolate from each
/// other, and the interpreter is created once on the first execution.
const CHAT_VENV_ID: &str = "apollia-chat";

/// Inputs of [`push_writing_natives`], grouped so the function stays inside
/// the workspace's argument budget.
struct WritingNativesParams<'a> {
    /// The folders granted to the agent: the writing tools reach those
    /// granted for writing.
    folders: &'a apollia_tools::FolderRoots,
    venv_base_dir: &'a std::path::Path,
    /// Effective disabled set (operator plus governance): a native named here
    /// is not re-added at all.
    disabled: &'a [String],
}

/// Append the natives that write files or run code (file write/edit, notebook
/// edit, bash, python).
fn push_writing_natives(
    extra_executors: &mut Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
    params: WritingNativesParams<'_>,
) {
    let WritingNativesParams {
        folders,
        venv_base_dir,
        disabled,
    } = params;
    let is_active = |name: &str| !disabled.iter().any(|d| d == name);

    // The writing tools reach the folders granted for writing; with none,
    // they refuse every call with a message saying how to grant one.
    for name in apollia_tools::folder_access::FOLDER_WRITE_TOOLS {
        if !is_active(name) {
            continue;
        }
        if folders.write.is_empty() {
            extra_executors.push(Box::new(apollia_tools::NoFolderGranted::new(name)));
        } else if let Some(tool) = writing_tool(name, folders.write.clone()) {
            extra_executors.push(tool);
        }
    }
    if is_active("bash_executor") {
        extra_executors.push(Box::new(
            apollia_tools::tools::bash_executor::BashExecutor::new(),
        ));
    }
    if is_active("python_executor") {
        match apollia_tools::tools::python_executor::PythonExecutor::new(
            CHAT_VENV_ID,
            venv_base_dir,
        ) {
            Ok(t) => extra_executors.push(Box::new(t)),
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
}

/// The writing file tool `name`, rooted at `roots`; `None` when it cannot be
/// built.
fn writing_tool(
    name: &str,
    roots: Vec<std::path::PathBuf>,
) -> Option<Box<dyn apollia_tools::executor::ToolExecutor>> {
    use apollia_tools::executor::ToolExecutor;
    use apollia_tools::tools::{file_edit::FileEdit, file_write::FileWrite};
    let tool: Box<dyn ToolExecutor> = match name {
        "file_write" => Box::new(FileWrite::new(roots).ok()?),
        "file_edit" => Box::new(FileEdit::new(roots).ok()?),
        "notebook_edit" => Box::new(apollia_tools::NotebookEdit::new(roots).ok()?),
        _ => return None,
    };
    Some(tool)
}

/// Build the fallback dispatcher used when no `ChatToolsConfig` was provided
/// (e.g. tests): connector + MCP + read-only file natives only.
fn build_fallback_chat_dispatcher(
    sandbox_roots: &[std::path::PathBuf],
    mut extra_executors: Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
) -> apollia_tools::executor::ToolDispatcher {
    if let Ok(t) = apollia_tools::tools::file_read::FileRead::new(sandbox_roots.to_vec()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_list::FileList::new(sandbox_roots.to_vec()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_glob::FileGlob::new(sandbox_roots.to_vec()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_grep::FileGrep::new(sandbox_roots.to_vec()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::notebook_read::NotebookRead::new(sandbox_roots.to_vec()) {
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
        message_count: u32::try_from(session.history.len()).unwrap_or(u32::MAX),
        output_tokens: crate::chat::generation::output_tokens_of(&session.history),
    }
}

/// Return the current time as an RFC-3339/ISO-8601 string.
pub(in crate::chat::manager) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns `true` if the platform can actually execute code through the
    /// python executor's sandbox. On Linux without `CAP_SYS_ADMIN` (e.g.
    /// GitHub Actions runners), `unshare --pid --mount` fails with EPERM
    /// inside the spawned child, so the dispatch reports the sandbox failure
    /// as the program's own output: tests asserting on that output must be
    /// skipped gracefully. Mirrors `can_run_shell` in `bash_executor`.
    fn can_run_sandbox() -> bool {
        #[cfg(target_os = "linux")]
        {
            apollia_core::SecurityPosture::detect().unshare_available
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

    /// A folder granted to read and to write.
    fn folders_at(path: &std::path::Path) -> apollia_tools::FolderRoots {
        apollia_tools::FolderRoots {
            read: vec![path.to_path_buf()],
            write: vec![path.to_path_buf()],
        }
    }

    fn chat_tools_config(data_dir: &std::path::Path) -> std::sync::Arc<ChatToolsConfig> {
        std::sync::Arc::new(ChatToolsConfig {
            data_dir: data_dir.to_path_buf(),
            brave_api_key: None,
            tools_config: apollia_core::ToolsConfig::default(),
            tool_turn_temperature: None,
            source_registry: None,
            governance: None,
        })
    }

    // GIVEN the dispatcher a chat session really gets, over a data directory
    //       where no virtualenv was ever provisioned
    // WHEN python_executor is dispatched
    // THEN the code runs, and the interpreter was created under the shared chat
    //      virtualenv rather than a per-session one
    #[tokio::test]
    async fn test_chat_dispatcher_runs_python_without_prior_provisioning() {
        if !can_run_sandbox() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let data_dir = tempfile::tempdir().expect("tempdir");
        let sandbox = tempfile::tempdir().expect("tempdir");
        let cfg = chat_tools_config(data_dir.path());
        let dispatcher = build_full_chat_dispatcher(FullDispatcherParams {
            cfg: &cfg,
            session_id: "session-under-test",
            folders: &folders_at(sandbox.path()),
            pending_user_inputs: &None,
            extra_executors: Vec::new(),
            sources: apollia_tools::SourceAccess::default(),
            memory_namespace: "apollia".to_owned(),
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

    // GIVEN the dispatcher a chat session really gets, with no project open and
    //       no trusted path, where a write used to be classified by risk
    // WHEN file_write is dispatched on the conversation path
    // THEN the file is written at once: its consent was the family card of
    //      the loop, and no prompt of its own sits in front of the executor
    #[tokio::test]
    async fn test_chat_dispatcher_writes_without_a_prompt_of_its_own() {
        // GIVEN
        let data_dir = tempfile::tempdir().expect("tempdir");
        let sandbox = tempfile::tempdir().expect("tempdir");
        let cfg = chat_tools_config(data_dir.path());
        let dispatcher = build_full_chat_dispatcher(FullDispatcherParams {
            cfg: &cfg,
            session_id: "session-under-test",
            folders: &folders_at(sandbox.path()),
            pending_user_inputs: &None,
            extra_executors: Vec::new(),
            sources: apollia_tools::SourceAccess::default(),
            memory_namespace: "apollia".to_owned(),
        });

        // WHEN
        let result = dispatcher
            .dispatch(
                "file_write",
                serde_json::json!({ "path": "note.txt", "content": "hello" }),
            )
            .await;

        // THEN
        assert!(result.is_ok(), "file_write failed: {result:?}");
        assert_eq!(
            std::fs::read_to_string(sandbox.path().join("note.txt")).expect("written"),
            "hello"
        );
    }

    /// Switch `tools` off in the governance database of `data_dir`, the way
    /// `apollia-os tools disable` and the desktop tool page write them.
    fn disable_in_governance(data_dir: &std::path::Path, tools: &[&str]) {
        apollia_tools::governance_db::GovernanceDb::open(data_dir).expect("init governance.db");
        let db_path = data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME);
        let mut registry =
            apollia_tools::NativeToolRegistry::new(&db_path).expect("open the tool registry");
        for tool in tools {
            registry.set_enabled(tool, false).expect("disable the tool");
        }
    }

    // GIVEN a data directory whose governance database carries a plain native,
    //       a re-added code native and the http_fetch wrapper switched off
    // WHEN the dispatcher a chat session really gets is built over it
    // THEN none of the three is registered, while an untouched native still is
    #[tokio::test]
    async fn test_chat_dispatcher_drops_the_tools_governance_disabled() {
        // GIVEN
        let data_dir = tempfile::tempdir().expect("tempdir");
        let sandbox = tempfile::tempdir().expect("tempdir");
        disable_in_governance(
            data_dir.path(),
            &["file_read", "bash_executor", "http_fetch"],
        );
        let cfg = chat_tools_config(data_dir.path());

        // WHEN
        let dispatcher = build_full_chat_dispatcher(FullDispatcherParams {
            cfg: &cfg,
            session_id: "session-under-test",
            folders: &folders_at(sandbox.path()),
            pending_user_inputs: &None,
            extra_executors: Vec::new(),
            sources: apollia_tools::SourceAccess::default(),
            memory_namespace: "apollia".to_owned(),
        });

        // THEN
        let names = dispatcher.tool_names();
        for disabled in ["file_read", "bash_executor", "http_fetch"] {
            assert!(
                !names.contains(&disabled),
                "{disabled} was disabled in governance.db, yet the chat dispatcher registered it: {names:?}"
            );
        }
        assert!(
            names.contains(&"file_list"),
            "an untouched native should stay registered: {names:?}"
        );
    }

    // GIVEN a governance database with `bash_executor` switched off
    // WHEN bash_executor is dispatched on the conversation path
    // THEN the call is refused as an unknown tool instead of running
    #[tokio::test]
    async fn test_chat_dispatcher_refuses_a_governance_disabled_bash() {
        // GIVEN
        let data_dir = tempfile::tempdir().expect("tempdir");
        let sandbox = tempfile::tempdir().expect("tempdir");
        disable_in_governance(data_dir.path(), &["bash_executor"]);
        let cfg = chat_tools_config(data_dir.path());
        let dispatcher = build_full_chat_dispatcher(FullDispatcherParams {
            cfg: &cfg,
            session_id: "session-under-test",
            folders: &folders_at(sandbox.path()),
            pending_user_inputs: &None,
            extra_executors: Vec::new(),
            sources: apollia_tools::SourceAccess::default(),
            memory_namespace: "apollia".to_owned(),
        });

        // WHEN
        let result = dispatcher
            .dispatch(
                "bash_executor",
                serde_json::json!({ "command": "echo governance-bypassed", "timeout_secs": 30 }),
            )
            .await;

        // THEN
        assert!(
            matches!(
                result,
                Err(apollia_tools::executor::ToolExecutionError::UnknownTool { .. })
            ),
            "a governance-disabled bash_executor must not run: {result:?}"
        );
    }
}

#[cfg(test)]
mod folder_tests;

#[cfg(test)]
mod fail_closed_tests;
