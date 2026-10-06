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
mod error;
mod launch;
mod proxy;
mod relay;

use apollia_runtime::embedded::RuntimeHandle;

pub(crate) use client::AttachedClient;
pub(crate) use error::AttachedError;
pub(crate) use launch::run;
pub(crate) use proxy::segment;

/// The commands that serve the attached mode. Every other registered command
/// answers [`AttachedError::NotRouted`] while the runtime is a daemon.
///
/// `start_variant_download` needs no route over HTTP: it never reads the
/// runtime, only the Hub, the models folder and its index, which the daemon
/// shares since it listens on this machine. [`run`] manages the download
/// manager it holds. `list_model_cards` needs none either: it reads the model
/// registry, the data directory and its `apollia.toml`, never the runtime.
/// `get_resolved_setting` and `set_setting` need none: they read and write the
/// `apollia.toml` of the data directory, never the runtime (`set_setting` then shows its write to the runtime, in either
/// mode, for the journal), and neither does `get_models_dir`.
pub(crate) const ROUTED_COMMANDS: &[&str] = &[
    "chat_session_metrics",
    "create_chat_session",
    "get_chat_session",
    "list_chat_sessions",
    "pause_chat_session",
    "send_chat_message",
    // The images of the chat: what the model reads, and the stored images.
    "get_chat_model_capabilities",
    "read_chat_image",
    // The settings a conversation holds for itself, read and replaced.
    "get_chat_session_settings",
    "set_chat_session_settings",
    // The conversations as the list manages them: configure, rename, erase.
    "delete_chat_session",
    "generate_chat_session_name",
    "rename_chat_session",
    "update_chat_session",
    // The trash of the conversations: send one there, list it, restore one.
    "list_trashed_chat_sessions",
    "restore_chat_session",
    "trash_chat_session",
    // The folders of the conversation list, and the search in the messages.
    "create_chat_folder",
    "delete_chat_folder",
    "list_chat_folders",
    "move_chat_session_to_folder",
    "rename_chat_folder",
    "search_chat_messages",
    // The agents and the models the selector of a conversation offers.
    "list_chat_targets",
    // The model calls behind the total of one answer.
    "list_turn_llm_calls",
    // A turn played again, a decision on a tool call, the worker skills.
    "authorize_chat_tool",
    "edit_and_resend_chat_message",
    "list_a2a_skills",
    "regenerate_chat_response",
    // The actions under an answer: branch off, continue the last answer,
    // rewrite an answer, remove a message.
    "continue_chat_response",
    "delete_chat_message",
    "edit_chat_response",
    "fork_chat_session",
    "get_chat_continuation",
    // Agents: the registry and the uninstall of an agent.
    "agent_status_snapshot",
    "list_agents",
    "uninstall_agent",
    // Agents: the update, restart included, over
    // `PUT /api/v1/agents/installed/{name}`.
    "update_agent",
    // Tasks: the list and the detail.
    "get_task_timeline",
    "list_tasks",
    // Tasks: the delegation tree, over `/api/v1/tasks/{id}/delegation-tree`.
    "get_delegation_tree",
    // Approvals: the task pauses waiting for a decision.
    "list_pending_approvals",
    // Every decision waiting for the user in one list, and the answer to one,
    // over `/api/v1/pending-decisions`.
    "decide_pending_decision",
    "list_pending_decisions",
    // The questions an agent asked the user.
    "list_pending_user_inputs",
    "respond_user_input",
    // Commands whose whole body is a call to a runtime route, served through
    // the methods of `proxy.rs` in both modes.
    // Models: the loaded models, the costs.
    "list_loaded_models",
    // Loading a model by hand: the estimate, the load, the last one loaded.
    "estimate_model_load",
    "get_last_loaded_model",
    "load_model",
    "reload_last_loaded_model",
    // The lease of an open conversation on its model, and the pin.
    "list_model_pins",
    "open_model_lease",
    "release_model_lease",
    "set_model_pin",
    // The AI screen: the embeddings model of search.
    "set_embeddings_default",
    "unload_loaded_model",
    // The servers of the AI screen: test, save, list, change, forget, and the
    // default model.
    "delete_llm_server",
    "get_default_model",
    "list_llm_server_models",
    "list_llm_servers",
    "save_llm_server",
    "set_default_model",
    "test_llm_server",
    "update_llm_server",
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
    // The registry of sources, what an agent may consult once checked, and
    // where the index of each stands.
    "add_source",
    "get_source_index",
    "list_source_folder_kinds",
    "list_sources",
    "remove_source",
    // The report of the migration of the former projects.
    "get_project_migration_report",
    // The local server of the developer screen: its state, its start and its
    // stop, over `/api/v1/server`.
    "get_local_server",
    "start_local_server",
    "stop_local_server",
    // Apollia as an MCP server, over `/api/v1/mcp/exposed`.
    "list_mcp_exposable",
    "set_mcp_exposed",
    // Workflows: the list, one workflow, its runs, a launch and a stop, over
    // `/api/v1/workflows` and `/api/v1/workflow-runs`.
    "cancel_workflow_run",
    "get_workflow",
    "get_workflow_run",
    "launch_workflow",
    "list_workflow_runs",
    "list_workflows",
    // The launch of a batch: the sources of the registry, the preview of a
    // workbook, the sensitive accesses of the agent and the launch, over
    // `/api/v1/sources` and `/api/v1/batches`.
    "launch_batch",
    "list_batch_accesses",
    "list_batch_sources",
    // The view of a batch and the history of the batches: one batch with its
    // items, the list, pause, resume, stop and retry, the export, and the
    // decision an item waits on, over `/api/v1/batches` and
    // `/api/v1/pending-decisions`.
    "batch_item_decision",
    "control_batch",
    "decide_batch_item",
    "export_batch",
    "get_batch",
    "list_batches",
    "preview_batch_source",
    // Notifications.
    "create_notification_channel",
    "delete_notification_channel",
    "get_notification_events",
    "get_notification_logs",
    "list_notification_channels",
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
    // The sheet of an agent: the agent resolved, its model fallback, its
    // governance history.
    "get_agent_definition",
    "get_agent_model_fallback",
    "list_agent_changes",
    "get_journal_entry",
    "list_journal_entries",
    "export_journal",
    "read_journal_export",
    "verify_journal",
    // The adjustment of an agent on its sheet: its overrides, the return to
    // defaults, its model and the presets to choose from.
    "clear_agent_model",
    "list_model_presets",
    "remove_agent_override",
    "reset_agent_overrides",
    "set_agent_model",
    "set_agent_override",
    // The creation of an agent without code: the validated sheet, sent to
    // the runtime that stores it.
    "create_agent_definition",
    // The export of an agent from its sheet: the runtime builds and signs the
    // archive, this process writes the file the user chose.
    "export_agent",
    // The preview of a `.apo` package before it is installed or updated, its
    // installation, and the trust of its publisher, over `/api/v1/packages`.
    "install_package_file",
    "preview_package_file",
    "trust_package_publisher",
    // The example packages the Agents screen proposes.
    "install_example_package",
    "list_example_packages",
    "preview_example_package",
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
    // Models: the embeddings model recommended for this machine, and the one
    // search by meaning uses, over `/api/v1/llm/embeddings`.
    "get_embeddings_recommendation",
    "set_default_embeddings_model",
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
    // `/api/v1/companion/...`.
    "create_companion_session",
    "get_companion_enabled",
    "set_companion_enabled",
    // The transcription history, over `/api/v1/stt/transcriptions`, and the
    // transcription of a file: the file is read on the machine of the window,
    // the daemon transcribes its content and keeps the row.
    "delete_transcription",
    "list_transcriptions",
    "transcribe_file",
    // The onboarding, over `/api/v1/onboarding`: its state and phases, the
    // conversation with its agent and its completion.
    "advance_onboarding_phase",
    "check_onboarding_finalized",
    "dismiss_onboarding",
    "finalize_onboarding_chat",
    "get_onboarding_state",
    "resume_onboarding",
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
    // Permissions: a rule written from an approval card.
    "add_permission_prefix_rule",
    // Tool governance: the tools, their configuration and the permission
    // rules, over `/api/v1/governance`.
    "governance_get_tool_config",
    "governance_list_permission_rules",
    "governance_list_tools",
    "governance_revoke_all_rules",
    "governance_revoke_permission_rule",
    "governance_set_tool_config",
    "governance_set_tool_enabled",
    "governance_state",
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
    "setup_local_llm",
    // MCP: the secret of a server, and the public registry joined with the
    // servers installed.
    "delete_mcp_secret",
    "fetch_mcp_registry",
    "store_mcp_secret",
    // Models: the removal of an installed model, refused while a backend or
    // the dictation engine of the runtime uses it.
    "delete_installed_model",
    // Models: the pin of an installed model, written to the index of the
    // models folder, which the daemon shares since it listens on this machine.
    "set_installed_model_pinned",
    // LLM engines: the list and the reload.
    "list_llm_backends",
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
    // `/api/v1/security-posture`, the engines of the machine with their
    // versions over `/api/v1/engines`, the channel they follow over
    // `/api/v1/engines/settings`, their update check over
    // `/api/v1/engines/check`, and the resources the machine is using over
    // `/api/v1/llm/hardware/usage`.
    "check_engine_updates",
    "get_observability_config",
    "get_python_interpreter",
    "get_resource_usage",
    "get_security_posture",
    "list_engines",
    "set_engine_channel",
    "set_observability_config",
    "set_python_interpreter",
    // The editor of `apollia.toml`, over `/api/v1/config`: the file with its
    // faults and the keys the policy fixes, the verdict on a text, its save,
    // and the default text.
    "default_config_text",
    "read_config_file",
    "save_config_text",
    "validate_config_text",
    // The organisation policy the runtime applies, read only, over
    // `/api/v1/governance/policy`.
    "get_org_policy",
];

/// The commands that belong to the window, not to the runtime: they read no
/// state of the runtime and serve the attached mode as they are, each with the
/// reason it needs no route. `scripts/check_attached_coverage.py` reads the
/// reasons and refuses an entry without one.
pub(crate) const WINDOW_COMMANDS: &[(&str, &str)] = &[
    (
        "get_documentation_page",
        "reads a page of the documentation archive compiled into this application",
    ),
    (
        "get_system_info",
        "describes this application, its version, and the machine that carries the window",
    ),
    (
        "app_restart",
        "relaunches this window, which attaches again at startup",
    ),
    (
        "restart_apollia",
        "stops the daemon through its own route, then relaunches this window, which starts the service again and attaches",
    ),
    (
        "open_hardware_window",
        "opens or brings forward a window of this application; what it shows is read by routed commands",
    ),
    ("print_journal", "opens the print dialog of the system on this window"),
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
    ("copy_example_dataset", "writes files this application carries into a folder the user chose"),
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
        "take_opened_package_files",
        "hands over the package files the system opened in this window",
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
        "export_conversation",
        "writes the conversation to the file chosen in this window's save dialog",
    ),
    (
        "reveal_session_path",
        "shows a file in the file manager of this machine, the session folder read from the runtime",
    ),
    (
        "reveal_installed_model",
        "shows a model of the models folder of this machine in its file manager",
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

#[cfg(test)]
mod tests;
