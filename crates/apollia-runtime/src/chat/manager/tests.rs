use super::*;
use apollia_core::AgentManifest;

/// Stub AgentLoader that always succeeds.
struct AlwaysOkLoader;
impl AgentLoader for AlwaysOkLoader {
    fn load_and_validate(&self, _path: &Path) -> Result<AgentManifest, String> {
        Ok(AgentManifest {
            name: "test-agent".into(),
            version: "0.1.0".into(),
            description: "test".into(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".into(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        })
    }
}

/// Stub AgentLoader that always fails.
struct AlwaysFailLoader;
impl AgentLoader for AlwaysFailLoader {
    fn load_and_validate(&self, _path: &Path) -> Result<AgentManifest, String> {
        Err("agent not found".into())
    }
}

/// Spawn a ChatSessionManager backed by a temp SQLite database.
fn spawn_test_manager(
    dir: &tempfile::TempDir,
    llm_router: Option<Arc<LlmRouter>>,
    agent_loader: Arc<dyn AgentLoader>,
) -> ChatSessionManagerHandle {
    spawn_test_manager_with_plan_default(dir, llm_router, agent_loader, false)
}

/// Spawn a ChatSessionManager with an explicit plan-mode default, so a test
/// can verify the default is applied at session creation.
fn spawn_test_manager_with_plan_default(
    dir: &tempfile::TempDir,
    llm_router: Option<Arc<LlmRouter>>,
    agent_loader: Arc<dyn AgentLoader>,
    plan_mode_default: bool,
) -> ChatSessionManagerHandle {
    let db_path = dir.path().join("chat.db");
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
    ChatSessionManagerHandle::spawn(
        &db_path,
        llm_router,
        tool_registry,
        agent_loader,
        None, // no agent runner in basic tests
        event_tx,
        StepBudgetConfig::default(),
        None, // no user memory in basic tests
        registry_handle,
        None, // no A2A invoker in basic tests
        None, // no project context in basic tests
        None, // no project repo in basic tests
        None, // no mcp handle in basic tests
        None, // no chat tools config in basic tests
        LoadingMode::Eager,
        20,
        None, // no hooks in tests
        plan_mode_default,
    )
    .expect("spawn manager")
}

fn fake_llm_router() -> Option<Arc<LlmRouter>> {
    Some(Arc::new(LlmRouter::empty()))
}

/// Minimal Libre-mode session params used by most manager tests.
fn libre_session_params() -> CreateSessionParams {
    CreateSessionParams {
        mode: ChatMode::Libre,
        agent_name: None,
        system_prompt: None,
        tools: vec![],
        project_id: None,
    }
}

#[tokio::test]
async fn test_create_session_libre() {
    // GIVEN a ChatSessionManager with LLM configured
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    // WHEN create_session mode=Libre
    let info = handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools: vec!["bash_executor".into()],
            project_id: None,
        })
        .await
        .expect("create_session");

    // THEN Ok(SessionInfo) with status=Active
    assert_eq!(info.mode, ChatMode::Libre);
    assert_eq!(info.status, SessionStatus::Active);
    assert!(info.agent_name.is_none());

    handle.shutdown().await;
}

#[tokio::test]
async fn test_create_session_agent_without_name() {
    // GIVEN a ChatSessionManager
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    // WHEN create_session mode=Agent, agent_name=None
    let result = handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Agent,
            agent_name: None,
            system_prompt: None,
            tools: vec![],
            project_id: None,
        })
        .await;

    // THEN Err(ChatError::AgentNotFound)
    assert!(matches!(result, Err(ChatError::AgentNotFound(_))));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_create_session_no_llm() {
    // GIVEN a ChatSessionManager without LLM
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, None, Arc::new(AlwaysOkLoader));

    // WHEN create_session mode=Libre
    let result = handle.create_session(libre_session_params()).await;

    // THEN Err(ChatError::NoLlmConfigured)
    assert!(matches!(result, Err(ChatError::NoLlmConfigured)));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_list_sessions() {
    // GIVEN 2 sessions created
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    handle
        .create_session(libre_session_params())
        .await
        .expect("create 1");
    handle
        .create_session(libre_session_params())
        .await
        .expect("create 2");

    // WHEN list_sessions
    let sessions = handle.list_sessions(None).await;

    // THEN 2 sessions returned
    assert_eq!(sessions.len(), 2);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_get_session_detail() {
    // GIVEN session with 3 messages
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");

    for i in 0..3 {
        handle
            .send_message(info.id.clone(), format!("message {i}"))
            .await
            .expect("send");
    }

    // WHEN get_session
    let detail = handle.get_session(info.id.clone()).await;

    // THEN SessionDetail with 3 messages
    let detail = detail.expect("should exist");
    assert_eq!(detail.message_count, 3);
    assert_eq!(detail.session.history.len(), 3);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_get_session_todo_empty_and_unknown() {
    // GIVEN a freshly created session with no todo items
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));
    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");

    // WHEN reading the todo for the known but empty session
    let items = handle
        .get_session_todo(info.id.clone())
        .await
        .expect("known session");

    // THEN an empty list is returned (200 semantics)
    assert!(items.is_empty());

    // WHEN reading the todo for an unknown session
    let unknown = handle.get_session_todo("does-not-exist".to_string()).await;

    // THEN SessionNotFound is returned (404 semantics)
    assert!(matches!(unknown, Err(ChatError::SessionNotFound(_))));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_close_session() {
    // GIVEN session active
    let dir = tempfile::tempdir().expect("tempdir");
    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
    let db_path = dir.path().join("chat.db");
    let tool_registry = ToolRegistryHandle::start();
    let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
    let handle = ChatSessionManagerHandle::spawn(
        &db_path,
        fake_llm_router(),
        tool_registry,
        Arc::new(AlwaysOkLoader),
        None,
        event_tx,
        StepBudgetConfig::default(),
        None,
        registry_handle,
        None,
        None,
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,  // no hooks in tests
        false, // plan-mode default off in tests
    )
    .expect("spawn");

    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");

    // Drain the ChatSessionCreated event
    let _ = event_rx.recv().await;

    // WHEN close_session
    handle.close_session(info.id.clone()).await.expect("close");

    // THEN ChatSessionClosed event is emitted
    let event = event_rx.recv().await.expect("event");
    assert!(matches!(event, RuntimeEvent::ChatSessionClosed { .. }));

    // AND session detail shows Closed
    let detail = handle.get_session(info.id.clone()).await.expect("detail");
    assert_eq!(detail.session.status, SessionStatus::Closed);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_send_message_to_closed_session() {
    // GIVEN session closed
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");
    handle.close_session(info.id.clone()).await.expect("close");

    // WHEN send_message
    let result = handle.send_message(info.id.clone(), "hello".into()).await;

    // THEN Err(ChatError::SessionClosed)
    assert!(matches!(result, Err(ChatError::SessionClosed(_))));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_send_message_returns_message_id() {
    // GIVEN active session
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");

    // WHEN send_message
    let msg_id = handle
        .send_message(info.id.clone(), "Bonjour".into())
        .await
        .expect("send");

    // THEN a valid message ID is returned
    assert!(!msg_id.is_empty());

    handle.shutdown().await;
}

#[tokio::test]
async fn test_resolve_tool_approval() {
    // GIVEN a manager with a registered pending approval
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("chat.db");
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let repository = ChatSessionRepository::open(&db_path).expect("open");
    let pending = PendingChatApprovals::new();
    let rx = pending.register("sess-1::msg-1::bash".to_string());

    let (tx, _rx) = mpsc::channel(256);
    // Manually build manager to inject pending_chat_approvals
    let mut manager = ChatSessionManager {
        sessions: HashMap::new(),
        repository,
        llm_router: fake_llm_router(),
        tool_registry,
        registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
        agent_runner: None,
        event_bus: event_tx,
        runtime_budget: StepBudgetConfig::default(),
        plan_mode_default: false,
        pending_chat_approvals: pending,
        pending_fs_approvals: PendingFilesystemApprovals::new(),
        pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
        mcp_handle: None,
        chat_tools_config: None,
        pending_user_replies: HashMap::new(),
        metrics: HashMap::new(),
        user_memory: None,
        enrichment_extractor: None,
        tx,
        a2a_invoker: None,
        project_context: None,
        project_repo: None,
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        todo_handle: None,
        plan_handle: None,
        hook_executor: None,
        pause_tokens: HashMap::new(),
        pause_states: HashMap::new(),
        pending_injections: HashMap::new(),
        pending_plan_continuations: HashMap::new(),
    };

    // Insert a dummy session so the lookup succeeds
    let session = ChatSession {
        id: "sess-1".into(),
        mode: ChatMode::Libre,
        agent_name: None,
        system_prompt: String::new(),
        status: SessionStatus::Processing,
        history: vec![],
        authorized_tools: std::collections::HashSet::new(),
        available_tools: vec!["bash".into()],
        created_at: "2026-03-20T10:00:00Z".into(),
        active_exchange: None,
        llm_backend: None,
        title: None,
        parent_session_id: None,
        fork_depth: 0,
        project_id: None,
        force_project_context_inject: false,
        fs_allow_rules: std::sync::Arc::new(
            std::sync::Mutex::new(std::collections::HashSet::new()),
        ),
        plan_mode: false,
        plan_phase: PlanPhase::Done,
    };
    manager.sessions.insert("sess-1".into(), session);

    // WHEN resolve_tool Accept
    let result =
        manager.handle_resolve_tool("sess-1", "msg-1", "bash", "bash", ToolDecision::Accept);

    // THEN ok, approval resolved
    assert!(result.is_ok());

    // AND the receiver gets the decision
    let decision = rx.await.expect("decision");
    assert_eq!(decision, ToolDecision::Accept);
}

#[tokio::test]
async fn test_always_accept_not_honored_for_code_executor() {
    // GIVEN a manager with pending approvals for a code executor and a normal tool
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("chat.db");
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let repository = ChatSessionRepository::open(&db_path).expect("open");
    let pending = PendingChatApprovals::new();
    let rx_bash = pending.register("sess-1::msg-1::bash_executor".to_string());
    let rx_web = pending.register("sess-1::msg-1::web_read".to_string());

    let (tx, _rx) = mpsc::channel(256);
    let mut manager = ChatSessionManager {
        sessions: HashMap::new(),
        repository,
        llm_router: fake_llm_router(),
        tool_registry,
        registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
        agent_runner: None,
        event_bus: event_tx,
        runtime_budget: StepBudgetConfig::default(),
        plan_mode_default: false,
        pending_chat_approvals: pending,
        pending_fs_approvals: PendingFilesystemApprovals::new(),
        pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
        mcp_handle: None,
        chat_tools_config: None,
        pending_user_replies: HashMap::new(),
        metrics: HashMap::new(),
        user_memory: None,
        enrichment_extractor: None,
        tx,
        a2a_invoker: None,
        project_context: None,
        project_repo: None,
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        todo_handle: None,
        plan_handle: None,
        hook_executor: None,
        pause_tokens: HashMap::new(),
        pause_states: HashMap::new(),
        pending_injections: HashMap::new(),
        pending_plan_continuations: HashMap::new(),
    };

    let session = ChatSession {
        id: "sess-1".into(),
        mode: ChatMode::Libre,
        agent_name: None,
        system_prompt: String::new(),
        status: SessionStatus::Processing,
        history: vec![],
        authorized_tools: std::collections::HashSet::new(),
        available_tools: vec!["bash_executor".into(), "web_read".into()],
        created_at: "2026-03-20T10:00:00Z".into(),
        active_exchange: None,
        llm_backend: None,
        title: None,
        parent_session_id: None,
        fork_depth: 0,
        project_id: None,
        force_project_context_inject: false,
        fs_allow_rules: std::sync::Arc::new(
            std::sync::Mutex::new(std::collections::HashSet::new()),
        ),
        plan_mode: false,
        plan_phase: PlanPhase::Done,
    };
    manager.sessions.insert("sess-1".into(), session);

    let always = ToolDecision::AlwaysAccept {
        scope: crate::chat::AlwaysAcceptScope::ThisSession,
    };

    // WHEN "always accept" resolves for the code executor
    manager
        .handle_resolve_tool(
            "sess-1",
            "msg-1",
            "bash_executor",
            "bash_executor",
            always.clone(),
        )
        .expect("resolve bash");
    // AND for a normal tool
    manager
        .handle_resolve_tool("sess-1", "msg-1", "web_read", "web_read", always.clone())
        .expect("resolve web");

    // THEN the current calls are still approved (the decisions are delivered)
    assert!(rx_bash.await.expect("bash decision").is_always_accept());
    assert!(rx_web.await.expect("web decision").is_always_accept());

    // AND the code executor is NOT blanket-authorized, while the normal tool is
    let session = manager.sessions.get("sess-1").expect("session");
    assert!(!session.authorized_tools.contains("bash_executor"));
    assert!(session.authorized_tools.contains("web_read"));
}

#[tokio::test]
async fn test_shutdown() {
    // GIVEN a ChatSessionManager spawned
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    // WHEN shutdown
    handle.shutdown().await;

    // THEN the actor stops, subsequent sends fail gracefully
    let result = handle.create_session(libre_session_params()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_close_already_closed_session() {
    // GIVEN a closed session
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");
    handle.close_session(info.id.clone()).await.expect("close");

    // WHEN close_session again
    let result = handle.close_session(info.id.clone()).await;

    // THEN Err(ChatError::SessionClosed)
    assert!(matches!(result, Err(ChatError::SessionClosed(_))));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_cross_session_context_substantive_message() {
    // GIVEN 3 past sessions with summaries
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("chat.db");

    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let (tx, _rx) = mpsc::channel(256);
    let repository = ChatSessionRepository::open(&db_path).expect("open");

    // Seed past sessions with summaries on the same repository instance
    for (id, summary, ts) in [
        (
            "past-1",
            "Discussion about data migration project using batch processing",
            "2026-03-20T10:00:00Z",
        ),
        (
            "past-2",
            "Review of API design for user management endpoints",
            "2026-03-18T10:00:00Z",
        ),
        (
            "past-3",
            "Setup of CI/CD pipeline with GitHub Actions",
            "2026-03-15T10:00:00Z",
        ),
    ] {
        repository
            .create_session(id, &ChatMode::Libre, None, "", &[], ts, None, None)
            .expect("create");
        repository.close_session(id, ts).expect("close");
        repository.update_summary(id, summary).expect("summary");
    }

    let manager = ChatSessionManager {
        sessions: HashMap::new(),
        repository,
        llm_router: fake_llm_router(),
        tool_registry,
        registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
        agent_runner: None,
        event_bus: event_tx,
        runtime_budget: StepBudgetConfig::default(),
        plan_mode_default: false,
        pending_chat_approvals: PendingChatApprovals::new(),
        pending_fs_approvals: PendingFilesystemApprovals::new(),
        pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
        mcp_handle: None,
        chat_tools_config: None,
        pending_user_replies: HashMap::new(),
        metrics: HashMap::new(),
        user_memory: None,
        enrichment_extractor: None,
        tx,
        a2a_invoker: None,
        project_context: None,
        project_repo: None,
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        todo_handle: None,
        plan_handle: None,
        hook_executor: None,
        pause_tokens: HashMap::new(),
        pause_states: HashMap::new(),
        pending_injections: HashMap::new(),
        pending_plan_continuations: HashMap::new(),
    };

    // WHEN building cross-session context with a substantive first message
    let context = manager.build_cross_session_context("data migration project batch processing");

    // THEN a context block with past sessions is returned
    let block = context.expect("should have cross-session context");
    assert!(block.starts_with("## Previous conversations (for reference)\n"));
    assert!(block.contains("migration"));
}

#[tokio::test]
async fn test_cross_session_context_trivial_message() {
    // GIVEN past sessions with summaries
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("chat.db");

    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let (tx, _rx) = mpsc::channel(256);
    let repository = ChatSessionRepository::open(&db_path).expect("open");

    repository
        .create_session(
            "past-1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");
    repository
        .close_session("past-1", "2026-03-20T12:00:00Z")
        .expect("close");
    repository
        .update_summary("past-1", "Discussion about data migration")
        .expect("summary");

    let manager = ChatSessionManager {
        sessions: HashMap::new(),
        repository,
        llm_router: fake_llm_router(),
        tool_registry,
        registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
        agent_runner: None,
        event_bus: event_tx,
        runtime_budget: StepBudgetConfig::default(),
        plan_mode_default: false,
        pending_chat_approvals: PendingChatApprovals::new(),
        pending_fs_approvals: PendingFilesystemApprovals::new(),
        pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
        mcp_handle: None,
        chat_tools_config: None,
        pending_user_replies: HashMap::new(),
        metrics: HashMap::new(),
        user_memory: None,
        enrichment_extractor: None,
        tx,
        a2a_invoker: None,
        project_context: None,
        project_repo: None,
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        todo_handle: None,
        plan_handle: None,
        hook_executor: None,
        pause_tokens: HashMap::new(),
        pause_states: HashMap::new(),
        pending_injections: HashMap::new(),
        pending_plan_continuations: HashMap::new(),
    };

    // WHEN building cross-session context with a trivial message
    let context = manager.build_cross_session_context("bonjour");

    // THEN None is returned (message too short)
    assert!(context.is_none());
}

#[tokio::test]
async fn test_cross_session_context_no_relevant_sessions() {
    // GIVEN a repository with no sessions (empty)
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("chat.db");

    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let (tx, _rx) = mpsc::channel(256);
    let repository = ChatSessionRepository::open(&db_path).expect("open");

    let manager = ChatSessionManager {
        sessions: HashMap::new(),
        repository,
        llm_router: fake_llm_router(),
        tool_registry,
        registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
        agent_runner: None,
        event_bus: event_tx,
        runtime_budget: StepBudgetConfig::default(),
        plan_mode_default: false,
        pending_chat_approvals: PendingChatApprovals::new(),
        pending_fs_approvals: PendingFilesystemApprovals::new(),
        pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
        mcp_handle: None,
        chat_tools_config: None,
        pending_user_replies: HashMap::new(),
        metrics: HashMap::new(),
        user_memory: None,
        enrichment_extractor: None,
        tx,
        a2a_invoker: None,
        project_context: None,
        project_repo: None,
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        todo_handle: None,
        plan_handle: None,
        hook_executor: None,
        pause_tokens: HashMap::new(),
        pause_states: HashMap::new(),
        pending_injections: HashMap::new(),
        pending_plan_continuations: HashMap::new(),
    };

    // WHEN building cross-session context with a substantive message but no past sessions
    let context = manager.build_cross_session_context("data migration project batch processing");

    // THEN None is returned (no relevant sessions found)
    assert!(context.is_none());
}

#[tokio::test]
async fn test_set_plan_mode_toggles_and_persists() {
    // GIVEN a freshly created Libre session (plan mode off by default)
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));
    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");
    let before = handle.get_session(info.id.clone()).await.expect("detail");
    assert!(!before.session.plan_mode);

    // WHEN plan mode is enabled
    handle
        .set_plan_mode(info.id.clone(), true)
        .await
        .expect("enable");

    // THEN the in-memory session reflects plan mode in Discovery phase
    let enabled = handle.get_session(info.id.clone()).await.expect("detail");
    assert!(enabled.session.plan_mode);
    assert_eq!(enabled.session.plan_phase, PlanPhase::Discovery);

    // AND disabling resets the phase to Done (never stuck awaiting approval)
    handle
        .set_plan_mode(info.id.clone(), false)
        .await
        .expect("disable");
    let disabled = handle.get_session(info.id.clone()).await.expect("detail");
    assert!(!disabled.session.plan_mode);
    assert_eq!(disabled.session.plan_phase, PlanPhase::Done);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_new_session_inherits_plan_mode_default_off() {
    // GIVEN a manager with the plan-mode default off (the standard case)
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager_with_plan_default(
        &dir,
        fake_llm_router(),
        Arc::new(AlwaysOkLoader),
        false,
    );

    // WHEN a new session is created
    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");

    // THEN it starts with plan mode off and a neutral phase
    let detail = handle.get_session(info.id.clone()).await.expect("detail");
    assert!(!detail.session.plan_mode);
    assert_eq!(detail.session.plan_phase, PlanPhase::Done);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_new_session_inherits_plan_mode_default_on() {
    // GIVEN a manager with the plan-mode default enabled
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager_with_plan_default(
        &dir,
        fake_llm_router(),
        Arc::new(AlwaysOkLoader),
        true,
    );

    // WHEN a new session is created
    let info = handle
        .create_session(libre_session_params())
        .await
        .expect("create");

    // THEN it inherits plan mode on, starting in the Discovery phase
    let detail = handle.get_session(info.id.clone()).await.expect("detail");
    assert!(detail.session.plan_mode);
    assert_eq!(detail.session.plan_phase, PlanPhase::Discovery);

    // AND the per-session toggle still overrides the inherited default
    handle
        .set_plan_mode(info.id.clone(), false)
        .await
        .expect("disable");
    let overridden = handle.get_session(info.id.clone()).await.expect("detail");
    assert!(!overridden.session.plan_mode);
    assert_eq!(overridden.session.plan_phase, PlanPhase::Done);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_set_plan_mode_survives_reload() {
    // GIVEN a session with plan mode enabled, persisted to SQLite
    let dir = tempfile::tempdir().expect("tempdir");
    let first = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));
    let info = first
        .create_session(libre_session_params())
        .await
        .expect("create");
    first
        .set_plan_mode(info.id.clone(), true)
        .await
        .expect("enable");
    first.shutdown().await;

    // WHEN a brand-new manager reopens the same database and resumes it
    let second = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));
    let detail = second
        .resume_session(info.id.clone())
        .await
        .expect("resume");

    // THEN both plan-mode fields survived the reload from disk
    assert!(detail.session.plan_mode);
    assert_eq!(detail.session.plan_phase, PlanPhase::Discovery);

    second.shutdown().await;
}

/// Build a directly-constructed manager holding one Libre session persisted
/// in SQLite, forced into the given plan phase both in memory and on disk.
/// Returns the manager and a fresh event-bus receiver for assertions.
pub(super) fn manager_with_session_in_phase(
    dir: &tempfile::TempDir,
    phase: PlanPhase,
) -> (
    ChatSessionManager,
    tokio::sync::broadcast::Receiver<RuntimeEvent>,
) {
    let db_path = dir.path().join("chat.db");
    let (event_tx, event_rx) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let repository = ChatSessionRepository::open(&db_path).expect("open");
    repository
        .create_session(
            "sess-1",
            &ChatMode::Libre,
            None,
            "",
            &["bash".to_string()],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create row");
    repository
        .set_plan_mode("sess-1", true, phase)
        .expect("set phase");

    let (tx, _rx) = mpsc::channel(256);
    let mut manager = ChatSessionManager {
        sessions: HashMap::new(),
        repository,
        llm_router: fake_llm_router(),
        tool_registry,
        registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
        agent_runner: None,
        event_bus: event_tx,
        runtime_budget: StepBudgetConfig::default(),
        plan_mode_default: false,
        pending_chat_approvals: PendingChatApprovals::new(),
        pending_fs_approvals: PendingFilesystemApprovals::new(),
        pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
        mcp_handle: None,
        chat_tools_config: None,
        pending_user_replies: HashMap::new(),
        metrics: HashMap::new(),
        user_memory: None,
        enrichment_extractor: None,
        tx,
        a2a_invoker: None,
        project_context: None,
        project_repo: None,
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        todo_handle: None,
        plan_handle: None,
        hook_executor: None,
        pause_tokens: HashMap::new(),
        pause_states: HashMap::new(),
        pending_injections: HashMap::new(),
        pending_plan_continuations: HashMap::new(),
    };
    let session = ChatSession {
        id: "sess-1".into(),
        mode: ChatMode::Libre,
        agent_name: None,
        system_prompt: String::new(),
        status: SessionStatus::Active,
        history: vec![],
        authorized_tools: std::collections::HashSet::new(),
        available_tools: vec!["bash".into()],
        created_at: "2026-03-20T10:00:00Z".into(),
        active_exchange: None,
        llm_backend: None,
        title: None,
        parent_session_id: None,
        fork_depth: 0,
        project_id: None,
        force_project_context_inject: false,
        fs_allow_rules: std::sync::Arc::new(
            std::sync::Mutex::new(std::collections::HashSet::new()),
        ),
        plan_mode: true,
        plan_phase: phase,
    };
    manager.sessions.insert("sess-1".into(), session);
    (manager, event_rx)
}

/// Drain the event bus until a `ChatPlanApproved`/`ChatPlanRejected` arrives
/// or the channel empties. The continuation dispatch emits other events
/// (e.g. `ChatMessageSent`) so the gate event is not guaranteed first.
fn next_gate_event(
    rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
) -> Option<RuntimeEvent> {
    loop {
        match rx.try_recv() {
            Ok(
                e @ (RuntimeEvent::ChatPlanApproved { .. } | RuntimeEvent::ChatPlanRejected { .. }),
            ) => return Some(e),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

#[tokio::test]
async fn test_approve_plan_transitions_to_executing_and_emits() {
    // GIVEN a session awaiting approval
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, mut rx) = manager_with_session_in_phase(&dir, PlanPhase::AwaitingApproval);

    // WHEN the plan is approved
    manager
        .handle_approve_plan("sess-1")
        .await
        .expect("approve ok");

    // THEN the phase moves to Executing (in memory and persisted) and
    // ChatPlanApproved is emitted; the continuation turn is dispatched, so
    // the session has accepted a new exchange (status Processing).
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::Executing
    );
    let persisted = manager
        .repository
        .get_session("sess-1")
        .expect("get")
        .expect("row");
    assert_eq!(persisted.plan_phase, PlanPhase::Executing.as_sql());
    match next_gate_event(&mut rx) {
        Some(RuntimeEvent::ChatPlanApproved { session_id }) => {
            assert_eq!(session_id, "sess-1");
        }
        other => panic!("expected ChatPlanApproved, got {other:?}"),
    }
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().status,
        SessionStatus::Processing
    );
}

/// Build a chat-scope [`PlanStep`] with no dependencies for a manager test.
pub(super) fn plan_step_for_test(id: &str) -> apollia_core::plan::PlanStep {
    apollia_core::plan::PlanStep {
        step_id: id.into(),
        title: id.into(),
        description: format!("desc {id}"),
        status: apollia_core::plan::StepStatus::Pending,
        depends_on: Vec::new(),
        tool_hint: None,
        model_hint: None,
        rationale: None,
        provenance: apollia_core::plan::StepProvenance::default(),
        args: None,
    }
}

#[tokio::test]
async fn test_approve_reconciles_stale_phase_from_plan_status() {
    // GIVEN a session whose in-memory phase is still stale (Discovery) while the
    // persisted plan status is already AwaitingApproval. This reproduces the
    // race: the approval click is enqueued before ExchangeComplete flips the
    // in-memory phase, but PlanActor::submit has already persisted the status.
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, mut rx) = manager_with_session_in_phase(&dir, PlanPhase::Discovery);

    let plan = crate::chat::plan_actor::spawn_plan_actor(
        rusqlite::Connection::open_in_memory().expect("in-memory db"),
        None,
    )
    .expect("spawn plan actor");
    plan.propose(
        "sess-1",
        vec![plan_step_for_test("a"), plan_step_for_test("b")],
        None,
    )
    .await
    .expect("propose");
    plan.submit("sess-1").await.expect("submit");
    manager.plan_handle = Some(plan);

    // WHEN the plan is approved while the in-memory phase is still Discovery
    manager
        .handle_approve_plan("sess-1")
        .await
        .expect("guard reconciles from plan status and approval proceeds");

    // THEN the guard reconciled from the authoritative plan status instead of
    // failing with NotAwaitingApproval; the phase moved to Executing (in memory
    // and persisted) and ChatPlanApproved was emitted.
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::Executing
    );
    let persisted = manager
        .repository
        .get_session("sess-1")
        .expect("get")
        .expect("row");
    assert_eq!(persisted.plan_phase, PlanPhase::Executing.as_sql());
    match next_gate_event(&mut rx) {
        Some(RuntimeEvent::ChatPlanApproved { session_id }) => {
            assert_eq!(session_id, "sess-1");
        }
        other => panic!("expected ChatPlanApproved, got {other:?}"),
    }
}

#[tokio::test]
async fn test_approve_still_rejects_when_plan_not_submitted() {
    // GIVEN a session in Discovery whose plan exists but is only a Draft (never
    // submitted), so the reconciliation must NOT open the gate.
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Discovery);

    let plan = crate::chat::plan_actor::spawn_plan_actor(
        rusqlite::Connection::open_in_memory().expect("in-memory db"),
        None,
    )
    .expect("spawn plan actor");
    plan.propose("sess-1", vec![plan_step_for_test("a")], None)
        .await
        .expect("propose");
    manager.plan_handle = Some(plan);

    // WHEN approve is requested with a Draft plan (not awaiting approval)
    let result = manager.handle_approve_plan("sess-1").await;

    // THEN the guard still fails fast: reconciliation only proceeds on a plan
    // whose persisted status is AwaitingApproval.
    assert!(matches!(
        result,
        Err(ChatError::NotAwaitingApproval { ref current_phase, .. })
            if current_phase == "discovery"
    ));
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::Discovery
    );
}

#[tokio::test]
async fn test_reject_plan_emits_and_stays_awaiting() {
    // GIVEN a session awaiting approval
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, mut rx) = manager_with_session_in_phase(&dir, PlanPhase::AwaitingApproval);

    // WHEN the plan is rejected with a reason
    manager
        .handle_reject_plan("sess-1", Some("step 2 is risky".into()))
        .await
        .expect("reject ok");

    // THEN ChatPlanRejected carries the reason and the phase stays awaiting
    match next_gate_event(&mut rx) {
        Some(RuntimeEvent::ChatPlanRejected { session_id, reason }) => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(reason.as_deref(), Some("step 2 is risky"));
        }
        other => panic!("expected ChatPlanRejected, got {other:?}"),
    }
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::AwaitingApproval
    );
}

#[tokio::test]
async fn test_message_during_awaiting_starts_revision_turn() {
    // GIVEN a session awaiting approval
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::AwaitingApproval);

    // WHEN the user sends a message instead of pressing approve
    manager
        .handle_send_message(
            "sess-1",
            "Please plan the work: audit the repo, fix the tests, then open a PR",
        )
        .expect("send ok");

    // THEN the message is accepted (a revision turn is dispatched) and the
    // phase stays AwaitingApproval: no hard block, no execution.
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::AwaitingApproval
    );
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().status,
        SessionStatus::Processing
    );
}

// ── cooperative pause / resume ────────────────────────────

#[tokio::test]
async fn pause_without_active_turn_is_noop() {
    // GIVEN a session with no active ReAct turn (no token registered)
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);

    // WHEN pause is requested
    let result = manager.handle_pause_session("sess-1");

    // THEN it returns Ok(()) with no side effect: state stays Running and no
    // token was cancelled to a non-existent turn
    assert!(result.is_ok());
    assert_eq!(manager.pause_state("sess-1"), Some(PauseState::Running));
}

#[tokio::test]
async fn pause_unknown_session_is_typed_error() {
    // GIVEN a manager with one known session
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);

    // WHEN pausing an unknown session (error case)
    let result = manager.handle_pause_session("ghost");

    // THEN a typed UnknownSession error is returned
    assert!(matches!(
        result,
        Err(PauseError::UnknownSession { ref session_id }) if session_id == "ghost"
    ));
}

#[tokio::test]
async fn pause_cancels_active_token_and_sets_pausing() {
    // GIVEN a session with an active turn (a registered cancel token)
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);
    let token = CancellationToken::new();
    manager.pause_tokens.insert("sess-1".into(), token.clone());

    // WHEN pause is requested
    manager.handle_pause_session("sess-1").expect("pause ok");

    // THEN the token is cancelled and the state moves to Pausing
    assert!(token.is_cancelled());
    assert_eq!(manager.pause_state("sess-1"), Some(PauseState::Pausing));
}

#[tokio::test]
async fn exchange_complete_records_paused_state() {
    // GIVEN a session with a paused ReAct response
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);
    manager
        .pause_tokens
        .insert("sess-1".into(), CancellationToken::new());
    let response = ChatAgentResponse {
        content: String::new(),
        tool_calls: vec![],
        newly_authorized: vec![],
        tokens_used: apollia_llm::types::TokenUsage::default(),
        thinking_trace: None,
        reasoning_boundaries: vec![],
        verification_report: None,
        frontier_ceiling_reached: false,
        final_plan_phase: None,
        paused: true,
        context_window_tokens: None,
        context_tokens_used: 0,
    };

    // WHEN the exchange completes as paused
    manager.handle_exchange_complete("sess-1", "msg-1", response);

    // THEN the session is recorded Paused, ready to resume
    assert_eq!(manager.pause_state("sess-1"), Some(PauseState::Paused));
}

#[tokio::test]
async fn resume_sets_running_and_dispatches() {
    // GIVEN a paused session
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);
    manager
        .pause_states
        .insert("sess-1".into(), PauseState::Paused);

    // WHEN resume is requested
    manager
        .handle_resume_paused_session("sess-1")
        .expect("resume ok");

    // THEN the state returns to Running and a continuation turn is dispatched
    // (the session moved to Processing)
    assert_eq!(manager.pause_state("sess-1"), Some(PauseState::Running));
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().status,
        SessionStatus::Processing
    );
}

// ── natural-language instruction injection ────────────────

#[tokio::test]
async fn inject_on_running_session_is_rejected() {
    // GIVEN a session in Running state (not paused)
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);

    // WHEN inject is requested (error case)
    let result = manager.handle_inject_instruction("sess-1", "do Y");

    // THEN it returns NotPaused, nothing is queued, no turn restarted
    assert!(matches!(
        result,
        Err(InjectError::NotPaused { ref session_id }) if session_id == "sess-1"
    ));
    assert!(manager.pending_injections.is_empty());
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().status,
        SessionStatus::Active
    );
}

#[tokio::test]
async fn inject_unknown_session_is_typed_error() {
    // GIVEN a manager with one known session
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);

    // WHEN injecting into an unknown session (error case)
    let result = manager.handle_inject_instruction("ghost", "do Y");

    // THEN a typed UnknownSession error is returned
    assert!(matches!(
        result,
        Err(InjectError::UnknownSession { ref session_id }) if session_id == "ghost"
    ));
}

#[tokio::test]
async fn inject_empty_instruction_is_rejected() {
    // GIVEN a paused session
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);
    manager
        .pause_states
        .insert("sess-1".into(), PauseState::Paused);

    // WHEN injecting whitespace-only text (error case)
    let result = manager.handle_inject_instruction("sess-1", "   ");

    // THEN it returns EmptyInstruction and nothing is queued
    assert!(matches!(result, Err(InjectError::EmptyInstruction)));
    assert!(manager.pending_injections.is_empty());
}

#[tokio::test]
async fn inject_on_paused_session_queues_and_resumes() {
    // GIVEN a paused session
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);
    manager
        .pause_states
        .insert("sess-1".into(), PauseState::Paused);

    // WHEN an operator instruction is injected
    manager
        .handle_inject_instruction("sess-1", "before step X, do Y")
        .expect("inject ok");

    // THEN the session resumes (Running, Processing) and the queued injection
    // was taken by the dispatched resume turn (consumed exactly once)
    assert_eq!(manager.pause_state("sess-1"), Some(PauseState::Running));
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().status,
        SessionStatus::Processing
    );
    assert!(
        manager.pending_injections.is_empty(),
        "the injection is consumed by the resume turn dispatch"
    );
}

#[tokio::test]
async fn test_approve_outside_awaiting_returns_typed_error() {
    // GIVEN a session in Discovery (plan mode on, not yet awaiting)
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Discovery);

    // WHEN approve is requested
    let result = manager.handle_approve_plan("sess-1").await;

    // THEN a typed NotAwaitingApproval error is returned, no transition
    assert!(matches!(
        result,
        Err(ChatError::NotAwaitingApproval { ref current_phase, .. })
            if current_phase == "discovery"
    ));
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::Discovery
    );
}

#[tokio::test]
async fn test_reject_unknown_session_returns_not_found() {
    // GIVEN a manager with a known session in Discovery
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Discovery);

    // WHEN rejecting an unknown session id
    let result = manager.handle_reject_plan("ghost", None).await;

    // THEN a typed SessionNotFound error is returned, no panic
    assert!(matches!(result, Err(ChatError::SessionNotFound(_))));
}

#[tokio::test]
async fn test_set_plan_mode_missing_session_returns_typed_error() {
    // GIVEN a manager with no matching session
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

    // WHEN toggling plan mode on an unknown session id
    let result = handle.set_plan_mode("ghost".into(), true).await;

    // THEN a typed SessionNotFound error is returned, no panic
    assert!(matches!(result, Err(ChatError::SessionNotFound(_))));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_create_session_agent_invalid_agent() {
    // GIVEN a manager with a loader that always fails
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysFailLoader));

    // WHEN create_session mode=Agent with an agent name
    let result = handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Agent,
            agent_name: Some("nonexistent".into()),
            system_prompt: None,
            tools: vec![],
            project_id: None,
        })
        .await;

    // THEN Err(ChatError::AgentNotFound)
    assert!(matches!(result, Err(ChatError::AgentNotFound(_))));

    handle.shutdown().await;
}

fn make_response(paused: bool) -> ChatAgentResponse {
    ChatAgentResponse {
        content: "done".into(),
        tool_calls: vec![],
        newly_authorized: vec![],
        tokens_used: apollia_llm::types::TokenUsage::default(),
        thinking_trace: None,
        reasoning_boundaries: vec![],
        verification_report: None,
        frontier_ceiling_reached: false,
        final_plan_phase: None,
        paused,
        context_window_tokens: None,
        context_tokens_used: 0,
    }
}

/// Drain the bus for the first `ChatPlanPhaseChanged` and return its phase.
fn next_phase_event(rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>) -> Option<String> {
    loop {
        match rx.try_recv() {
            Ok(RuntimeEvent::ChatPlanPhaseChanged { phase, .. }) => return Some(phase),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

#[tokio::test]
async fn test_closed_session_not_resurrected_by_late_exchange_complete() {
    // GIVEN a closed session
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Done);
    manager.handle_close_session("sess-1").expect("close");

    // WHEN a late ExchangeComplete from an in-flight turn is delivered
    manager.handle_exchange_complete("sess-1", "msg-1", make_response(false));

    // THEN the session stays Closed, never flipped back to Active
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().status,
        SessionStatus::Closed
    );
    let persisted = manager
        .repository
        .get_session("sess-1")
        .expect("get")
        .expect("row");
    assert_eq!(persisted.status, "closed");
}

#[tokio::test]
async fn test_close_session_purges_runtime_state_and_refuses_approvals() {
    // GIVEN an active session carrying pause tokens, state, an injection, and
    // a pending tool approval
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Done);
    manager
        .pause_tokens
        .insert("sess-1".into(), CancellationToken::new());
    manager
        .pause_states
        .insert("sess-1".into(), PauseState::Paused);
    manager.pending_injections.insert(
        "sess-1".into(),
        InjectedInstruction {
            session_id: "sess-1".into(),
            text: "queued".into(),
        },
    );
    let mut approval_rx = manager
        .pending_chat_approvals
        .register("sess-1::msg-1::bash".into());

    // WHEN the session is closed
    manager.handle_close_session("sess-1").expect("close");

    // THEN every per-session map is purged and the waiting approval is refused
    assert!(manager.pause_tokens.is_empty());
    assert!(manager.pause_states.is_empty());
    assert!(manager.pending_injections.is_empty());
    assert!(matches!(
        approval_rx.try_recv(),
        Ok(ToolDecision::Refuse { .. })
    ));
}

#[tokio::test]
async fn test_companion_send_without_llm_resets_status_to_active() {
    // GIVEN a Companion session on a manager with no LLM configured
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Done);
    manager.llm_router = None;
    if let Some(s) = manager.sessions.get_mut("sess-1") {
        s.mode = ChatMode::Companion;
    }

    // WHEN a message is sent and the dispatch fails
    let result = manager.handle_send_message("sess-1", "hello");

    // THEN the error surfaces AND the session is reset to Active (not stuck
    // Processing), with the active exchange cleared
    assert!(matches!(result, Err(ChatError::NoLlmConfigured)));
    let session = manager.sessions.get("sess-1").unwrap();
    assert_eq!(session.status, SessionStatus::Active);
    assert!(session.active_exchange.is_none());
}

#[tokio::test]
async fn test_validate_create_request_companion_requires_llm() {
    // GIVEN a Companion create request with no LLM configured
    let dir = tempfile::tempdir().expect("tempdir");
    let (manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Done);

    // WHEN the request is validated
    let result = ChatSessionManager::validate_create_request(
        &manager.registry_handle,
        false,
        ChatMode::Companion,
        None,
    )
    .await;

    // THEN it fails fast with NoLlmConfigured, like Libre mode
    assert!(matches!(result, Err(ChatError::NoLlmConfigured)));
}

#[tokio::test]
async fn test_set_plan_mode_emits_phase_changed() {
    // GIVEN a session
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, mut rx) = manager_with_session_in_phase(&dir, PlanPhase::Done);

    // WHEN plan mode is enabled at the manager level
    manager.handle_set_plan_mode("sess-1", true).expect("set");

    // THEN a phase-changed event announces the Discovery phase
    assert_eq!(next_phase_event(&mut rx).as_deref(), Some("discovery"));
}

#[tokio::test]
async fn test_approve_plan_emits_executing_phase_changed() {
    // GIVEN a session awaiting approval
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, mut rx) = manager_with_session_in_phase(&dir, PlanPhase::AwaitingApproval);

    // WHEN the plan is approved
    manager
        .handle_approve_plan("sess-1")
        .await
        .expect("approve");

    // THEN a phase-changed event announces the Executing phase
    assert_eq!(next_phase_event(&mut rx).as_deref(), Some("executing"));
}

#[test]
fn test_agent_scoped_prefix_rule_does_not_seed_the_name_set() {
    use apollia_permissions::{PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction};

    // GIVEN one agent-scoped allow rule carrying an arg_prefix and one
    // name-only, both for the chat agent
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db).expect("open engine");
    engine
        .add_rule(&PrefixRule {
            tool_name: "file_read".to_string(),
            arg_prefix: Some("/tmp/safe".to_string()),
            action: RuleAction::Allow,
            scope: PermissionScope::Agent,
            agent_id: Some(APOLLIA_CHAT_AGENT_ID.to_string()),
            ..PrefixRule::default()
        })
        .expect("seed prefixed rule");
    engine
        .add_rule(&PrefixRule {
            tool_name: "web_search".to_string(),
            arg_prefix: None,
            action: RuleAction::Allow,
            scope: PermissionScope::Agent,
            agent_id: Some(APOLLIA_CHAT_AGENT_ID.to_string()),
            ..PrefixRule::default()
        })
        .expect("seed name-only rule");

    // WHEN the chat overrides are seeded from the store
    let mut out = ChatLibreOverrides::default();
    super::libre::apply_chat_prefix_allow_rules(&mut out, &db);

    // THEN the prefixed rule does not authorize the whole tool name (it is
    // evaluated per invocation instead), while the name-only rule does
    assert!(
        !out.pre_authorized_tools.contains("file_read"),
        "a prefixed rule must not widen into a tool-wide authorization: {:?}",
        out.pre_authorized_tools
    );
    assert!(
        out.pre_authorized_tools.contains("web_search"),
        "{:?}",
        out.pre_authorized_tools
    );
}
