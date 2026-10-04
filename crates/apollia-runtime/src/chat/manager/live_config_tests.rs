//! The chat follows the live sections of the configuration: a section saved
//! by the route applies to the next conversation or the next exchange.

use super::*;
use crate::config_reload::tests::Rig;
use crate::config_reload::ConfigFeed;

/// A chat actor over a temporary database, built with the boot values of a
/// runtime started without a file, following `feed`.
fn manager_following(dir: &tempfile::TempDir, feed: ConfigFeed) -> ChatSessionManager {
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let (tx, _rx) = mpsc::channel(16);
    ChatSessionManager {
        sessions: HashMap::new(),
        repository: ChatSessionRepository::open(&dir.path().join("chat.db")).expect("open"),
        llm_router: None,
        tool_registry: ToolRegistryHandle::start(),
        registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
        agent_runner: None,
        event_bus: event_tx,
        runtime_budget: StepBudgetConfig::default(),
        plan_mode_default: false,
        pending_chat_approvals: PendingChatApprovals::new(),
        pending_fs_approvals: PendingFilesystemApprovals::new(),
        pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
        mcp_handle: None,
        chat_tools_config: Some(Arc::new(ChatToolsConfig {
            data_dir: dir.path().to_path_buf(),
            brave_api_key: None,
            tools_config: apollia_core::ToolsConfig::default(),
            tool_turn_temperature: None,
            source_registry: None,
        })),
        governance_db_path: None,
        pending_user_replies: HashMap::new(),
        metrics: HashMap::new(),
        user_memory: None,
        enrichment_extractor: None,
        tx,
        a2a_invoker: None,
        mcp_loading: LoadingMode::Deferred,
        tool_search_limit: 20,
        todo_handle: None,
        plan_handle: None,
        hook_executor: None,
        package_tools: None,
        composer: None,
        pause_tokens: HashMap::new(),
        pause_states: HashMap::new(),
        pending_injections: HashMap::new(),
        pending_plan_continuations: HashMap::new(),
        config_feed: Some(feed),
        plan_mode_default_tx: tokio::sync::watch::Sender::new(false),
        hook_executor_tx: tokio::sync::watch::Sender::new(None),
    }
}

/// A chat actor spawned as the supervisor spawns it, without hooks and with
/// plan mode off.
fn spawned(dir: &tempfile::TempDir) -> ChatSessionManagerHandle {
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
    ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::empty())),
        ToolRegistryHandle::start(),
        Arc::new(crate::api::test_support::StemLoader { refusal: None }),
        None,
        event_tx,
        StepBudgetConfig::default(),
        None,
        registry_handle,
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .expect("spawn manager")
}

fn libre() -> CreateSessionParams {
    CreateSessionParams {
        mode: ChatMode::Libre,
        agent_name: None,
        system_prompt: None,
        tools: vec![],
    }
}

#[tokio::test]
async fn the_chat_section_saved_by_the_route_applies_to_the_next_conversation() {
    // GIVEN a chat started with plan mode off, following its runtime
    let dir = tempfile::tempdir().unwrap();
    let rig = Rig::on("");
    let chat = spawned(&dir);
    chat.follow_config(rig.feed()).await;

    // WHEN the route turns the plan-mode default on
    rig.save("[chat]\nplan_mode_default = true\n").await;

    // THEN the next conversation starts in plan mode, and the default the
    // handle answers follows
    let info = chat.create_session(libre()).await.expect("create");
    let detail = chat.get_session(info.id).await.expect("detail");
    assert!(detail.session.plan_mode);
    assert!(chat.plan_mode_default());
}

#[tokio::test]
async fn the_hooks_section_saved_by_the_route_rebuilds_the_registry_of_the_next_exchange() {
    // GIVEN a chat started without hooks, following its runtime
    let dir = tempfile::tempdir().unwrap();
    let rig = Rig::on("");
    let chat = spawned(&dir);
    chat.follow_config(rig.feed()).await;
    assert!(chat.hook_summaries().is_empty());

    // WHEN the route declares a handler, and the chat handles its next command
    rig.save(
        "[[hooks.handlers]]\nevents = [\"pre_tool_use\"]\ntype = \"http\"\nurl = \"http://127.0.0.1:9000/hook\"\n",
    )
    .await;
    chat.create_session(libre()).await.expect("create");

    // THEN the registry in force lists it
    let hooks = chat.hook_summaries();
    assert_eq!(hooks.len(), 1, "{hooks:?}");
}

#[tokio::test]
async fn the_tools_section_saved_by_the_route_reaches_the_dispatcher_of_the_next_exchange() {
    // GIVEN a chat actor following its runtime
    let dir = tempfile::tempdir().unwrap();
    let rig = Rig::on("");
    let mut manager = manager_following(&dir, rig.feed());

    // WHEN the route disables a tool, and the actor takes its next command
    rig.save("[tools]\ndisabled = [\"web_read\"]\nmax_output_chars = 900\n")
        .await;
    manager.apply_config_changes();

    // THEN the configuration the next exchange builds its dispatcher from
    // carries it
    let tools = &manager
        .chat_tools_config
        .as_ref()
        .expect("config")
        .tools_config;
    assert_eq!(tools.disabled, vec!["web_read".to_owned()]);
    assert_eq!(tools.max_output_chars, 900);
}

#[tokio::test]
async fn the_mcp_section_saved_by_the_route_applies_to_the_next_exchange() {
    // GIVEN a chat actor following its runtime, loading MCP tools deferred
    let dir = tempfile::tempdir().unwrap();
    let rig = Rig::on("");
    let mut manager = manager_following(&dir, rig.feed());

    // WHEN the route loads them eagerly with a smaller search limit, and the
    // actor takes its next command
    rig.save("[mcp]\ntool_loading = \"eager\"\ntool_search_limit = 5\n")
        .await;
    manager.apply_config_changes();

    // THEN the next exchange uses both
    assert_eq!(manager.mcp_loading, LoadingMode::Eager);
    assert_eq!(manager.tool_search_limit, 5);
}

#[tokio::test]
async fn a_change_of_a_section_the_chat_does_not_read_leaves_its_tools_untouched() {
    // GIVEN a chat actor following its runtime
    let dir = tempfile::tempdir().unwrap();
    let rig = Rig::on("");
    let mut manager = manager_following(&dir, rig.feed());
    let before = Arc::clone(manager.chat_tools_config.as_ref().expect("config"));

    // WHEN the route changes only the port
    rig.save("[api]\nport = 7790\n").await;
    manager.apply_config_changes();

    // THEN the configuration of the next exchange is the same one
    let after = manager.chat_tools_config.as_ref().expect("config");
    assert!(Arc::ptr_eq(&before, after));
}
