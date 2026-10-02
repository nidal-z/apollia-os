//! Stopped agents: their coordinators wait here, unloaded, for the next task.

use apollia_core::{AgentId, ProcessState};
use tracing::{info, warn};

use super::{SubmitError, TaskRouter};
use crate::coordinator::ExecutionBackend;

impl<B: ExecutionBackend> TaskRouter<B> {
    /// Bring a stopped agent back for the task that reached it: its parked
    /// coordinator returns to the dispatch table and the registry moves it
    /// back to `Active`. Its backend loads on the task's first await.
    pub(super) async fn reload_parked(&mut self, agent_id: &AgentId) -> Result<(), SubmitError> {
        let Some(coordinator) = self.parked.remove(agent_id) else {
            return Err(SubmitError::AgentUnavailable(agent_id.clone()));
        };
        for state in [ProcessState::Initializing, ProcessState::Active] {
            if let Err(error) = self.registry.update_state(agent_id.as_str(), state).await {
                warn!(agent_id = %agent_id, error = %error, "agent.reload.failed");
                self.parked.insert(agent_id.clone(), coordinator);
                return Err(SubmitError::AgentUnavailable(agent_id.clone()));
            }
        }
        self.coordinators.insert(agent_id.clone(), coordinator);
        info!(agent_id = %agent_id, "agent.reloaded");
        Ok(())
    }
}
