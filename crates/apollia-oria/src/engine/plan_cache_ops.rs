//! Plan cache lookups, stores, and the cached-plan execution path.
//!
//! Split out of `engine.rs`: the engine's state stays in the parent, the
//! reads and writes of the plan cache, and what a cache hit runs, live here.

use apollia_core::{AIPResult, AIPTask, AgentManifest, RuntimeEvent};

use crate::engine::{AIPAgent, ORIAEngine};
use crate::observer::ContextBundle;
use crate::plan::ExecutionPlan;
use crate::plan_repository::{PlanRepository, PlanRepositoryError};

impl ORIAEngine {
    /// Looks up a cached plan for `cache_key`, returning a ready-to-run
    /// [`ExecutionPlan`] (fresh `plan_id`, the supplied `task_id`, cached steps)
    /// on a hit, or `None` on a miss, an absent cache, or a recoverable error.
    ///
    /// Lock poisoning and lookup errors are logged and treated as a miss so the
    /// caller falls back to the Reasoner.
    pub(super) fn lookup_cached_plan(
        &self,
        cache_key: &str,
        task_id: &str,
    ) -> Option<ExecutionPlan> {
        let cache_mutex = self.plan_cache.as_ref()?;
        let cache = match cache_mutex.lock() {
            Ok(cache) => cache,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    detail = "the lookup is skipped",
                    "plan.cache.lock.poisoned"
                );
                return None;
            }
        };
        let cached_plan = match cache.lookup(cache_key) {
            Ok(Some(cached_plan)) => cached_plan,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(error = %e, "plan.cache.lookup.failed");
                return None;
            }
        };
        Some(ExecutionPlan {
            plan_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            steps: cached_plan.steps,
        })
    }
    /// Stores `plan` in the plan cache under `cache_key`.
    ///
    /// No-op when no cache is configured. Lock poisoning and store errors are
    /// logged and otherwise ignored (caching is best-effort).
    pub(super) fn store_plan_in_cache(
        &self,
        cache_key: &str,
        plan: &ExecutionPlan,
        manifest: &AgentManifest,
    ) {
        let Some(cache_mutex) = self.plan_cache.as_ref() else {
            return;
        };
        match cache_mutex.lock() {
            Ok(cache) => {
                if let Err(e) = cache.store(cache_key, plan, &manifest.name, &manifest.version) {
                    tracing::warn!(error = %e, "plan.cache.store.failed");
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    detail = "the store is skipped",
                    "plan.cache.lock.poisoned"
                );
            }
        }
    }
    /// Fills each tool step's structured arguments before the plan is cached,
    /// persisted and executed.
    ///
    /// For every tool step whose `args` are absent or invalid against the
    /// target tool's input schema, resolves them with a schema-guided model
    /// call so the persisted plan is fully specified, auditable and replayable.
    /// Best-effort: a step that cannot be resolved keeps `args = None`, and the
    /// [`crate::actor::ActorLoop`] resolves it just in time at execution.
    ///
    /// No-op without an injected tool proxy (the schema source) or when the
    /// router has no backend to answer the resolution call.
    pub(super) async fn enrich_plan_with_args(&self, plan: &mut ExecutionPlan) {
        let Some(proxy) = self.tool_proxy.as_ref() else {
            return;
        };
        for step in plan.steps.iter_mut() {
            let Some(tool_name) = step.tool_hint.as_deref() else {
                continue;
            };
            if tool_name == "llm" {
                continue;
            }
            let Some(schema) = proxy.tool_schema(tool_name).await else {
                continue;
            };
            // Keep already-valid plan-time args untouched.
            if step
                .args
                .as_ref()
                .is_some_and(|args| crate::arg_resolver::validate_args(args, &schema).is_ok())
            {
                continue;
            }
            let Some(model) = self.llm_router.get(step.model_hint.as_deref()) else {
                continue;
            };
            match crate::arg_resolver::resolve_tool_args(
                &model,
                tool_name,
                &schema,
                &step.description,
                0.0,
            )
            .await
            {
                Ok(args) => step.args = Some(args),
                Err(e) => tracing::event!(
                    tracing::Level::WARN,
                    step_id = %step.step_id,
                    tool = %tool_name,
                    error = %e,
                    "oria.plan.arg_enrichment_failed"
                ),
            }
        }
    }
    /// Execute a plan retrieved from the cache.
    ///
    /// Mirrors the post-Reasoner path of [`execute_orchestrated_plan`]: emit
    /// PlanGenerated for the cached plan, then delegate execution, verification,
    /// and replan to [`run_plan_with_verification`](Self::run_plan_with_verification).
    // Explicit dependency list for the cached-plan execution path; bundling
    // these into a struct would only relocate the argument list.
    // REASON: threads the engine's borrowed state through the cached-plan run; a struct would borrow the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_cached_plan(
        &self,
        plan: ExecutionPlan,
        task: AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
        manifest: AgentManifest,
        ctx: &ContextBundle,
        cache_key: &str,
    ) -> AIPResult {
        let plan_id = plan.plan_id.clone();
        let step_count = plan.steps.len();
        let task_id_str = task.task_id.clone();

        let db_path = self.db_path.as_deref().unwrap_or(":memory:");

        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan_id.clone(),
            step_count,
            // The orchestrated engine path correlates via task_id, not a chat run.
            run_id: None,
        });

        self.run_plan_with_verification(plan, &task, agent, &manifest, ctx, cache_key, db_path)
            .await
    }
    /// Opens a `PlanRepository` at `db_path`, inserts the plan and its steps.
    ///
    /// Falls back to `:memory:` if `db_path` fails. Errors during `insert_plan`
    /// or `insert_steps` are logged but do not abort execution (persistence is
    /// non-blocking).
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError`] when neither `db_path` nor `:memory:`
    /// opens. The in-memory fallback opens a database like any other, so its
    /// failure is returned rather than asserted.
    pub(super) fn open_repo_with_plan(
        &self,
        db_path: &str,
        plan: &ExecutionPlan,
        agent_name: &str,
    ) -> Result<PlanRepository, PlanRepositoryError> {
        let repo = match PlanRepository::new(db_path) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    detail = "falling back to an in-memory database",
                    "plan.repository.open.failed"
                );
                PlanRepository::new(":memory:")?
            }
        };

        if let Err(e) = repo.insert_plan(plan, agent_name) {
            tracing::error!(error = %e, detail = "non-blocking", "plan.persist.failed");
        }
        if let Err(e) = repo.insert_steps(&plan.plan_id, &plan.steps) {
            tracing::error!(error = %e, detail = "non-blocking", "plan.steps.persist.failed");
        }

        Ok(repo)
    }
}
