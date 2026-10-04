//! The models resident beside a new one, and stopping some of them to make
//! room under the load guardrail (see [`crate::residency::room`]).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use apollia_core::ModelUnloadReason;

use crate::engine_store::Engine;
use crate::residency::room::Resident;
use crate::residency::Residency;

use super::LlamaServerSupervisor;

impl LlamaServerSupervisor {
    /// Share `residency` with this engine: its leases and pins keep a model
    /// loaded. A second call keeps the first residency.
    pub fn attach_residency(&self, residency: Arc<Residency>) {
        let _ = self.residency.set(residency);
    }

    /// The residency this engine consults, `None` until one is attached.
    #[must_use]
    pub fn residency(&self) -> Option<Arc<Residency>> {
        self.residency.get().cloned()
    }

    /// Whether `model_path` must stay loaded: pinned, or designated by an
    /// open conversation.
    fn holds(&self, model_path: &str) -> bool {
        self.residency
            .get()
            .is_some_and(|residency| residency.is_held(model_path))
    }

    /// The chat models resident beside `model_path`, its own processes left
    /// out since a launch replaces them.
    pub(super) async fn residents_beside(&self, model_path: &str) -> Vec<Resident> {
        let instances = self.instances.lock().await;
        instances
            .iter()
            .filter(|i| i.model_path != model_path)
            .map(|i| Resident {
                model_path: i.model_path.clone(),
                footprint_gb: i.footprint_gb.unwrap_or(0.0),
                evictable: i.in_flight.load(Ordering::Acquire) == 0 && !self.holds(&i.model_path),
                last_used: i.last_used,
            })
            .collect()
    }

    /// Stop the processes of `models` that serve no request, to make room for
    /// another model. The caller holds the respawn lock.
    pub(super) async fn make_room(&self, models: &[String]) {
        let stopping: Vec<_> = {
            let mut instances = self.instances.lock().await;
            let (stopping, kept) = std::mem::take(&mut *instances).into_iter().partition(|i| {
                models.contains(&i.model_path) && i.in_flight.load(Ordering::Acquire) == 0
            });
            *instances = kept;
            stopping
        };
        for mut instance in stopping {
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
            match self.residency.get() {
                Some(residency) => residency.announce_unloaded(
                    Engine::LlamaServer.as_str(),
                    &instance.model_path,
                    ModelUnloadReason::MakeRoom,
                ),
                None => tracing::event!(
                    tracing::Level::INFO,
                    model = %instance.model_path,
                    reason = %ModelUnloadReason::MakeRoom,
                    "llm.model.unloaded"
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests;
