use super::*;
use apollia_core::events::{subscribe_resilient, ResilientReceiver};

/// Detect the GPU and spawn the sidecar runner for the local LLM and STT
/// backends. Returns `None` (logged) when the runner cannot start; cloud
/// backends keep working without it.
pub(in crate::supervisor) async fn spawn_runner_supervisor(
) -> Option<crate::runner_supervisor::RunnerSupervisor> {
    use crate::runner_supervisor::{gpu_detection, RunnerSupervisor};

    let detected = gpu_detection::detect_gpu();
    tracing::info!(
        vendor = ?detected.vendor,
        model = %detected.model,
        backend = ?detected.recommended_backend,
        "supervisor.runner.spawning"
    );

    match RunnerSupervisor::start(detected.clone(), detected.recommended_backend).await {
        Ok(sup) => {
            info!("supervisor.runner.spawned");
            Some(sup)
        }
        Err(e) => {
            warn!(
                error = %e,
                detail = "local LLM and STT disabled",
                "supervisor.runner.spawn.failed"
            );
            None
        }
    }
}

/// Flatten the nested `timeout(api_server.start())` result into a single
/// `Result<APIServerHandle, SupervisorError>`, mapping a timeout to
/// [`SupervisorError::StartupTimeout`].
pub(in crate::supervisor) fn flatten_api_start(
    api_start: Result<Result<APIServerHandle, APIServerError>, tokio::time::error::Elapsed>,
    startup_timeout_secs: u64,
) -> Result<APIServerHandle, SupervisorError> {
    match api_start {
        Ok(Ok(handle)) => Ok(handle),
        Ok(Err(api_err)) => Err(SupervisorError::from(api_err)),
        Err(_elapsed) => Err(SupervisorError::StartupTimeout {
            actor: "api_server".to_string(),
            timeout_secs: startup_timeout_secs,
        }),
    }
}

/// Stop the already-started actors in reverse order when APIServer startup
/// fails (TriggerEngine → TaskRouter → ToolRegistry → AgentRegistry).
pub(in crate::supervisor) async fn rollback_startup_actors<B: ExecutionBackend>(
    trigger_engine: &TriggerEngineHandle,
    router_handle: &TaskRouterHandle<B>,
    tool_registry_handle: &ToolRegistryHandle,
    registry_handle: &AgentRegistryHandle,
) {
    trigger_engine.shutdown().await;
    router_handle.shutdown();
    // `ToolRegistryHandle::shutdown` consumes `self`; the handle is only a
    // cheap `Clone` over an mpsc sender, so cloning to send the Shutdown
    // message is semantically equivalent to the original owned-move path.
    tool_registry_handle.clone().shutdown().await;
    registry_handle.shutdown();
}

/// Emit [`RuntimeEvent::OnboardingRequired`] when user memory is empty (first
/// launch). Non-blocking and best-effort: lock/read failures are logged.
pub(in crate::supervisor) fn emit_onboarding_if_needed(
    user_memory: Option<
        &std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>,
    >,
    event_sender: &EventBusSender,
) {
    let Some(um) = user_memory else {
        return;
    };
    let repo = match um.lock() {
        Ok(repo) => repo,
        Err(e) => {
            warn!(
                error = %e,
                detail = "skipping the onboarding check",
                "memory.user.lock.poisoned"
            );
            return;
        }
    };
    match repo.is_empty() {
        Ok(true) => {
            let _ = event_sender.send(RuntimeEvent::OnboardingRequired);
            info!("onboarding.required");
        }
        Ok(false) => info!("onboarding.skipped"),
        Err(e) => warn!(error = %e, "onboarding.check.failed"),
    }
}

/// Load and parse trigger definitions from the repository, skipping (with a
/// warning) any row that fails to convert. Propagates only the list failure.
pub(in crate::supervisor) fn load_trigger_definitions(
    repo: &TriggerDefinitionRepository,
) -> Result<Vec<apollia_triggers::TriggerDefinition>, SupervisorError> {
    let rows = repo.list().map_err(|e| SupervisorError::ActorStartFailed {
        actor: "trigger_engine".to_string(),
        reason: format!("failed to list trigger definitions: {e}"),
    })?;
    let mut defs = Vec::with_capacity(rows.len());
    for row in rows {
        match apollia_triggers::TriggerDefinition::try_from(row) {
            Ok(def) => defs.push(def),
            Err(e) => warn!(error = %e, "trigger.definition.invalid"),
        }
    }
    Ok(defs)
}

/// One-shot migration of MCP servers from `mcp.toml` into `mcp.db` when the
/// database is empty. No-op when the DB already has servers or the TOML is
/// absent/empty. Errors are logged, never fatal.
pub(in crate::supervisor) fn migrate_mcp_from_toml(
    repo: &McpServerRepository,
    mcp_config_path: &std::path::Path,
) {
    let existing = match repo.list() {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "mcp.migration.check.failed");
            return;
        }
    };
    if !existing.is_empty() {
        return;
    }
    let toml_config = match McpConfig::load(mcp_config_path) {
        Ok(c) if !c.servers.is_empty() => c,
        _ => return,
    };
    match repo.import_from_toml(toml_config.servers) {
        Ok(n) => info!(count = n, "mcp.migration.completed"),
        Err(e) => warn!(error = %e, "mcp.migration.failed"),
    }
}

/// Start the MCP client manager (always, even when the server list is empty)
/// and spawn the legacy `mcp.toml` config watcher when that file still exists.
pub(in crate::supervisor) async fn start_mcp_manager(
    server_configs: Vec<apollia_mcp::config::McpServerConfig>,
    tool_registry_handle: &ToolRegistryHandle,
    event_sender: &EventBusSender,
    mcp_config_path: &std::path::Path,
    loading_mode: LoadingMode,
) -> Option<McpClientManagerHandle> {
    // Always start the McpClientManager actor, even when the server list is
    // empty. Without this, the desktop "Add MCP server" flow cannot register a
    // first server: every write route checks require_mcp_handle and returns
    // 503 "MCP is not configured" until at least one server exists in mcp.db at
    // boot, a chicken-and-egg trap for first-time users.
    let server_count = server_configs.len();
    match McpClientManagerHandle::start(
        server_configs,
        tool_registry_handle,
        Some(event_sender.clone()),
        None,
        loading_mode,
    )
    .await
    {
        Ok(handle) => {
            let status = handle.status().await;
            let total_tools: usize = status.iter().map(|s| s.tools_count).sum();
            info!(
                servers = server_count,
                connected = status.len(),
                tools = total_tools,
                "mcp.bootstrap.completed"
            );
            // Start MCP config watcher only when the legacy mcp.toml exists.
            // Config is now stored in mcp.db (SQLite), mcp.toml is a
            // deprecated migration path kept for back-compat.
            if mcp_config_path.exists() {
                apollia_triggers::handlers::config_watch::McpConfigWatcher::spawn(
                    mcp_config_path.to_path_buf(),
                    handle.clone(),
                );
            }
            Some(handle)
        }
        Err(e) => {
            warn!(error = %e, detail = "continuing without MCP", "mcp.bootstrap.failed");
            None
        }
    }
}

/// Watch for a coordinated-shutdown signal on the EventBus.
///
/// This is a standalone async function (not a method on Supervisor) because
/// the Supervisor is consumed by `start()`. The caller runs `watch()` as a
/// background task after obtaining handles.
///
/// Listens for `ShutdownRequested` and returns when it arrives. It does not
/// restart actors: the runtime is fail-fast at startup then degrades on a
/// post-startup crash (see the module docs).
pub async fn watch(event_sender: &EventBusSender) -> Result<(), SupervisorError> {
    let mut rx = subscribe_resilient(event_sender, "supervisor.watch");

    while let Some(event) = rx.recv().await {
        // Non-terminal events are not acted on: no restart-on-crash.
        if matches!(event, RuntimeEvent::ShutdownRequested) {
            info!("supervisor.watch.shutdown");
            return Ok(());
        }
    }
    info!("supervisor.watch.bus.closed");
    Ok(())
}

/// Drain events from a receiver until `AllReady` is seen or timeout expires.
pub(in crate::supervisor) async fn drain_until_all_ready(
    rx: &mut ResilientReceiver,
    timeout: Duration,
) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Some(RuntimeEvent::AllReady) => return,
                    Some(_) => continue,
                    None => return,
                }
            }
            _ = tokio::time::sleep_until(deadline) => return,
        }
    }
}

/// Marker key stored under `__user__.notifications_seeded_desktop` (with the
/// `__` internal prefix added by `user_memory::set_internal`).
///
/// Once set, prevents a subsequent boot from re-seeding the default desktop
/// channel, even if the operator deletes the seeded channel manually.
pub(in crate::supervisor) const SEEDED_DESKTOP_CHANNEL_MARKER: &str =
    "notifications_seeded_desktop";

/// On first boot with a usable [`UserMemoryRepository`] and an empty
/// notifications database, insert a sensible default desktop channel so the
/// operator does not start from a blank Notifications page.
///
/// The function is fully idempotent:
/// - if the seed marker is already set in memory, return without touching anything;
/// - if the notification repository already has at least one channel, set the
///   marker anyway (so further deletions never trigger a re-seed) and return;
/// - otherwise, insert the channel, then set the marker.
///
/// All failures are best-effort and logged at `warn!`, they must never block
/// supervisor startup.
pub(in crate::supervisor) fn seed_default_desktop_channel_if_needed(
    notif_repo: &NotificationConfigRepository,
    user_memory: Option<
        &std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>,
    >,
) {
    let Some(um_arc) = user_memory else {
        // No user memory available, can't track the marker safely. Skip.
        return;
    };
    let um = um_arc.lock().unwrap_or_else(|e| e.into_inner());

    // 1. Check marker.
    match um.get_internal(SEEDED_DESKTOP_CHANNEL_MARKER) {
        Ok(Some(_)) => return, // already seeded, leave the user's setup alone
        Ok(None) => {}
        Err(e) => {
            warn!(error = %e, "notification.seed.marker.read.failed");
            return;
        }
    }

    // 2. If the repository is not empty, just set the marker (legacy user with
    //    existing channels, record that we've considered seeding once).
    let channels = match notif_repo.list_channels() {
        Ok(rows) => rows,
        Err(e) => {
            warn!(error = %e, "notification.seed.channels.list.failed");
            return;
        }
    };
    if !channels.is_empty() {
        let _ = um.set_internal(SEEDED_DESKTOP_CHANNEL_MARKER, "true");
        return;
    }

    // 3. Compose label from the user's profile name (if available).
    let name: Option<String> = um.get("name").ok().flatten().and_then(|entry| {
        let trimmed = entry.value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let label = match name {
        Some(n) => format!("Bureau de {n}"),
        None => "Bureau".to_string(),
    };

    // 4. Insert the channel.
    let row = apollia_notifications::NotificationChannelRow {
        id: "desktop-default".to_string(),
        label: Some(label),
        channel_type: "desktop".to_string(),
        enabled: true,
        config_json: serde_json::json!({}),
        events_json: None,
        min_interval_seconds: 0,
        created_at: String::new(),
        updated_at: String::new(),
    };
    if let Err(e) = notif_repo.insert_channel(&row) {
        warn!(error = %e, "notification.seed.insert.failed");
        return;
    }

    // 5. Set the marker so we never re-seed (even after manual deletion).
    if let Err(e) = um.set_internal(SEEDED_DESKTOP_CHANNEL_MARKER, "true") {
        warn!(
            error = %e,
            detail = "channel inserted,
            a re-seed is possible on the next boot",
            "notification.seed.marker.write.failed"
        );
    }
    info!("notification.seed.completed");
}
