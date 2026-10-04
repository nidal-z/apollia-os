//! The desktop attached to a runtime that another process serves.
//!
//! The runtime runs either inside this process or as the daemon
//! `apollia-os start`. [`RuntimeLink`] is the seam between the two: an
//! embedded arm holding the [`RuntimeHandle`], and an HTTP arm holding an
//! [`AttachedClient`] that reaches the daemon over its local API with the
//! bearer token of this installation. A command routed through the seam
//! matches on it and serves both modes.
//!
//! A command not routed yet is refused in attached mode before its body runs,
//! by [`gate`], with an error that names it and says why, instead of reaching
//! for state this process never built. [`ROUTED_COMMANDS`] is the list the
//! gate reads, and `scripts/check_attached_coverage.py` counts it against every
//! registered command.

mod client;
mod proxy;
mod relay;

use apollia_core::EventBusSender;
use apollia_runtime::embedded::RuntimeHandle;
use tauri::Manager;

pub(crate) use client::AttachedClient;

/// Capacity of the local bus the relay publishes the daemon's events on, the
/// same order as the runtime's own bus.
const RELAY_BUS_CAPACITY: usize = 1024;

/// The commands that serve the attached mode. Every other registered command
/// answers [`AttachedError::NotRouted`] while the runtime is a daemon.
///
/// `start_variant_download` needs no route over HTTP: it never reads the
/// runtime, only the Hub, the models folder and its index, which the daemon
/// shares since it listens on this machine. [`run`] manages the download
/// manager it holds. `list_model_cards` needs none either: it reads the model
/// registry, the data directory and its `apollia.toml`, never the runtime.
/// `get_companion_context` needs none: it answers from a table of help texts
/// compiled into this binary. `get_resolved_setting` and `set_setting` need
/// none: they read and write the `apollia.toml` of the data directory, never
/// the runtime, and neither does `get_models_dir`, which reads the same file.
pub(crate) const ROUTED_COMMANDS: &[&str] = &[
    "chat_session_metrics",
    "create_chat_session",
    "get_chat_session",
    "list_chat_sessions",
    "pause_chat_session",
    "send_chat_message",
    // The conversations as the list manages them: configure, rename, name,
    // erase.
    "delete_chat_session",
    "generate_chat_session_name",
    "rename_chat_session",
    "update_chat_session",
    // The chat: a turn played again, a decision on a tool call, the history
    // of those decisions, and the skills of the worker agents.
    "authorize_chat_tool",
    "edit_and_resend_chat_message",
    "list_a2a_skills",
    "list_chat_approval_history",
    "regenerate_chat_response",
    // Agents: the registry, the install and the uninstall of an agent.
    "agent_status_snapshot",
    "install_agent",
    "list_agents",
    "uninstall_agent",
    // Agents: the update, restart included, over
    // `PUT /api/v1/agents/installed/{name}`.
    "update_agent",
    // Tasks: the list, the launch, the detail, the cancellation.
    "cancel_task",
    "get_task_timeline",
    "list_tasks",
    "submit_task",
    // Tasks: the deletion of a stored record and the delegation tree, over
    // `/api/v1/tasks/{id}/record` and `/api/v1/tasks/{id}/delegation-tree`.
    "delete_task",
    "get_delegation_tree",
    // Approvals: the pending and resolved ones, and the decision that resumes
    // the task.
    "list_pending_approvals",
    "list_resolved_approvals",
    "resume_task",
    // The questions an agent asked the user.
    "list_pending_user_inputs",
    "respond_user_input",
    "respond_user_input_rejected",
    // Commands whose whole body is a call to a runtime route, served through
    // the methods of `proxy.rs` in both modes.
    // Models: the inference servers, the loaded models, the costs.
    "create_llm_backend",
    "delete_llm_backend",
    "get_llm_cost_stats",
    "list_loaded_models",
    "set_default_llm_backend",
    "unload_loaded_model",
    "update_llm_backend",
    // MCP servers and the curated catalogue.
    "add_mcp_server",
    "fetch_mcp_curated",
    "get_mcp_server_detail",
    "get_mcp_server_raw_config",
    "list_mcp_resources",
    "list_mcp_servers",
    "remove_mcp_server",
    "restart_mcp_server",
    "set_mcp_server_approval",
    "test_mcp_connection",
    "test_mcp_live_server",
    "node_runtime_state",
    "update_mcp_server_config",
    // Agent packages installed from a folder, over
    // `/api/v1/agent-packages/`. The preview and the install name the folder
    // the window picked; the daemon listens on this machine and reads it.
    "get_agent_package_detail",
    "install_agent_package",
    "list_agent_packages",
    "preview_agent_package",
    "uninstall_agent_package",
    // Connections, the folders added as connections, and the mailbox
    // connection: the providers known from the address, and the addition,
    // which logs in before it stores.
    "add_folder_connection",
    "add_imap_mailbox",
    "list_connections",
    "list_imap_providers",
    "remove_folder_connection",
    // The report of the migration of the former projects.
    "get_project_migration_report",
    // Apollia as an MCP server, over `/api/v1/mcp/exposed`.
    "list_mcp_exposable",
    "set_mcp_exposed",
    // Notifications and the activity feed.
    "create_notification_channel",
    "delete_notification_channel",
    "get_notification_events",
    "get_notification_logs",
    "list_notification_channels",
    "list_runtime_activity",
    "set_notification_events",
    "test_notification_channel",
    "update_notification_channel",
    // Triggers.
    "create_trigger",
    "delete_trigger",
    "fire_trigger",
    "get_trigger_definition",
    "get_trigger_logs",
    "list_triggers",
    "set_trigger_enabled",
    "update_trigger",
    // Tools, observability, traces and messages between agents.
    "clear_plan_cache",
    "describe_tool",
    "get_active_hooks",
    "get_audit_stats",
    "get_llm_daily_costs",
    "get_plan_cache_stats",
    "get_task_trace",
    "list_agent_messages",
    "list_tools",
    "verify_audit_run",
    // The sheet of an agent: the agent resolved, its model fallback, its
    // governance history.
    "get_agent_definition",
    "get_agent_model_fallback",
    "list_agent_changes",
    // The adjustment of an agent on its sheet: its overrides, the return to
    // defaults, its model and the presets to choose from.
    "clear_agent_model",
    "list_model_presets",
    "remove_agent_override",
    "reset_agent_overrides",
    "set_agent_model",
    "set_agent_override",
    // The export of an agent from its sheet: the runtime builds and signs the
    // archive, this process writes the file the user chose.
    "export_agent",
    // The user profiles and their facts, over `/api/v1/profiles`.
    "create_profile",
    "delete_profile",
    "delete_profile_fact",
    "get_profile_by_id",
    "list_profiles",
    "reset_profile",
    "set_default_profile",
    "set_profile_fact",
    "update_profile",
    // The credentials of the native tools, over `/api/v1/tool-credentials`.
    "governance_delete_credential",
    "governance_list_credentials",
    "governance_set_credential",
    "governance_test_credential",
    // Memory, read side: the namespaces, their entries, the search, the
    // entries recalled during a turn, the statistics of a conversation, the
    // embeddings model it searches with.
    "get_conversation_stats",
    "get_injected_memory_entries",
    "get_memory_embeddings_model",
    "list_memory_entries",
    "list_memory_namespaces",
    "search_memory",
    // Models: the download of a whole variant from the Hub.
    "start_variant_download",
    // Models: the signed cards of the catalogue, read when it opens.
    "list_model_cards",
    // Named settings of `apollia.toml`, read with their source and written
    // key by key: the developer mode.
    "get_resolved_setting",
    "set_setting",
    // The models folder in force, read from the same `apollia.toml`.
    "get_models_dir",
    // Plan mode: the default, the switch, the plan and its history, the soft
    // gate of a chat session and the gate of an orchestrated run.
    "approve_plan",
    "get_chat_plan",
    "get_plan_mode_default",
    "list_plan_mutations",
    "reject_plan",
    "set_plan_mode",
    "submit_plan_decision",
    // Memory upkeep, over `/api/v1/memory/namespaces/{namespace}/...`. The
    // export and import files stay on the machine of the window; the daemon,
    // which owns the memory, sends and receives their content.
    "clear_memory",
    "delete_memory_entry",
    "memory_export_namespace",
    "memory_import_namespace",
    "purge_memory",
    // The companion: its preference and its guide session, over
    // `/api/v1/companion/...`. Its per-route help text is a table compiled
    // into the window and needs no route.
    "create_companion_session",
    "get_companion_context",
    "get_companion_enabled",
    "set_companion_enabled",
    // The transcription history, over `/api/v1/stt/transcriptions`, and the
    // transcription of a file: the file is read on the machine of the window,
    // the daemon transcribes its content and keeps the row.
    "delete_transcription",
    "list_transcriptions",
    "transcribe_file",
    // The onboarding, over `/api/v1/onboarding`: its state and phases, the
    // profile chosen, the conversation with its agent and its completion.
    "advance_onboarding_phase",
    "check_onboarding_finalized",
    "dismiss_onboarding",
    "finalize_onboarding_chat",
    "get_onboarding_state",
    "resume_onboarding",
    "set_onboarding_profile",
    "trigger_onboarding",
    // Resets and erasures, over `/api/v1/onboarding/*`, `/api/v1/memory/wipe`,
    // `/api/v1/logs/clear` and `/api/v1/factory-reset`. The flag, the memory,
    // the logs and the data directory they erase are the daemon's: the
    // window's process writes none of them.
    "clear_all_memories",
    "clear_logs",
    "factory_reset",
    "mark_onboarded",
    "reset_onboarding",
    // Permissions: the rules the onboarding agent proposed, a rule written
    // from an approval card, and the answer to a request to touch a file.
    "add_permission_prefix_rule",
    "apply_proposed_permission_rule",
    "dismiss_proposed_permission_rule",
    "list_proposed_permission_rules",
    "respond_hitl_filesystem",
    // Tool governance: the tools, their configuration, the permission rules
    // and the audit, over `/api/v1/governance`.
    "governance_get_tool_config",
    "governance_list_audit",
    "governance_list_permission_rules",
    "governance_list_tools",
    "governance_revoke_all_rules",
    "governance_revoke_permission_rule",
    "governance_set_tool_config",
    "governance_set_tool_enabled",
    // Accounts of the native connectors and their Drive folder, under
    // `/api/v1/connectors/`. The window keeps the loopback listener and the
    // browser of the consent; the daemon starts it, completes it and stores
    // the token.
    "oauth_complete_flow",
    "oauth_disconnect",
    "oauth_get_status",
    "oauth_list_drive_folders",
    "oauth_reset_drive_folder",
    "oauth_set_drive_folder",
    "oauth_start_flow",
    // The OAuth clients of the connectors, over `/api/v1/oauth/clients`. The
    // import reads the file the operator picked in this window and sends its
    // text, so the daemon never needs a path of this machine.
    "oauth_import_client_json",
    "oauth_list_client_ids",
    "oauth_set_api_key",
    "oauth_set_client_id",
    "oauth_set_client_secret",
    "oauth_test_client",
    // Models: the hardware, the search, a model's card, the installed models
    // and the recommendation, over `/api/v1/llm`.
    "get_hardware_profile",
    "get_hf_model",
    "list_installed_models",
    "recommend_models",
    "search_hf_models",
    // The installation of the local AI at onboarding, over
    // `/api/v1/local-ai/...`: the daemon describes its machine, scans its own
    // folders, and copies and registers the models it will load.
    "get_ai_setup_info",
    "scan_for_gguf_models",
    "scan_for_whisper_models",
    "setup_local_llm",
    "setup_whisper_model",
    // MCP: the secret of a server, and the public registry joined with the
    // servers installed.
    "delete_mcp_secret",
    "fetch_mcp_registry",
    "store_mcp_secret",
    // Models: the removal of an installed model, refused while a backend or
    // the dictation engine of the runtime uses it.
    "delete_installed_model",
    // LLM engines: the list, the test, the reload. The last ping of each
    // engine stays in this window's memory, which [`run`] manages in both
    // modes.
    "list_llm_backends",
    "ping_llm_backend",
    "reload_llm",
    "reload_llm_from_db",
    // Dictation: its settings, the reload of its engine, its status and its
    // model files, over `/api/v1/stt/*`. The microphones stay those of this
    // machine.
    "get_stt_config",
    "get_stt_status",
    "list_stt_models",
    "reload_stt",
    "update_stt_config",
    // The artifacts of a conversation, over `/api/v1/artifacts` and
    // `/api/v1/sessions/{id}/artifacts`, in the daemon's `artifacts.db`.
    "delete_artifact",
    "list_artifacts",
    "save_artifact",
    "update_artifact",
    // The Observability view: the global timeline and the mailbox, read by
    // the runtime over `/api/v1/observability/timeline` and
    // `/api/v1/mailbox/messages`, and the tool audit over `/api/v1/audit`.
    "get_global_timeline",
    "get_tool_audit_trail",
    "list_mailbox_messages",
    // The assists around the chat, over `/api/v1/assists`: the rewrite of a
    // prompt, the next steps cards, an automation described in words, and
    // the examples offered for a connector just added.
    "meta_generate_capabilities_coaching",
    "meta_generate_next_steps",
    "meta_parse_automation",
    "meta_rewrite_input",
    // The system settings, in the daemon's `apollia.toml` and on the machine
    // that runs its tools: the observability policy over
    // `/api/v1/config/observability`, the Python interpreter over
    // `/api/v1/config/python-interpreter`, the security posture over
    // `/api/v1/security-posture`.
    "get_observability_config",
    "get_python_interpreter",
    "get_security_posture",
    "set_observability_config",
    "set_python_interpreter",
];

/// The commands that belong to the window, not to the runtime: they read no
/// state of the runtime and serve the attached mode as they are, each with the
/// reason it needs no route. `scripts/check_attached_coverage.py` reads the
/// reasons and refuses an entry without one.
pub(crate) const WINDOW_COMMANDS: &[(&str, &str)] = &[
    (
        "get_system_info",
        "describes this application, its version, and the machine that carries the window",
    ),
    (
        "app_restart",
        "relaunches this window, which attaches again at startup",
    ),
    (
        "quit_app",
        "closes this window and leaves the daemon serving",
    ),
    (
        "check_for_update",
        "asks the update endpoint about this application",
    ),
    (
        "install_update",
        "replaces this application, not the daemon",
    ),
    (
        "get_cli_status",
        "reads the command-line link this application installs",
    ),
    (
        "get_background_service",
        "reads the service the OS manager holds for this user, which the window attaches to at startup",
    ),
    (
        "set_background_service",
        "installs or removes the service of this user's session, then relaunches this window",
    ),
    (
        "install_cli",
        "links the command line bundled with this application",
    ),
    (
        "uninstall_cli",
        "removes the command-line link this application installed",
    ),
    (
        "link_preview",
        "fetches the card of a link the window shows",
    ),
    (
        "get_api_port",
        "reads the port this window resolved at startup, the daemon's when attached",
    ),
    (
        "list_audio_input_devices",
        "lists the microphones of this machine",
    ),
    (
        "start_tour_recording",
        "captures the microphone of this machine, the daemon transcribing what it heard",
    ),
    (
        "stop_tour_recording",
        "stops the capture of this machine and sends its audio to the daemon",
    ),
    (
        "open_logs_dir",
        "opens the folder of this window's own log file in the file manager of this machine",
    ),
    (
        "open_config_in_editor",
        "opens the configuration file in the editor of this machine",
    ),
    (
        "export_conversation",
        "writes the conversation to the file chosen in this window's save dialog",
    ),
    (
        "reveal_session_path",
        "shows a file in the file manager of this machine, the session folder read from the runtime",
    ),
    (
        "start_model_download",
        "downloads into the models folder of this machine with the download manager of this window, which also holds the variant downloads",
    ),
    (
        "get_hf_token",
        "reads the Hugging Face token from the secret store of this machine",
    ),
    (
        "set_hf_token",
        "writes the Hugging Face token to the secret store of this machine",
    ),
    (
        "clear_hf_token",
        "removes the Hugging Face token from the secret store of this machine",
    ),
    (
        "cancel_model_download",
        "cancels a download held by the download manager of this window",
    ),
    (
        "import_model_file",
        "copies a file or folder of this machine, picked in this window, into its models folder",
    ),
    (
        "automation_script",
        "drives this window in a development build",
    ),
    (
        "automation_capture",
        "captures this window in a development build",
    ),
    (
        "automation_resize",
        "resizes this window in a development build",
    ),
    (
        "automation_finish",
        "ends the run driving this window in a development build",
    ),
    (
        "list_mcp_enrichments",
        "reads the connector labels compiled into this application",
    ),
    (
        "mcp_oauth_discover",
        "reads the sign-in metadata an MCP server publishes, for the wizard this window shows",
    ),
    (
        "mcp_oauth_resolve_client_id",
        "resolves the client id of the sign-in this window runs, from its own environment and build",
    ),
    (
        "mcp_oauth_login",
        "opens the browser of this machine and awaits the sign-in on its loopback; the token goes to the secret store the daemon reads",
    ),
];

/// Encodes one path segment of a daemon route.
pub(crate) fn segment(value: &str) -> std::borrow::Cow<'_, str> {
    urlencoding::encode(value)
}

/// Where the runtime this application drives lives.
#[derive(Clone)]
pub(crate) enum RuntimeLink {
    /// In this process.
    Embedded(Box<RuntimeHandle>),
    /// In another process, reached over HTTP.
    Attached(AttachedClient),
}

/// Which arm of the seam the application runs, for the gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkMode {
    /// The runtime runs in this process: every command is served.
    Embedded,
    /// The runtime is a daemon: only routed commands are served.
    Attached,
}

/// Why an attached command did not complete.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum AttachedError {
    /// The command reads state that only exists when the runtime runs in this
    /// process, and no HTTP route serves it yet.
    #[error("{command} is unavailable while Apollia runs as a service")]
    NotRouted {
        /// The command refused.
        command: String,
    },
    /// The HTTP stack could not be initialised.
    #[error("the HTTP client cannot be initialised")]
    Client(#[source] reqwest::Error),
    /// The runtime did not answer: stopped, restarting, or past the deadline.
    #[error("the Apollia service does not answer")]
    Unreachable(#[source] reqwest::Error),
    /// The runtime answered with an error status.
    #[error("the Apollia service refused the request ({status}): {message}")]
    Refused {
        /// The HTTP status.
        status: u16,
        /// The runtime's own message, empty when its body held none.
        message: String,
    },
    /// The runtime answered a body this application cannot read.
    #[error("the Apollia service answered an unreadable body")]
    Body(#[source] apollia_core::net::ReadCappedError),
}

impl AttachedError {
    /// The text the interface shows, in its language.
    pub(crate) fn for_display(&self, locale: crate::i18n::UiLocale) -> String {
        match self {
            Self::NotRouted { command } => crate::i18n::attached_unavailable(locale, command),
            Self::Refused { message, .. } if !message.is_empty() => message.clone(),
            other => other.to_string(),
        }
    }
}

impl From<AttachedError> for String {
    fn from(error: AttachedError) -> Self {
        error.for_display(crate::i18n::locale())
    }
}

/// Whether `command` may run in `mode`.
///
/// # Errors
///
/// [`AttachedError::NotRouted`] for a command outside [`ROUTED_COMMANDS`] and
/// [`WINDOW_COMMANDS`] while the runtime is a daemon.
pub(crate) fn gate(mode: LinkMode, command: &str) -> Result<(), AttachedError> {
    if mode == LinkMode::Embedded
        || ROUTED_COMMANDS.contains(&command)
        || WINDOW_COMMANDS.iter().any(|(name, _)| *name == command)
    {
        return Ok(());
    }
    Err(AttachedError::NotRouted {
        command: command.to_owned(),
    })
}

/// Wraps the generated command handler with [`gate`]: a refused command is
/// answered with its typed error and its body never runs.
pub(crate) fn gated<R, H>(
    mode: LinkMode,
    handler: H,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
where
    R: tauri::Runtime,
    H: Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
{
    move |invoke| match gate(mode, invoke.message.command()) {
        Ok(()) => handler(invoke),
        Err(error) => {
            tracing::event!(
                tracing::Level::DEBUG,
                command = %invoke.message.command(),
                "desktop.attached.command_not_routed"
            );
            invoke.resolver.reject(String::from(error));
            true
        }
    }
}

/// Arms the dictation of this window against the daemon: the microphone is
/// the one of this machine, the engine and the history the daemon's, the
/// thresholds its configuration, read once when the window attaches. Nothing
/// is armed while the daemon has dictation off, as in the embedded mode.
async fn arm_dictation(
    app: tauri::AppHandle,
    client: AttachedClient,
    bus: EventBusSender,
    state: crate::commands::stt::SttFlowState,
) {
    let config: apollia_core::SttConfigRow = match client.get("/api/v1/stt/config").await {
        Ok(config) => config,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "desktop.attached.dictation_unarmed"
            );
            return;
        }
    };
    if !config.enabled {
        return;
    }
    let flow = crate::stt::flow::SttFlow::new(
        config,
        crate::stt::transcriber::Transcriber::Daemon(client),
        bus,
        app,
    );
    if let Ok(mut guard) = state.lock() {
        *guard = Some(std::sync::Arc::new(flow));
    }
}

/// Runs the application attached to the daemon serving `port`, and never
/// returns.
///
/// Nothing of the embedded runtime is built: no PyO3, no database, no
/// listener. The interface talks to the daemon through the routed commands,
/// receives its events through the relay, and learns it is gone through the
/// liveness probe. Quitting leaves the daemon running.
pub(crate) fn run(port: u16, token: String, context: tauri::Context<tauri::Wry>) -> ! {
    let client = match AttachedClient::new(format!("http://127.0.0.1:{port}"), token) {
        Ok(client) => client,
        Err(error) => {
            tracing::event!(
                tracing::Level::ERROR,
                error = %error,
                "desktop.attached.client_failed"
            );
            std::process::exit(1)
        }
    };
    let (bus, _) = tokio::sync::broadcast::channel(RELAY_BUS_CAPACITY);
    let bus: EventBusSender = bus;

    let setup_client = client.clone();
    let setup_bus = bus.clone();
    let dictation: crate::commands::stt::SttFlowState =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(RuntimeLink::Attached(client))
        .manage(crate::startup::ApiPort(port))
        .manage(dictation.clone())
        .manage(
            std::sync::Arc::new(tokio::sync::Mutex::new(apollia_llm::DownloadManager::new()))
                as crate::commands::model_hub::SharedDownloadManager,
        )
        .manage(std::sync::Arc::new(apollia_llm::HfModelTypeCache::new()))
        .manage(crate::commands::llm::LlmPingCache::default())
        .on_menu_event(|app, event| {
            if event.id().as_ref() == crate::tray::MENU_QUIT_APP {
                crate::tray::initiate_quit(app);
            }
        })
        .setup(move |app| {
            crate::i18n::attach_listener(app);
            crate::tray::setup_tray(app)?;
            #[cfg(target_os = "macos")]
            crate::tray::setup_app_menu(app)?;

            crate::events::spawn_event_bridge(app.handle().clone(), setup_bus.clone());
            relay::spawn_event_relay(setup_client.clone(), setup_bus.clone());
            relay::spawn_liveness(app.handle().clone(), setup_client.clone());
            tauri::async_runtime::spawn(arm_dictation(
                app.handle().clone(),
                setup_client.clone(),
                setup_bus.clone(),
                dictation.clone(),
            ));

            let main_window = app
                .get_webview_window("main")
                .ok_or("main window not found in tauri.conf.json")?;
            let window_for_close = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    match crate::tray::close_button_action() {
                        crate::tray::CloseAction::HideToTray => {
                            let _ = window_for_close.hide();
                        }
                        crate::tray::CloseAction::Quit => {
                            crate::tray::initiate_quit(window_for_close.app_handle());
                        }
                    }
                }
            });
            tracing::event!(tracing::Level::INFO, port, "desktop.attached.started");
            Ok(())
        })
        .invoke_handler(gated(LinkMode::Attached, crate::handlers::commands()))
        .build(context);

    match app {
        Ok(app) => {
            let _ = app.run_return(|_, _| {});
            std::process::exit(0)
        }
        Err(error) => {
            tracing::event!(
                tracing::Level::ERROR,
                error = %error,
                "desktop.attached.build_failed"
            );
            std::process::exit(1)
        }
    }
}

#[cfg(test)]
mod tests;
