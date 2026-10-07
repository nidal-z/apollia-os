//! Making a stored agent available to the runtime.
//!
//! An agent is a definition in `agents.db`: no code stands behind it, so
//! nothing is imported to make it available. It is registered from that
//! definition, moved to `Active`, and given a coordinator in the task router,
//! so that it is listed, read and stopped like any registered agent. Its
//! tasks are run by the agent executor (`crate::agents::task`).
//!
//! The Apollia agent, the built-in chat, is always there and is not
//! registered: it is never started nor stopped.

use std::sync::Arc;

use apollia_core::{AgentId, ObservabilityConfig, ProcessState};
use apollia_tools::{AgentRepository, AgentRepositoryError, StoredAgent, TaskRepository};

use crate::config_reload::Live;
use crate::coordinator::{ExecutionBackend, ExecutionCoordinator};
use crate::eventbus::EventBusSender;
use crate::registry::{AgentRegistryError, AgentRegistryHandle};
use crate::router::TaskRouterHandle;

/// What registering an agent needs from the runtime.
pub struct Availability<'a, B: ExecutionBackend> {
    /// The agent registry.
    pub registry: &'a AgentRegistryHandle,
    /// The task router, which holds one coordinator per registered agent.
    pub router: &'a TaskRouterHandle<B>,
    /// The runtime event bus.
    pub event_sender: &'a EventBusSender,
    /// The backend of the coordinator.
    pub backend: &'a B,
    /// The task store the coordinator records into, when there is one.
    pub task_repository: Option<&'a Arc<TaskRepository>>,
    /// The observability settings in force.
    pub observability: Live<ObservabilityConfig>,
}

/// The stored agents the runtime makes available: the enabled ones, the
/// Apollia agent left out.
///
/// # Errors
///
/// [`AgentRepositoryError`] when `agents.db` cannot be read.
pub fn stored_agents_to_register(
    repository: &AgentRepository,
) -> Result<Vec<StoredAgent>, AgentRepositoryError> {
    Ok(repository
        .list_stored()?
        .into_iter()
        .filter(|agent| agent.enabled && !apollia_tools::is_built_in_agent(&agent.name))
        .collect())
}

/// Registers `agent`, moves it to `Active` and gives the router its
/// coordinator. Returns the identifier the registry gave it.
///
/// # Errors
///
/// [`AgentRegistryError`] when the registry refuses the registration or the
/// transition.
pub async fn make_available<B: ExecutionBackend + Clone>(
    agent: &StoredAgent,
    runtime: &Availability<'_, B>,
) -> Result<AgentId, AgentRegistryError> {
    let manifest = apollia_tools::manifest_of(agent);
    let max_concurrent = manifest.max_concurrent_tasks;
    let agent_id = runtime.registry.register(manifest).await?;
    runtime
        .registry
        .update_state(agent_id.as_str(), ProcessState::Active)
        .await?;

    let mut coordinator = ExecutionCoordinator::new(
        agent_id.clone(),
        max_concurrent,
        runtime.event_sender.clone(),
        runtime.backend.clone(),
    )
    .with_agent_name(agent.name.clone());
    if let Some(repository) = runtime.task_repository {
        coordinator =
            coordinator.with_task_repository(Arc::clone(repository), runtime.observability.clone());
    }
    // A failed registration of the coordinator leaves the agent listed; a
    // task to it is still run by the agent executor.
    let _ = runtime
        .router
        .register_coordinator(agent_id.clone(), coordinator)
        .await;
    tracing::event!(
        tracing::Level::INFO,
        name = %agent.name,
        id = %agent_id,
        "agent.available"
    );
    Ok(agent_id)
}

/// Registers every stored, enabled agent the registry does not hold yet, and
/// returns how many it registered.
///
/// This is what makes an agent available as soon as it is installed or
/// created, with no start. An agent the registry already holds is left as it
/// is, a stopped one included: `agent start` brings that one back. A store
/// that cannot be read registers nothing; each failure is logged.
pub async fn register_missing<B: ExecutionBackend + Clone>(
    data_dir: &std::path::Path,
    runtime: &Availability<'_, B>,
) -> usize {
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let stored = tokio::task::spawn_blocking(move || {
        if !db.exists() {
            return Ok(Vec::new());
        }
        AgentRepository::open(&db).and_then(|repository| stored_agents_to_register(&repository))
    })
    .await;
    let agents = match stored {
        Ok(Ok(agents)) => agents,
        Ok(Err(e)) => {
            tracing::event!(tracing::Level::WARN, error = %e, "agent.list.failed");
            return 0;
        }
        Err(e) => {
            tracing::event!(tracing::Level::WARN, error = %e, "agent.list.failed");
            return 0;
        }
    };
    let mut registered = 0;
    for agent in &agents {
        match runtime.registry.find_by_name(&agent.name).await {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => {
                tracing::event!(
                    tracing::Level::WARN,
                    name = %agent.name,
                    error = %e,
                    "agent.register.failed"
                );
                continue;
            }
        }
        match make_available(agent, runtime).await {
            Ok(_) => registered += 1,
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                name = %agent.name,
                error = %e,
                "agent.register.failed"
            ),
        }
    }
    registered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::MockBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;

    fn stored(name: &str, enabled: bool) -> StoredAgent {
        StoredAgent {
            name: name.to_owned(),
            origin: apollia_tools::AgentOrigin::Package,
            package: None,
            version: "1.0.0".to_owned(),
            definition: serde_json::json!({
                "id": name, "name": name, "mission": "Helps.", "defaults": {},
            }),
            enabled,
            installed_at: "2026-10-01T00:00:00Z".to_owned(),
            updated_at: "2026-10-01T00:00:00Z".to_owned(),
        }
    }

    #[tokio::test]
    async fn test_an_agent_stored_after_boot_is_registered_once_and_a_stopped_one_left_alone() {
        // GIVEN a runtime whose store gains an enabled agent, a disabled one
        // and, as every store, the Apollia agent
        let data = tempfile::tempdir().expect("tempdir");
        let repository =
            AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(data.path()))
                .expect("open agents.db");
        repository
            .save_definition(&stored("veille", true))
            .expect("save");
        repository
            .save_definition(&stored("asleep", false))
            .expect("save");
        let (events, _) = EventBus::new();
        let registry = AgentRegistry::spawn(events.clone());
        let router: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), events.clone(), 8);
        let runtime = Availability {
            registry: &registry,
            router: &router,
            event_sender: &events,
            backend: &MockBackend,
            task_repository: None,
            observability: ObservabilityConfig::default().into(),
        };

        // WHEN the missing agents are registered, twice, then the agent is
        // stopped and they are registered once more
        let first = register_missing(data.path(), &runtime).await;
        let second = register_missing(data.path(), &runtime).await;
        let id = registry
            .find_by_name("veille")
            .await
            .expect("registry")
            .expect("registered");
        registry
            .update_state(id.as_str(), ProcessState::Stopping)
            .await
            .expect("stopping");
        registry
            .update_state(id.as_str(), ProcessState::Stopped)
            .await
            .expect("stopped");
        let third = register_missing(data.path(), &runtime).await;

        // THEN the enabled agent alone was registered, once, and a stop is
        // not undone behind the back of who stopped it
        assert_eq!((first, second, third), (1, 0, 0));
        let listed = registry.list_agents().await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].manifest.name, "veille");
        assert_eq!(listed[0].process_state, ProcessState::Stopped);
    }
}
