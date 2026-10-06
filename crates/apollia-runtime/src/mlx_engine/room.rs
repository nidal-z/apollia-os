//! The models resident beside a new one, and stopping some of them to make
//! room under the load guardrail (see [`crate::residency::room`]).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use apollia_core::ModelUnloadReason;

use crate::engine_store::Engine;
use crate::local_engine::guardrails::{self, GuardrailPolicy, LoadEstimate};
use crate::local_engine::{GuardrailRefusal, LoadConsent};
use crate::residency::replace::{beside_the_ones_that_stay, replaced, Replaceable};
use crate::residency::room::{plan_room, Resident, Room};
use crate::residency::Residency;

use super::MlxEngineSupervisor;

impl MlxEngineSupervisor {
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

    /// Judge `estimate` of `model_path` under `policy` with the footprints of
    /// the resident models added, stopping the ones nothing holds when the sum
    /// is refused and stopping them makes it fit. Returns the footprint of the
    /// model alone.
    pub(super) async fn judge_with_residents(
        &self,
        policy: &GuardrailPolicy,
        estimate: Option<LoadEstimate>,
        consent: LoadConsent,
        model_path: &str,
    ) -> Result<Option<f64>, GuardrailRefusal> {
        let Some(estimate) = estimate else {
            guardrails::enforce(policy, None, consent, model_path)?;
            return Ok(None);
        };
        let residents: Vec<Resident> = self
            .instances
            .lock()
            .await
            .iter()
            .filter(|i| i.model_path != model_path)
            .map(|i| Resident {
                model_path: i.model_path.clone(),
                footprint_gb: i.footprint_gb.unwrap_or(0.0),
                evictable: i.in_flight.load(Ordering::Acquire) == 0
                    && !self
                        .residency
                        .get()
                        .is_some_and(|r| r.is_held(&i.model_path)),
                last_used: i.last_used,
            })
            .collect();
        let (room, judged) = plan_room(policy, &estimate, &residents);
        if let Room::Evict(models) = &room {
            self.make_room(models).await;
        }
        guardrails::enforce(policy, Some(&judged), consent, model_path)?;
        Ok(Some(estimate.total_gb))
    }

    /// Hold the launch of `model_path` to `policy` when a load first unloads
    /// the resident models: judged beside the pinned ones and the ones
    /// answering a request, which stay, then every other one is stopped. A
    /// refused launch stops nothing. Returns the footprint of the model alone.
    /// The caller holds the respawn lock.
    pub(super) async fn admit_alone(
        &self,
        policy: &GuardrailPolicy,
        estimate: Option<LoadEstimate>,
        consent: LoadConsent,
        model_path: &str,
    ) -> Result<Option<f64>, GuardrailRefusal> {
        let residents: Vec<Replaceable> = {
            let instances = self.instances.lock().await;
            instances
                .iter()
                .filter(|i| i.model_path != model_path)
                .map(|i| Replaceable {
                    model_path: i.model_path.clone(),
                    footprint_gb: i.footprint_gb.unwrap_or(0.0),
                    stays: i.in_flight.load(Ordering::Acquire) > 0
                        || self
                            .residency
                            .get()
                            .is_some_and(|r| r.is_pinned(&i.model_path)),
                })
                .collect()
        };
        let judged = estimate
            .as_ref()
            .map(|estimate| beside_the_ones_that_stay(estimate, &residents));
        guardrails::enforce(policy, judged.as_ref(), consent, model_path)?;
        self.make_room(&replaced(&residents)).await;
        Ok(estimate.map(|estimate| estimate.total_gb))
    }

    /// Stop the processes of `models` that serve no request. The caller holds
    /// the respawn lock.
    async fn make_room(&self, models: &[String]) {
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
                    Engine::VllmMlx.as_str(),
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

    /// Record the footprint judged for the process behind `base_url`.
    pub(super) async fn record_footprint(&self, base_url: &str, footprint_gb: Option<f64>) {
        if let Some(instance) = self
            .instances
            .lock()
            .await
            .iter_mut()
            .find(|i| i.base_url() == base_url)
        {
            instance.footprint_gb = footprint_gb;
        }
    }
}
