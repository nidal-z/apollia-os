//! Config loading, agent rewiring and the shutdown wait.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::{ResilientReceiver, RuntimeEvent};
use apollia_runtime::api::routes_agents::AgentBackendFactory;
use apollia_runtime::coordinator::{DynBackend, ExecutionCoordinator};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::on_demand::{BackendLoader, OnDemandBackend};

use super::factory::find_config_file;
use super::StartError;

/// Locates and parses `apollia.toml` if present, validating the `[tools]`,
/// `[mcp]`, and `[hooks]` sections. Returns the parsed config and its path, or
/// `(None, None)` when no config file is found (defaults are then used by the
/// caller).
pub(super) fn load_start_config(
) -> Result<(Option<crate::config::ApolliaCConfig>, Option<PathBuf>), StartError> {
    let Some(path) = find_config_file() else {
        tracing::info!(detail = "starting with the defaults", "config.file.absent");
        return Ok((None, None));
    };
    tracing::info!(config = %path.display(), "config.loading");
    let cfg = crate::config::parse_apollia_toml(&path).map_err(|e| StartError::Config {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    if let Some(tools) = cfg.tools.as_ref() {
        tools.validate().map_err(|e| StartError::Config {
            path: path.clone(),
            reason: e.to_string(),
        })?;
    }
    if let Some(mcp) = cfg.mcp.as_ref() {
        mcp.validate().map_err(|e| StartError::Config {
            path: path.clone(),
            reason: e.to_string(),
        })?;
    }
    if let Some(hooks) = cfg.hooks.as_ref() {
        hooks.validate().map_err(|e| StartError::Config {
            path: path.clone(),
            reason: e.to_string(),
        })?;
    }
    Ok((Some(cfg), Some(path)))
}

/// Populates a shared `OnceLock` from an optional value, ignoring the result
/// (the lock is set at most once; a second attempt is a harmless no-op).
pub(super) fn set_lock_if_some<T>(lock: &std::sync::OnceLock<T>, value: Option<T>) {
    if let Some(v) = value {
        let _ = lock.set(v);
    }
}

/// Rewire every auto-loaded enabled agent once the factory's OnceLocks are
/// populated, so that no task can ever reach a backend built before them.
///
/// The coordinator gets a fresh [`OnDemandBackend`]: nothing is imported
/// here, the module of the agent is imported by its first task and released
/// after `idle_unload` without a call. Calling `register_coordinator` is
/// idempotent: the router replaces the existing entry in its `HashMap`.
pub(super) async fn rewire_auto_loaded_agents(
    repo: &apollia_tools::AgentRepository,
    factory: &Arc<dyn AgentBackendFactory>,
    handles: &apollia_runtime::supervisor::SupervisorHandles<DynBackend>,
    event_sender: &EventBusSender,
    idle_unload: std::time::Duration,
) {
    let installed = match repo.list_enabled() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                detail = "the rewire is skipped",
                "agent.rewire.list.failed"
            );
            return;
        }
    };
    let mut rewired = 0usize;
    for agent in installed {
        if !agent.enabled {
            continue;
        }
        let agent_id = match handles
            .registry_handle
            .find_by_name(&agent.manifest.name)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::debug!(
                    name = %agent.manifest.name,
                    reason = "the supervisor skipped the agent",
                    "agent.rewire.absent"
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    name = %agent.manifest.name,
                    error = %e,
                    "agent.rewire.lookup.failed"
                );
                continue;
            }
        };
        let loader: BackendLoader = {
            let factory = Arc::clone(factory);
            let install_path = agent.install_path.clone();
            let manifest = agent.manifest.clone();
            Arc::new(move || factory.create_for_agent(&install_path, &manifest))
        };
        let dyn_backend = DynBackend::new(OnDemandBackend::spawn(
            &agent.manifest.name,
            loader,
            idle_unload,
        ));
        let mut coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            agent.manifest.max_concurrent_tasks,
            event_sender.clone(),
            dyn_backend,
        )
        .with_agent_name(agent.manifest.name.clone());
        if let Some(ref task_repo) = handles.task_repository {
            coordinator = coordinator.with_task_repository(
                Arc::clone(task_repo),
                apollia_core::ObservabilityConfig::default(),
            );
        }
        match handles
            .router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
        {
            Ok(()) => {
                rewired += 1;
                tracing::debug!(
                    agent = %agent.manifest.name,
                    "agent.rewire.done"
                );
            }
            Err(e) => {
                tracing::warn!(
                    agent = %agent.manifest.name,
                    error = %e,
                    "agent.rewire.failed"
                );
            }
        }
    }
    if rewired > 0 {
        tracing::info!(count = rewired, "agent.rewire.completed");
    }
}

/// Wait until a `RuntimeEvent::ShutdownRequested` event is received on the bus.
pub(super) async fn wait_for_shutdown_event(rx: &mut ResilientReceiver) {
    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::ShutdownRequested) {
            return;
        }
    }
}
