//! The model of an agent, resolved at each call, and the router a reload puts
//! in place of this one.
//!
//! An agent asks for a model in two ways: the user assigns one (kept in
//! `system.db`, read again at every call so a change applies without a
//! restart), or its manifest declares a backend. When neither is given the
//! agent runs on the default model. When the model it asks for is not
//! available, the call runs on the default model instead and the router says
//! so with [`RuntimeEvent::LlmAgentModelFallback`]: the agent keeps working.
//!
//! A router is immutable once built, so a reload builds a new one. Whoever
//! still holds the old one (a running agent) reaches the new one through
//! [`LlmRouter::latest`], because the reload calls [`LlmRouter::supersede`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use apollia_core::events::{EventBusSender, RuntimeEvent};
use serde::Serialize;

use apollia_core::manifest::PackageRequestSettings;
use apollia_core::model_preset::RequestSettings;

use crate::request_defaults::{self, RequestDefaults, RequestLevels};
use crate::router::LlmRouter;
use crate::types::{CompletionModel, LlmError};

/// Longest chain of reloads [`LlmRouter::latest`] follows in one call. Each
/// call shortens the chain it walked, so a real chain stays at one or two
/// links; the bound only keeps a misuse from looping.
const MAX_SUCCESSOR_HOPS: usize = 64;

/// Directory beside `system.db` that holds the models, their index and the
/// publisher defaults file.
const MODELS_DIR_NAME: &str = "models";

/// The last time an agent's model was not available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentModelFallback {
    /// Agent whose call fell back.
    pub agent: String,
    /// Model the agent asked for.
    pub requested: String,
    /// Model that served the call instead.
    pub used: String,
    /// When it happened, RFC 3339.
    pub at: String,
}

/// What one call of an agent resolved to.
enum Choice {
    /// The model asked for, reachable.
    Asked(String),
    /// Nothing asked for.
    Unasked,
    /// The model asked for is not reachable.
    FellBack,
}

/// State of a router that outlives it across reloads.
#[derive(Default)]
pub(crate) struct LiveState {
    /// The router a reload put in place of this one, if any.
    successor: RwLock<Option<Arc<LlmRouter>>>,
    /// Last fallback of each agent. Each router owns its own; a reload copies
    /// them into the router it builds, and [`LlmRouter::supersede`] hands over
    /// any recorded in between, so no lock is shared from one router to the
    /// next.
    fallbacks: Mutex<HashMap<String, AgentModelFallback>>,
}

impl LiveState {
    fn fallbacks(&self) -> std::sync::MutexGuard<'_, HashMap<String, AgentModelFallback>> {
        self.fallbacks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The state of the router in force at the end of the chain of reloads
    /// that starts here.
    fn tail(self: &Arc<Self>) -> Arc<LiveState> {
        let mut tail = Arc::clone(self);
        for _ in 0..MAX_SUCCESSOR_HOPS {
            let successor = tail
                .successor
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            match successor {
                Some(successor) if !Arc::ptr_eq(&successor.live, &tail) => {
                    tail = Arc::clone(&successor.live);
                }
                _ => break,
            }
        }
        tail
    }
}

impl LlmRouter {
    /// The router in force: this one, or the one the latest reload put in its
    /// place. A running agent calls it before each call, so a reload reaches
    /// it without a restart.
    pub fn latest(self: &Arc<Self>) -> Arc<LlmRouter> {
        let mut current = Arc::clone(self);
        for _ in 0..MAX_SUCCESSOR_HOPS {
            let next = current
                .live
                .successor
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            match next {
                Some(next) if !Arc::ptr_eq(&next.live, &current.live) => current = next,
                _ => break,
            }
        }
        if !Arc::ptr_eq(&current.live, &self.live) {
            // Point straight at the end of the chain, so the routers in
            // between are released once nobody else holds them.
            *self
                .live
                .successor
                .write()
                .unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&current));
        }
        current
    }

    /// Record that `next` replaces this router. Every holder of this router,
    /// or of one it already replaced, reaches `next` through [`Self::latest`].
    pub fn supersede(&self, next: &Arc<LlmRouter>) {
        let tail = self.live.tail();
        if Arc::ptr_eq(&tail, &next.live) {
            return;
        }
        // A fallback recorded on the tail after `next` was built would
        // otherwise stay behind; one `next` recorded itself is newer.
        let handed: Vec<(String, AgentModelFallback)> = tail
            .fallbacks()
            .iter()
            .map(|(agent, fallback)| (agent.clone(), fallback.clone()))
            .collect();
        {
            let mut kept = next.live.fallbacks();
            for (agent, fallback) in handed {
                kept.entry(agent).or_insert(fallback);
            }
        }
        *tail
            .successor
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(next));
    }

    /// Keep, on a router rebuilt by a reload, what `system.db` does not hold:
    /// the `[llm.routing]` of the router it replaces, and the last fallback of
    /// each agent. Without it a reload silently drops the routing.
    pub fn carry_over_from(mut self, previous: &LlmRouter) -> Self {
        if self.routing.is_none() {
            self.routing = previous.routing.clone();
        }
        let carried = previous.live.tail().fallbacks().clone();
        self.live = Arc::new(LiveState {
            successor: RwLock::new(None),
            fallbacks: Mutex::new(carried),
        });
        self
    }

    /// The `[llm.routing]` this router carries, if any.
    pub fn routing(&self) -> Option<&apollia_core::LlmRoutingConfig> {
        self.routing.as_ref()
    }

    /// Resolve the backend of one call of `agent`.
    ///
    /// The model asked for is the user's assignment, else the backend the
    /// manifest declares. It is returned when this router can reach it. When
    /// nothing is asked for, `None` leaves the router's default in force. When
    /// the model asked for is not reachable, `None` too, and the fallback is
    /// recorded and sent on `bus` as [`RuntimeEvent::LlmAgentModelFallback`].
    pub fn resolve_agent_model(
        &self,
        agent: &str,
        assigned: Option<&str>,
        declared: Option<&str>,
        bus: Option<&EventBusSender>,
    ) -> Option<String> {
        match self.resolve_choice(agent, assigned, declared, bus) {
            Choice::Asked(name) => Some(name),
            Choice::Unasked | Choice::FellBack => None,
        }
    }

    // TEST-ONLY: the runners of the Python agent path resolved the planner
    // model with it; the agent executor routes on the model of the run with
    // `LlmRouter::route`.
    /// The reasoning backend of one orchestrated run of the agent `binding`
    /// names: its own model when reachable; the default when that model is
    /// not available (said as [`Self::resolve_agent_model`] says it); and
    /// [`Self::route_precise`] when the agent asks for nothing.
    ///
    /// # Errors
    /// As [`Self::route_precise`], when the agent asks for nothing.
    pub fn route_precise_for(
        &self,
        binding: &AgentModelBinding,
        bus: Option<&EventBusSender>,
    ) -> Result<Arc<dyn CompletionModel>, LlmError> {
        let assigned = binding.assigned();
        match self.resolve_choice(
            &binding.agent,
            assigned.as_deref(),
            binding.declared.as_deref(),
            bus,
        ) {
            Choice::Asked(name) => self.lookup(&name).ok_or(LlmError::BackendNotFound(name)),
            Choice::FellBack => Ok(self.route(None)),
            Choice::Unasked => self.route_precise(),
        }
    }

    fn resolve_choice(
        &self,
        agent: &str,
        assigned: Option<&str>,
        declared: Option<&str>,
        bus: Option<&EventBusSender>,
    ) -> Choice {
        let Some(requested) = assigned
            .or(declared)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            return Choice::Unasked;
        };
        if self.lookup(requested).is_some() {
            return Choice::Asked(requested.to_string());
        }
        let used = self.default_name().to_string();
        tracing::event!(
            tracing::Level::WARN,
            agent = %agent,
            requested = %requested,
            used = %used,
            "llm.agent_model.fallback"
        );
        let fallback = AgentModelFallback {
            agent: agent.to_string(),
            requested: requested.to_string(),
            used: used.clone(),
            at: chrono::Utc::now().to_rfc3339(),
        };
        self.live
            .tail()
            .fallbacks()
            .insert(agent.to_string(), fallback);
        if let Some(bus) = bus {
            // Fire-and-forget: a bus without subscribers is not an error.
            let _ = bus.send(RuntimeEvent::LlmAgentModelFallback {
                agent: agent.to_string(),
                requested: requested.to_string(),
                used,
            });
        }
        Choice::FellBack
    }

    /// The last time `agent`'s model was not available, if it ever was not
    /// since the runtime started.
    pub fn agent_model_fallback(&self, agent: &str) -> Option<AgentModelFallback> {
        self.live.tail().fallbacks().get(agent).cloned()
    }
}

/// What one agent asks for, read again at each of its calls.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentModelBinding {
    agent: String,
    declared: Option<String>,
    system_db: Option<PathBuf>,
    package_request: Option<PackageRequestSettings>,
    session_preset: Option<i64>,
}

impl AgentModelBinding {
    /// `declared` is the manifest's `llm_backend`; `system_db` is where the
    /// user's assignment lives (`None` reads no assignment).
    pub fn new(
        agent: impl Into<String>,
        declared: Option<String>,
        system_db: Option<PathBuf>,
    ) -> Self {
        Self {
            agent: agent.into(),
            declared,
            system_db,
            package_request: None,
            session_preset: None,
        }
    }

    /// The preset chosen for the conversation the calls belong to. It takes
    /// the place of the agent's preset at the preset level of
    /// [`Self::request_levels`], when it is a preset of the model the call
    /// runs on.
    #[must_use]
    pub fn with_session_preset(mut self, preset: Option<i64>) -> Self {
        self.session_preset = preset;
        self
    }

    // TEST-ONLY: the runners of the Python agent path read the package level
    // from the manifest of the agent; nothing in production sets it now.
    /// The request settings the agent's package declares, the package level
    /// of [`Self::request_defaults`].
    #[must_use]
    pub fn with_package_request(mut self, settings: Option<PackageRequestSettings>) -> Self {
        self.package_request = settings;
        self
    }

    /// The request settings one call of the agent starts from, resolved now in
    /// three levels (see [`crate::request_defaults`]): the defaults of the
    /// model `backend` routes to on `router`, the package's settings, and the
    /// preset of the user's assignment. The preset counts only when the call
    /// runs on the assigned model: a preset belongs to one model.
    pub fn request_defaults(&self, router: &LlmRouter, backend: Option<&str>) -> RequestDefaults {
        self.request_levels(router, backend).resolved()
    }

    /// The three levels [`Self::request_defaults`] resolves, kept apart.
    pub fn request_levels(&self, router: &LlmRouter, backend: Option<&str>) -> RequestLevels {
        let models_dir = self
            .system_db
            .as_deref()
            .and_then(Path::parent)
            .map(|dir| dir.join(MODELS_DIR_NAME));
        let model = router
            .get(backend)
            .map(|b| {
                RequestDefaults::from_model(&request_defaults::model_level(
                    b.model_id(),
                    b.backend_name(),
                    models_dir.as_deref(),
                ))
            })
            .unwrap_or_default();
        let package = self
            .package_request
            .as_ref()
            .map(RequestDefaults::from_package)
            .unwrap_or_default();
        let preset = self
            .system_db
            .as_deref()
            .and_then(|path| {
                self.session_preset
                    .and_then(|id| read_session_preset(path, id, backend))
                    .or_else(|| read_preset(path, &self.agent, backend))
            })
            .map(|settings| RequestDefaults::from_preset(&settings))
            .unwrap_or_default();
        RequestLevels {
            model,
            package,
            preset,
        }
    }

    /// The agent's name.
    pub fn agent(&self) -> &str {
        &self.agent
    }

    /// The model the user assigned to the agent, read from `system.db` now.
    /// An absent or unreadable database reads as no assignment.
    pub fn assigned(&self) -> Option<String> {
        let path = self.system_db.as_deref()?;
        read_assignment(path, &self.agent)
    }

    /// Resolve the backend of one call on `router`, as
    /// [`LlmRouter::resolve_agent_model`] does, reading the assignment now.
    pub fn resolve(&self, router: &LlmRouter, bus: Option<&EventBusSender>) -> Option<String> {
        let assigned = self.assigned();
        router.resolve_agent_model(
            &self.agent,
            assigned.as_deref(),
            self.declared.as_deref(),
            bus,
        )
    }
}

/// The request settings of the preset in force for `agent` on the model
/// `used`: the preset its assignment names, else the default preset of the
/// assigned model. `None` when the call does not run on the assigned model.
fn read_preset(path: &Path, agent: &str, used: Option<&str>) -> Option<RequestSettings> {
    if !path.exists() {
        return None;
    }
    let read = apollia_core::LlmBackendRepository::open(path).and_then(|repo| {
        let Some(assignment) = repo.agent_model(agent)? else {
            return Ok(None);
        };
        if used != Some(assignment.model_id.as_str()) {
            return Ok(None);
        }
        let preset = match assignment.preset_id {
            Some(id) => repo.find_preset(id)?,
            None => repo
                .list_presets(Some((assignment.model_source, &assignment.model_id)))?
                .into_iter()
                .find(|p| p.is_model_default),
        };
        Ok(preset.map(|p| p.request))
    });
    match read {
        Ok(settings) => settings,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %agent,
                error = %error,
                detail = "the call runs without the agent's preset",
                "llm.agent_model.preset_unreadable"
            );
            None
        }
    }
}

/// The request settings of the preset `id`, chosen for a conversation, when
/// it is a preset of the model `used`. `None` otherwise: a preset belongs to
/// one model, and the agent's own preset then stays in force.
fn read_session_preset(path: &Path, id: i64, used: Option<&str>) -> Option<RequestSettings> {
    if !path.exists() {
        return None;
    }
    let read = apollia_core::LlmBackendRepository::open(path).and_then(|repo| repo.find_preset(id));
    match read {
        Ok(Some(preset)) if used == Some(preset.model_id.as_str()) => Some(preset.request),
        Ok(_) => None,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                preset = id,
                error = %error,
                detail = "the call runs without the preset of the conversation",
                "llm.agent_model.session_preset_unreadable"
            );
            None
        }
    }
}

fn read_assignment(path: &Path, agent: &str) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let read =
        apollia_core::LlmBackendRepository::open(path).and_then(|repo| repo.agent_model(agent));
    match read {
        Ok(assignment) => assignment.map(|a| a.model_id),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %agent,
                error = %error,
                detail = "the agent runs without its assigned model",
                "llm.agent_model.unreadable"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests;
