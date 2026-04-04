//! `ActorLoop` — boucle d'exécution topologique d'un [`crate::plan::ExecutionPlan`].
//!
//! `ActorLoop` est la pièce centrale du mode Orchestré (Option B) : ORIA exécute
//! directement les outils et le LLM — `agent.run()` n'est **pas** appelé pendant
//! les steps. L'agent fournit uniquement son `manifest()` et optionnellement
//! `on_plan_complete()`.
//!
//! ## Pipeline d'exécution
//!
//! ```text
//! ActorLoop::execute()
//!   ├── topological_sort(plan.steps)         → ordre d'exécution
//!   ├── Pour chaque step_id dans ordre :
//!   │   ├── StepBudget::is_exhausted()       → STEP_BUDGET_EXCEEDED si épuisé
//!   │   ├── db.start_step()                  → SQLite
//!   │   ├── execute_step()                   → outil via ToolProxyTrait OU LLM via LlmRouter
//!   │   ├── budget.increment_steps()
//!   │   ├── db.complete_step() / fail_step()
//!   │   └── EventBus: StepStarted / StepCompleted / StepFailed
//!   ├── Si step échoue (retryable) + replan_count < max_replans :
//!   │   └── reasoner.replan() → nouveau plan → execute_remaining()
//!   └── Tous steps complétés → db.complete_plan() + AIPResult::completed_with_steps()
//! ```
//!
//! ## Thread safety
//!
//! `ActorLoop` contient un [`crate::plan_repository::PlanRepository`] qui est `!Send`
//! (connexion SQLite via `RefCell`). Il doit être créé et consommé dans le même thread.
//! Les futures produites par `execute()` sont donc `!Send`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::manifest::AgentManifest;
use apollia_core::observability::ObservabilityConfig;
use apollia_core::{AIPResult, PendingApprovals};
use apollia_llm::{
    router::ObservabilityConfig as LlmObsConfig, ChatMessage, CompletionRequest, LlmRouter,
};
use apollia_memory::manager::MemoryManager;

use crate::budget::StepBudget;
use crate::observer::{ContextBundle, ExecutionMode};
use crate::plan::{ExecutionPlan, PlanStep};
use crate::plan_repository::PlanRepository;
use crate::reasoner::Reasoner;
use crate::resilience::ResilienceLayer;
use crate::topo::{topological_levels, topological_sort};

// ────────────────────────────────────────────────────────────────────────────
// ToolProxyTrait
// ────────────────────────────────────────────────────────────────────────────

/// Abstraction du ToolProxy pour l'`ActorLoop` — permet les tests sans PyO3.
///
/// Même pattern d'abstraction que `ToolExecutor` (ADR-015) et `AgentRunner` (ADR-016).
/// L'implémentation concrète délègue à `ToolProxy::call()` via le bridge AIP.
/// Les tests utilisent un mock implémentant ce trait.
#[async_trait::async_trait]
pub trait ToolProxyTrait: Send + Sync {
    /// Invoque l'outil `tool_name` avec `input` sérialisé en JSON.
    ///
    /// Retourne la sortie textuelle de l'outil en cas de succès,
    /// ou un message d'erreur en cas d'échec.
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String>;

    /// Returns `true` if `tool_name` does not modify any external state.
    ///
    /// Used by [`ActorLoop::execute_tool_steps`] to decide whether a batch of steps
    /// at the same topological level can be executed concurrently.
    /// Defaults to `false` (conservative): unknown tools are never parallelised.
    fn is_tool_read_only(&self, _tool_name: &str) -> bool {
        false
    }
}

// ────────────────────────────────────────────────────────────────────────────
// StepError
// ────────────────────────────────────────────────────────────────────────────

/// Erreur d'un step individuel produite par [`ActorLoop::execute_step`].
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    /// L'appel à l'outil a échoué.
    #[error("Tool call failed: {0}")]
    ToolCallFailed(String),
    /// L'appel au LLM a échoué.
    #[error("LLM call failed: {0}")]
    LlmCallFailed(String),
    /// Aucun backend LLM n'est configuré dans le `LlmRouter`.
    #[error("No LLM backend configured")]
    NoLlmBackend,
    /// L'outil demandé n'est pas enregistré dans le registre.
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    /// Le step a été rejeté par l'utilisateur avant exécution (HITL Mode Orchestré).
    ///
    /// Retourné par [`ActorLoop::suspend_for_approval`] quand le `ResumeHandler`
    /// envoie `approved=false`. L'exécution du plan s'arrête immédiatement —
    /// les steps suivants ne sont pas tentés.
    #[error("step rejeté par l'utilisateur : {reason}")]
    RejectedByUser {
        /// Raison transmise par l'opérateur lors du rejet.
        reason: String,
    },
    /// Le oneshot channel d'approbation a été fermé avant réponse (shutdown runtime).
    ///
    /// Indique que le runtime est en cours d'arrêt. L'exécution du plan est
    /// interrompue proprement sans panique.
    #[error("channel d'approbation fermé — runtime en cours d'arrêt")]
    ApprovalChannelClosed,
}

impl StepError {
    /// Retourne `true` si cette erreur peut déclencher une replanification.
    ///
    /// `ToolCallFailed` et `LlmCallFailed` sont retryables (problèmes transitoires).
    /// Les autres variantes sont permanentes et ne déclenchent pas de replanification.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            StepError::ToolCallFailed(_) | StepError::LlmCallFailed(_)
        )
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Constants
// ────────────────────────────────────────────────────────────────────────────

/// Importance level for step-level episodic memory entries.
///
/// Set to 0.6 — above the default recall threshold (0.5) so that step outputs
/// appear in standard memory queries, but below critical events (1.0).
const STEP_MEMORY_IMPORTANCE: f64 = 0.6;

/// Default maximum character length for step output stored in episodic memory.
///
/// Used as the field default in [`ActorLoop`]. The configurable value is
/// injected via [`ActorLoop::with_step_memory_max_chars`].
const DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS: usize = 200;

/// Maximum number of read-only tool steps driven concurrently by
/// [`ActorLoop::execute_tool_steps`].
///
/// Mirrors the same constant in `apollia-tools` to cap OS-level concurrency
/// (file descriptors, process handles) without requiring a shared config at
/// this layer.
const MAX_CONCURRENT_ORIA_TOOLS: usize = 10;

// ────────────────────────────────────────────────────────────────────────────
// StepContext
// ────────────────────────────────────────────────────────────────────────────

/// Context accumulated during plan execution, injected into each step.
///
/// Built incrementally by [`ActorLoop::execute`]: after each step completes,
/// its output is added to `previous_outputs`. Each subsequent step receives
/// a `StepContext` reflecting all prior results and the current budget state.
///
/// For LLM steps, `previous_outputs` is formatted into a system message
/// (`"Previous step results:\n- s1: …\n- s2: …"`) to enrich the prompt.
/// For tool steps, outputs are interpolated via `{{step_id}}` placeholders.
pub struct StepContext {
    /// Outputs of all previously completed steps, keyed by `step_id`.
    pub previous_outputs: HashMap<String, String>,
    /// Zero-based index of the current step in the topological order.
    pub step_index: usize,
    /// Total number of steps in the plan.
    pub total_steps: usize,
    /// Snapshot of the remaining budget at the moment this context was built.
    pub remaining_budget: apollia_llm::StepBudgetView,
}

impl StepContext {
    /// Formats `previous_outputs` as a human-readable block for LLM system messages.
    ///
    /// Returns `None` if there are no previous outputs.
    /// Returns lines formatted as `"Previous step results:\n- s1: {output}\n- s2: {output}"`.
    pub fn format_previous_outputs(&self) -> Option<String> {
        if self.previous_outputs.is_empty() {
            return None;
        }
        let mut lines = Vec::with_capacity(self.previous_outputs.len() + 1);
        lines.push("Previous step results:".to_string());
        // Sort by key for deterministic ordering.
        let mut entries: Vec<_> = self.previous_outputs.iter().collect();
        entries.sort_by_key(|(k, _)| (*k).clone());
        for (step_id, output) in entries {
            lines.push(format!("- {step_id}: {output}"));
        }
        Some(lines.join("\n"))
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ActorLoop
// ────────────────────────────────────────────────────────────────────────────

/// Boucle d'exécution topologique d'un [`ExecutionPlan`].
///
/// Exécute séquentiellement les steps dans l'ordre déterminé par le tri topologique,
/// en appliquant le [`StepBudget`] et la [`ResilienceLayer`] sur chaque appel outil/LLM.
/// Persiste chaque transition de statut dans SQLite via [`PlanRepository`].
/// Émet des [`RuntimeEvent`] sur l'`EventBus` à chaque changement d'état.
///
/// En cas d'échec retryable d'un step, déclenche une replanification via le [`Reasoner`]
/// jusqu'à `max_replans` fois.
///
/// Pour activer le support HITL, injecter un [`PendingApprovals`] via
/// [`with_pending_approvals`]. Sans cela, les steps avec `tools_requiring_approval`
/// s'exécutent directement sans suspension.
///
/// [`with_pending_approvals`]: ActorLoop::with_pending_approvals
pub struct ActorLoop {
    plan: ExecutionPlan,
    replan_count: u32,
    max_replans: u32,
    db: PlanRepository,
    event_bus: EventBusSender,
    /// Manifest de l'agent propriétaire de ce plan.
    ///
    /// Stocké en lecture seule pour que `execute_step` puisse accéder à
    /// `tools_requiring_approval` lors de chaque step.
    pub manifest: AgentManifest,
    /// Registre HITL des approbations en attente — partagé avec le `ResumeHandler`.
    ///
    /// `Some` → les steps dont l'outil est dans `tools_requiring_approval` suspendent
    /// l'exécution et attendent la décision humaine via un oneshot channel.
    /// `None` → pas de suspension HITL (mode dégradé, steps s'exécutent normalement).
    pending_approvals: Option<Arc<PendingApprovals>>,
    /// Configuration d'observabilité pour la troncature des inputs/outputs persistés.
    obs_config: ObservabilityConfig,
    /// Memory manager for episodic recording after each step.
    ///
    /// When `Some`, step outputs are automatically recorded as episodic memories
    /// in the agent's namespace. `Arc<Mutex<MemoryManager>>` follows the ADR-033
    /// precedent (rare mutations, operator-level writes).
    memory_manager: Option<Arc<Mutex<MemoryManager>>>,
    /// Maximum character length for step output stored in episodic memory.
    ///
    /// Injected from `ORIAConfig::step_memory_max_chars`. Defaults to
    /// [`DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS`] when not configured.
    step_memory_max_chars: usize,
}

impl ActorLoop {
    /// Crée un `ActorLoop` pour un plan donné.
    ///
    /// Le plan doit déjà être inséré dans SQLite avant la création de l'`ActorLoop`
    /// (via `PlanRepository::insert_plan` + `insert_steps`).
    ///
    /// `manifest` est conservé en lecture seule pour que les steps puissent
    /// accéder à `tools_requiring_approval`.
    ///
    /// Pour activer le support HITL, chaîner avec [`with_pending_approvals`].
    ///
    /// [`with_pending_approvals`]: ActorLoop::with_pending_approvals
    pub fn new(
        plan: ExecutionPlan,
        max_replans: u32,
        db: PlanRepository,
        event_bus: EventBusSender,
        manifest: AgentManifest,
    ) -> Self {
        Self {
            plan,
            replan_count: 0,
            max_replans,
            db,
            event_bus,
            manifest,
            pending_approvals: None,
            obs_config: ObservabilityConfig::default(),
            memory_manager: None,
            step_memory_max_chars: DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS,
        }
    }

    /// Injecte le registre HITL des approbations en attente.
    ///
    /// Requis pour que les steps dont l'outil est dans `tools_requiring_approval`
    /// suspendent l'exécution et attendent la décision humaine.
    /// Partagé entre l'`ActorLoop` et le `ResumeHandler` via `AppState`.
    pub fn with_pending_approvals(mut self, pending: Option<Arc<PendingApprovals>>) -> Self {
        self.pending_approvals = pending;
        self
    }

    /// Configure l'observabilité pour la troncature des inputs/outputs persistés.
    ///
    /// Par défaut, utilise [`ObservabilityConfig::default()`].
    pub fn with_obs_config(mut self, config: ObservabilityConfig) -> Self {
        self.obs_config = config;
        self
    }

    /// Injecte un [`MemoryManager`] pour l'enregistrement épisodique per-step.
    ///
    /// Quand configuré, chaque step complété avec succès enregistre automatiquement
    /// une entrée épisodique dans le namespace de l'agent. L'écriture est fire-and-forget :
    /// un échec est loggé en warning mais n'interrompt jamais l'exécution du plan.
    ///
    /// `Arc<Mutex<MemoryManager>>` suit le précédent ADR-033 (mutations rares).
    pub fn with_memory_manager(mut self, mm: Option<Arc<Mutex<MemoryManager>>>) -> Self {
        self.memory_manager = mm;
        self
    }

    /// Overrides the maximum character length for step output stored in episodic memory.
    ///
    /// Injected from `ORIAConfig::step_memory_max_chars`. When not called,
    /// defaults to [`DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS`] (200 chars).
    pub fn with_step_memory_max_chars(mut self, max_chars: usize) -> Self {
        self.step_memory_max_chars = max_chars;
        self
    }

    /// Exécute le plan complet dans l'ordre topologique.
    ///
    /// Retourne `AIPResult::completed_with_steps` si tous les steps se complètent.
    /// Retourne `AIPResult::failed` si le budget est épuisé, si trop de replanifications
    /// ont été tentées, ou si un step échoue de façon permanente.
    ///
    /// Toutes les erreurs SQLite sont loggées mais n'interrompent pas l'exécution
    /// (fire-and-forget).
    pub async fn execute(
        &mut self,
        tool_proxy: &dyn ToolProxyTrait,
        llm_router: &LlmRouter,
        budget: &StepBudget,
        _resilience: &ResilienceLayer,
        reasoner: &Reasoner,
    ) -> AIPResult {
        let levels = match topological_levels(&self.plan.steps) {
            Ok(l) => l,
            Err(_) => {
                if let Err(e) = self.db.fail_plan(&self.plan.plan_id, "INVALID_PLAN") {
                    tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                }
                return AIPResult::failed("INVALID_PLAN", "Circular dependency in execution plan");
            }
        };

        let mut completed_outputs: HashMap<String, String> = HashMap::new();

        // Iterate level by level. Within a level all steps have the same dependency
        // depth and can run concurrently when they are all read-only tool calls.
        for level_ids in levels {
            // Collect the PlanStep objects for this level.
            let level_steps: Vec<PlanStep> = level_ids
                .iter()
                .filter_map(|id| self.plan.steps.iter().find(|s| s.step_id == *id))
                .cloned()
                .collect();

            // A level qualifies for concurrent batch execution when every step is a
            // read-only tool call that does not require human approval.
            let batch_eligible = level_steps.len() > 1
                && level_steps.iter().all(|s| {
                    s.tool_hint.as_deref().is_some_and(|t| {
                        t != "llm"
                            && tool_proxy.is_tool_read_only(t)
                            && !self
                                .manifest
                                .tools_requiring_approval
                                .iter()
                                .any(|a| a == t)
                    })
                });

            if batch_eligible {
                // ── BATCH PATH ────────────────────────────────────────────────────
                // Phase 1 (sequential): budget guard, events, DB pre-execution.
                if budget.is_exhausted() {
                    if let Err(e) = self
                        .db
                        .fail_plan(&self.plan.plan_id, "STEP_BUDGET_EXCEEDED")
                    {
                        tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                    }
                    let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                        task_id: self.plan.task_id.clone().into(),
                        plan_id: self.plan.plan_id.clone(),
                        reason: "STEP_BUDGET_EXCEEDED".to_string(),
                    });
                    return AIPResult::failed(
                        "STEP_BUDGET_EXCEEDED",
                        &format!("Budget de {} steps atteint", budget.max_steps),
                    );
                }
                for step in &level_steps {
                    let step_id = &step.step_id;
                    let step_num = completed_outputs.len() + 1;
                    let total = self.plan.steps.len();
                    let _ = self.event_bus.send(RuntimeEvent::StepStarted {
                        task_id: self.plan.task_id.clone().into(),
                        plan_id: self.plan.plan_id.clone(),
                        step_id: step_id.clone(),
                        step_num,
                        total,
                        desc: step.description.clone(),
                    });
                    if let Err(e) = self.db.start_step(&self.plan.plan_id, step_id) {
                        tracing::warn!(error = %e, step_id = %step_id, "start_step DB call failed (ignored)");
                    }
                    let rendered = interpolate_outputs(&step.description, &completed_outputs);
                    if let Err(e) = self.db.save_step_input(
                        step_id,
                        &self.plan.plan_id,
                        &rendered,
                        &self.obs_config,
                    ) {
                        tracing::warn!(error = %e, step_id = %step_id, "save_step_input DB call failed (ignored)");
                    }
                    let actual_tool = step.tool_hint.as_deref().unwrap_or("llm");
                    if let Err(e) = self
                        .db
                        .save_step_tool(step_id, &self.plan.plan_id, actual_tool)
                    {
                        tracing::warn!(error = %e, step_id = %step_id, "save_step_tool DB call failed (ignored)");
                    }
                }

                // Phase 2: Concurrent invocations.
                let started = Instant::now();
                let batch_results = self
                    .execute_tool_steps(&level_steps, &completed_outputs, tool_proxy)
                    .await;
                let duration_ms = started.elapsed().as_millis() as u64;

                // Phase 3 (sequential): budget increment, DB post-execution, events, errors.
                for (step, (step_id, result)) in level_steps.iter().zip(batch_results) {
                    budget.increment_steps();
                    if let Err(e) =
                        self.db
                            .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
                    {
                        tracing::warn!(error = %e, step_id = %step_id, "save_step_duration DB call failed (ignored)");
                    }
                    match result {
                        Ok(output) => {
                            if let Err(e) = self.db.save_step_output(
                                &step_id,
                                &self.plan.plan_id,
                                &output,
                                &self.obs_config,
                            ) {
                                tracing::warn!(error = %e, step_id = %step_id, "save_step_output DB call failed (ignored)");
                            }
                            if let Err(e) =
                                self.db.complete_step(&self.plan.plan_id, &step_id, &output)
                            {
                                tracing::warn!(error = %e, step_id = %step_id, "complete_step DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepCompleted {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                duration_ms,
                            });
                            self.record_step_memory(&step_id, &step.description, &output);
                            completed_outputs.insert(step_id, output);
                        }
                        Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                &e.to_string(),
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db
                                    .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                error: e.to_string(),
                                retryable: true,
                            });
                            return self
                                .replan_and_continue(
                                    step_id,
                                    e.to_string(),
                                    completed_outputs,
                                    tool_proxy,
                                    llm_router,
                                    budget,
                                    reasoner,
                                )
                                .await;
                        }
                        Err(ref e) if e.is_retryable() => {
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                &e.to_string(),
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db
                                    .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db.fail_plan(&self.plan.plan_id, "MAX_REPLAN_EXCEEDED")
                            {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                error: e.to_string(),
                                retryable: true,
                            });
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: "MAX_REPLAN_EXCEEDED".to_string(),
                            });
                            return AIPResult::failed(
                                "MAX_REPLAN_EXCEEDED",
                                &format!("{} replanifications dépassées", self.max_replans),
                            );
                        }
                        Err(StepError::RejectedByUser { ref reason }) => {
                            if let Err(db_err) =
                                self.db
                                    .save_step_error(&step_id, &self.plan.plan_id, reason)
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db.fail_step(&self.plan.plan_id, &step_id, reason)
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, "REJECTED") {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: "REJECTED".to_string(),
                            });
                            return AIPResult::failed("REJECTED", reason);
                        }
                        Err(StepError::ApprovalChannelClosed) => {
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                "approval_channel_closed",
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) = self.db.fail_step(
                                &self.plan.plan_id,
                                &step_id,
                                "approval_channel_closed",
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) = self
                                .db
                                .fail_plan(&self.plan.plan_id, "APPROVAL_CHANNEL_CLOSED")
                            {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: "APPROVAL_CHANNEL_CLOSED".to_string(),
                            });
                            return AIPResult::failed(
                                "APPROVAL_CHANNEL_CLOSED",
                                "Approval channel closed — runtime shutting down",
                            );
                        }
                        Err(e) => {
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                &e.to_string(),
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db
                                    .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db.fail_plan(&self.plan.plan_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                error: e.to_string(),
                                retryable: false,
                            });
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: e.to_string(),
                            });
                            return AIPResult::failed(
                                "STEP_FAILED",
                                &format!("Step {} failed: {}", step_id, e),
                            );
                        }
                    }
                }
            } else {
                // ── SEQUENTIAL PATH ────────────────────────────────────────────────
                // Each step in the level is processed one at a time (unchanged logic).
                for step_id in level_ids {
                    let step = match self.plan.steps.iter().find(|s| s.step_id == step_id) {
                        Some(s) => s.clone(),
                        None => continue,
                    };

                    // : vérifier le budget avant chaque step.
                    if budget.is_exhausted() {
                        if let Err(e) = self
                            .db
                            .fail_plan(&self.plan.plan_id, "STEP_BUDGET_EXCEEDED")
                        {
                            tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                        }
                        let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            reason: "STEP_BUDGET_EXCEEDED".to_string(),
                        });
                        return AIPResult::failed(
                            "STEP_BUDGET_EXCEEDED",
                            &format!("Budget de {} steps atteint", budget.max_steps),
                        );
                    }

                    // Émettre StepStarted.
                    let step_num = completed_outputs.len() + 1;
                    let total = self.plan.steps.len();
                    let _ = self.event_bus.send(RuntimeEvent::StepStarted {
                        task_id: self.plan.task_id.clone().into(),
                        plan_id: self.plan.plan_id.clone(),
                        step_id: step_id.clone(),
                        step_num,
                        total,
                        desc: step.description.clone(),
                    });
                    if let Err(e) = self.db.start_step(&self.plan.plan_id, &step_id) {
                        tracing::warn!(error = %e, step_id = %step_id, "start_step DB call failed (ignored)");
                    }

                    // persist rendered input + tool name before execution.
                    let rendered_input = interpolate_outputs(&step.description, &completed_outputs);
                    if let Err(e) = self.db.save_step_input(
                        &step_id,
                        &self.plan.plan_id,
                        &rendered_input,
                        &self.obs_config,
                    ) {
                        tracing::warn!(error = %e, step_id = %step_id, "save_step_input DB call failed (ignored)");
                    }
                    let actual_tool = step.tool_hint.as_deref().unwrap_or("llm");
                    if let Err(e) =
                        self.db
                            .save_step_tool(&step_id, &self.plan.plan_id, actual_tool)
                    {
                        tracing::warn!(error = %e, step_id = %step_id, "save_step_tool DB call failed (ignored)");
                    }

                    // build StepContext with accumulated outputs and budget snapshot.
                    let step_ctx = StepContext {
                        previous_outputs: completed_outputs.clone(),
                        step_index: completed_outputs.len(),
                        total_steps: self.plan.steps.len(),
                        remaining_budget: budget.to_budget_view(),
                    };

                    let started = Instant::now();
                    let result = self
                        .execute_step(&step, &step_ctx, tool_proxy, llm_router)
                        .await;
                    let duration_ms = started.elapsed().as_millis() as u64;
                    budget.increment_steps();

                    // persist duration unconditionally.
                    if let Err(e) =
                        self.db
                            .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
                    {
                        tracing::warn!(error = %e, step_id = %step_id, "save_step_duration DB call failed (ignored)");
                    }

                    match result {
                        Ok(output) => {
                            // persist observability output.
                            if let Err(e) = self.db.save_step_output(
                                &step_id,
                                &self.plan.plan_id,
                                &output,
                                &self.obs_config,
                            ) {
                                tracing::warn!(error = %e, step_id = %step_id, "save_step_output DB call failed (ignored)");
                            }
                            if let Err(e) =
                                self.db.complete_step(&self.plan.plan_id, &step_id, &output)
                            {
                                tracing::warn!(error = %e, step_id = %step_id, "complete_step DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepCompleted {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                duration_ms,
                            });

                            // record episodic memory per step (fire-and-forget).
                            self.record_step_memory(&step_id, &step.description, &output);

                            completed_outputs.insert(step_id, output);
                        }

                        Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                            // persist error detail.
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                &e.to_string(),
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db
                                    .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                error: e.to_string(),
                                retryable: true,
                            });
                            return self
                                .replan_and_continue(
                                    step_id,
                                    e.to_string(),
                                    completed_outputs,
                                    tool_proxy,
                                    llm_router,
                                    budget,
                                    reasoner,
                                )
                                .await;
                        }

                        Err(ref e) if e.is_retryable() => {
                            // replan_count >= max_replans : MAX_REPLAN_EXCEEDED
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                &e.to_string(),
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db
                                    .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db.fail_plan(&self.plan.plan_id, "MAX_REPLAN_EXCEEDED")
                            {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                error: e.to_string(),
                                retryable: true,
                            });
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: "MAX_REPLAN_EXCEEDED".to_string(),
                            });
                            return AIPResult::failed(
                                "MAX_REPLAN_EXCEEDED",
                                &format!("{} replanifications dépassées", self.max_replans),
                            );
                        }

                        // : rejet humain → plan stoppé, steps suivants non exécutés.
                        Err(StepError::RejectedByUser { ref reason }) => {
                            if let Err(db_err) =
                                self.db
                                    .save_step_error(&step_id, &self.plan.plan_id, reason)
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db.fail_step(&self.plan.plan_id, &step_id, reason)
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, "REJECTED") {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: "REJECTED".to_string(),
                            });
                            return AIPResult::failed("REJECTED", reason);
                        }

                        // Fermeture du channel d'approbation → runtime en arrêt.
                        Err(StepError::ApprovalChannelClosed) => {
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                "approval_channel_closed",
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) = self.db.fail_step(
                                &self.plan.plan_id,
                                &step_id,
                                "approval_channel_closed",
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) = self
                                .db
                                .fail_plan(&self.plan.plan_id, "APPROVAL_CHANNEL_CLOSED")
                            {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: "APPROVAL_CHANNEL_CLOSED".to_string(),
                            });
                            return AIPResult::failed(
                                "APPROVAL_CHANNEL_CLOSED",
                                "Approval channel closed — runtime shutting down",
                            );
                        }

                        Err(e) => {
                            // Échec permanent non-retryable.
                            if let Err(db_err) = self.db.save_step_error(
                                &step_id,
                                &self.plan.plan_id,
                                &e.to_string(),
                            ) {
                                tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db
                                    .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                            }
                            if let Err(db_err) =
                                self.db.fail_plan(&self.plan.plan_id, &e.to_string())
                            {
                                tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                            }
                            let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                step_id: step_id.clone(),
                                error: e.to_string(),
                                retryable: false,
                            });
                            let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                                task_id: self.plan.task_id.clone().into(),
                                plan_id: self.plan.plan_id.clone(),
                                reason: e.to_string(),
                            });
                            return AIPResult::failed(
                                "STEP_FAILED",
                                &format!("Step {} failed: {}", step_id, e),
                            );
                        }
                    }
                } // closes `for step_id in level_ids`
            } // closes `else { // sequential path`
        } // closes `for level_ids in levels`

        // Tous les steps complétés.
        if let Err(e) = self.db.complete_plan(&self.plan.plan_id) {
            tracing::warn!(error = %e, "complete_plan DB call failed (ignored)");
        }
        let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_count: completed_outputs.len(),
            duration_ms: 0,
        });

        AIPResult::completed_with_steps(completed_outputs)
    }

    /// Executes a batch of tool-only steps, parallelising when all tools are read-only.
    ///
    /// **Parallel path** — when every step in `steps` targets a read-only tool
    /// (as reported by [`ToolProxyTrait::is_tool_read_only`]) **and** no tool in the
    /// batch requires human approval: all invocations are driven concurrently via
    /// `futures::stream::StreamExt::buffered` with a cap of
    /// `MAX_CONCURRENT_READ_TOOLS` simultaneous calls.
    ///
    /// **Serial path** — in all other cases (LLM steps, mutating tools, tools
    /// requiring approval, or a batch of one): invocations run sequentially.
    ///
    /// Output order matches input order in both paths.
    /// Inputs are interpolated from `completed_outputs` before invocation.
    async fn execute_tool_steps(
        &self,
        steps: &[PlanStep],
        completed_outputs: &HashMap<String, String>,
        tool_proxy: &dyn ToolProxyTrait,
    ) -> Vec<(String, Result<String, StepError>)> {
        use futures::stream::{self, StreamExt};

        if steps.is_empty() {
            return vec![];
        }

        let all_read_only = steps.len() > 1
            && steps.iter().all(|s| {
                s.tool_hint.as_deref().is_some_and(|t| {
                    t != "llm"
                        && tool_proxy.is_tool_read_only(t)
                        && !self
                            .manifest
                            .tools_requiring_approval
                            .iter()
                            .any(|a| a == t)
                })
            });

        // Pre-compute per-call data to avoid lifetime issues with the async stream.
        let tool_names: Vec<String> = steps
            .iter()
            .map(|s| s.tool_hint.clone().unwrap_or_default())
            .collect();
        let inputs: Vec<serde_json::Value> = steps
            .iter()
            .map(|s| serde_json::json!({"input": interpolate_outputs(&s.description, completed_outputs)}))
            .collect();
        let step_ids: Vec<String> = steps.iter().map(|s| s.step_id.clone()).collect();

        let raw_results: Vec<Result<String, String>> = if all_read_only {
            stream::iter((0..steps.len()).map(|i| tool_proxy.invoke(&tool_names[i], &inputs[i])))
                .buffered(MAX_CONCURRENT_ORIA_TOOLS)
                .collect::<Vec<_>>()
                .await
        } else {
            let mut results = Vec::with_capacity(steps.len());
            for i in 0..steps.len() {
                results.push(tool_proxy.invoke(&tool_names[i], &inputs[i]).await);
            }
            results
        };

        step_ids
            .into_iter()
            .zip(raw_results)
            .map(|(step_id, r)| (step_id, r.map_err(StepError::ToolCallFailed)))
            .collect()
    }

    /// Exécute un step individuel — outil ou LLM selon `tool_hint`.
    ///
    /// Avant l'exécution effective, vérifie si l'outil du step est dans
    /// `manifest.tools_requiring_approval`. Si oui et que `pending_approvals` est
    /// configuré, appelle [`suspend_for_approval`] et attend la décision humaine.
    ///
    /// - `tool_hint = Some("llm")` ou `None` → appel LLM, routé via `model_hint`
    ///   si présent, sinon backend défaut. Les outputs précédents sont
    ///   injectés dans le system message.
    /// - `tool_hint = Some(tool_name)` → appel via `ToolProxyTrait::invoke`
    ///   (`model_hint` ignoré pour les steps outil).
    ///
    /// Les outputs des steps précédents sont interpolés dans la description du step
    /// via [`interpolate_outputs`] avant d'être transmis à l'outil ou au LLM.
    ///
    /// [`suspend_for_approval`]: ActorLoop::suspend_for_approval
    async fn execute_step(
        &self,
        step: &PlanStep,
        step_ctx: &StepContext,
        tool_proxy: &dyn ToolProxyTrait,
        llm_router: &LlmRouter,
    ) -> Result<String, StepError> {
        // Vérifier si l'outil du step nécessite une approbation humaine.
        let tool_needs_approval = step
            .tool_hint
            .as_deref()
            .map(|t| {
                self.manifest
                    .tools_requiring_approval
                    .iter()
                    .any(|a| a == t)
            })
            .unwrap_or(false);

        if tool_needs_approval {
            if let Some(pending) = self.pending_approvals.as_ref() {
                self.suspend_for_approval(step, pending).await?;
            } else {
                tracing::warn!(
                    step_id = %step.step_id,
                    tool = ?step.tool_hint,
                    "PendingApprovals not configured — executing sensitive step without approval"
                );
            }
        }

        // Exécution normale du step après approbation (ou si outil non-sensible).
        let input = interpolate_outputs(&step.description, &step_ctx.previous_outputs);

        match step.tool_hint.as_deref() {
            // Step LLM — routé vers le backend spécifié par model_hint.
            // previous outputs injected into the system message.
            Some("llm") | None => {
                self.execute_llm_step(step, input, llm_router, step_ctx)
                    .await
            }
            // Step outil — model_hint ignoré.
            Some(tool_name) => tool_proxy
                .invoke(tool_name, &serde_json::json!({"input": input}))
                .await
                .map_err(StepError::ToolCallFailed),
        }
    }

    /// Exécute un appel LLM pour un step, en tenant compte du `model_hint`.
    ///
    /// - Si `model_hint = Some(hint)` et que le backend existe dans le `LlmRouter`,
    ///   l'appel est routé vers ce backend.
    /// - Si `model_hint = Some(hint)` mais le backend n'existe pas, un `tracing::warn!`
    ///   est émis et le backend par défaut est utilisé en fallback.
    /// - Si `model_hint = None`, le backend par défaut est utilisé.
    /// - si des steps précédents ont complété, leurs outputs sont
    ///   formatés dans un system message `"Previous step results:\n- s1: …"`.
    async fn execute_llm_step(
        &self,
        step: &PlanStep,
        input: String,
        llm_router: &LlmRouter,
        step_ctx: &StepContext,
    ) -> Result<String, StepError> {
        let mut messages = Vec::new();
        // inject previous step outputs as system context.
        if let Some(context_text) = step_ctx.format_previous_outputs() {
            messages.push(ChatMessage::system(context_text));
        }
        messages.push(ChatMessage::user(input));

        let request = CompletionRequest {
            messages,
            ..Default::default()
        };

        let backend_name = match &step.model_hint {
            Some(hint) => {
                if llm_router.get(Some(hint)).is_some() {
                    Some(hint.as_str())
                } else {
                    tracing::warn!(
                        step_id = %step.step_id,
                        model_hint = %hint,
                        "model_hint backend not found, falling back to default"
                    );
                    None
                }
            }
            None => None,
        };

        let obs = LlmObsConfig::default();
        let response = llm_router
            .complete_with_observability(backend_name, request, Some(&self.event_bus), &obs)
            .await
            .map_err(|e| StepError::LlmCallFailed(e.to_string()))?;

        Ok(response.content)
    }

    /// Suspend l'exécution du step et attend la décision humaine (HITL Mode Orchestré).
    ///
    /// ## Séquence
    ///
    /// 1. Enregistre un oneshot channel dans `pending_approvals` → récepteur `rx`.
    /// 2. Émet [`RuntimeEvent::TaskInputRequired`] avec `step_id: Some(step.step_id)`
    ///    sur l'`EventBus` pour notifier l'utilisateur.
    /// 3. Attend `rx.await` — le `ResumeHandler` envoie sur le sender.
    /// 4. Si `approved=true` → `Ok(())` → l'outil du step est exécuté normalement.
    /// 5. Si `approved=false` → `Err(StepError::RejectedByUser { reason })`.
    /// 6. Si le channel est fermé (shutdown runtime) → `Err(StepError::ApprovalChannelClosed)`.
    ///
    /// **StepBudget pausé pendant suspension** : l'attente est un `await` pur —
    /// le compteur de steps ne progresse pas pendant la suspension humaine.
    async fn suspend_for_approval(
        &self,
        step: &PlanStep,
        pending_approvals: &PendingApprovals,
    ) -> Result<(), StepError> {
        // Clé d'enregistrement : task_id + step_id pour identifier précisément la suspension.
        let approval_key = format!("{}::{}", self.plan.task_id, step.step_id);

        // 1. Enregistrer dans PendingApprovals → rx
        let rx = pending_approvals.register(&approval_key);

        // 2. Émettre TaskInputRequired avec step_id renseigné (distingue Mode Direct / Orchestré)
        let prompt = format!(
            "Approbation requise avant d'exécuter '{}' (step: {})",
            step.tool_hint.as_deref().unwrap_or("llm"),
            step.step_id
        );
        let _ = self.event_bus.send(RuntimeEvent::TaskInputRequired {
            task_id: self.plan.task_id.clone().into(),
            prompt,
            step_id: Some(step.step_id.clone()),
        });

        tracing::info!(
            task_id = %self.plan.task_id,
            step_id = %step.step_id,
            tool = ?step.tool_hint,
            "step suspended — waiting for human approval"
        );

        // 3. Attendre la décision humaine (await pur — StepBudget ne progresse pas)
        let response = rx.await.map_err(|_| StepError::ApprovalChannelClosed)?;

        tracing::info!(
            task_id = %self.plan.task_id,
            step_id = %step.step_id,
            approved = response.approved,
            "human decision received for step"
        );

        // 4/5. Retourner selon la décision
        if response.approved {
            Ok(())
        } else {
            Err(StepError::RejectedByUser {
                reason: response
                    .reason
                    .unwrap_or_else(|| "Rejeté par l'utilisateur".into()),
            })
        }
    }

    /// Records an episodic memory entry for a completed step.
    ///
    /// Fire-and-forget: errors are logged as warnings but never interrupt execution.
    /// Skipped silently when `memory_manager` is `None` or when the agent manifest
    /// has no `memory_namespace` configured.
    ///
    /// Output is truncated to [`STEP_MEMORY_OUTPUT_MAX_CHARS`] characters.
    fn record_step_memory(&self, step_id: &str, description: &str, output: &str) {
        // skip if no memory_manager or no namespace configured.
        let mm = match self.memory_manager.as_ref() {
            Some(mm) => mm,
            None => return,
        };
        let namespace = match self.manifest.memory_namespace.as_deref() {
            Some(ns) => ns,
            None => return,
        };

        let truncated_output = truncate_chars(output, self.step_memory_max_chars);
        let content = format!("step {step_id}: {description} -> {truncated_output}");
        let task_id = self.plan.task_id.clone();
        let agent_name = self.manifest.name.clone();
        let namespace_owned = namespace.to_string();
        let metadata = serde_json::json!({
            "source": "oria_orchestrated",
            "step_id": step_id,
        });

        let mm = Arc::clone(mm);
        // Fire-and-forget: spawn_blocking for the sync SQLite write.
        tokio::task::spawn_blocking(move || {
            let mut guard = match mm.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        step_id = %task_id,
                        "failed to acquire memory_manager lock for step memory (ignored)"
                    );
                    return;
                }
            };
            let store = match guard.store(&namespace_owned) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        namespace = %namespace_owned,
                        "failed to open memory store for step memory (ignored)"
                    );
                    return;
                }
            };
            let episodic = apollia_memory::episodic::EpisodicMemory::new(store);
            if let Err(e) = episodic.record(
                &namespace_owned,
                &agent_name,
                &content,
                STEP_MEMORY_IMPORTANCE,
                Some(task_id.as_str()),
                None,
                Some(&metadata),
            ) {
                // warn but don't interrupt execution.
                tracing::warn!(
                    error = %e,
                    namespace = %namespace_owned,
                    "failed to record episodic step memory (ignored)"
                );
            }
        });
    }

    /// Déclenche une replanification après l'échec retryable d'un step.
    ///
    /// Incrémente `replan_count`, émet [`RuntimeEvent::PlanReplanning`],
    /// appelle `Reasoner::replan()`, met à jour le plan SQLite et l'état interne,
    /// puis délègue la suite à [`execute_remaining`](Self::execute_remaining).
    ///
    /// Retourne `MAX_REPLAN_EXCEEDED` si le Reasoner échoue.
    ///
    /// Cette fonction retourne un `Future` boxé pour permettre la récursion mutuelle
    /// avec [`execute_remaining`](Self::execute_remaining).
    #[allow(clippy::too_many_arguments)]
    fn replan_and_continue<'a>(
        &'a mut self,
        failed_step_id: String,
        error_message: String,
        completed_outputs: HashMap<String, String>,
        tool_proxy: &'a dyn ToolProxyTrait,
        llm_router: &'a LlmRouter,
        budget: &'a StepBudget,
        reasoner: &'a Reasoner,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + 'a>> {
        Box::pin(async move {
            self.replan_count += 1;
            let attempt = self.replan_count;

            let _ = self.event_bus.send(RuntimeEvent::PlanReplanning {
                task_id: self.plan.task_id.clone().into(),
                plan_id: self.plan.plan_id.clone(),
                attempt,
                failed_step: failed_step_id.clone(),
                reason: error_message.clone(),
            });

            // Construire un contexte minimal pour le Reasoner.
            let ctx = build_replan_context(&self.plan);

            let new_plan = match reasoner
                .replan(&ctx, &completed_outputs, &failed_step_id, &error_message)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, &e.to_string()) {
                        tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                    }
                    let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                        task_id: self.plan.task_id.clone().into(),
                        plan_id: self.plan.plan_id.clone(),
                        reason: e.to_string(),
                    });
                    return AIPResult::failed("REPLAN_FAILED", &e.to_string());
                }
            };

            // Mettre à jour SQLite : begin_replan supprime les steps pending, on réinsère.
            if let Err(e) = self.db.begin_replan(&self.plan.plan_id, self.replan_count) {
                tracing::warn!(error = %e, "begin_replan DB call failed (ignored)");
            }
            if let Err(e) = self.db.insert_steps(&self.plan.plan_id, &new_plan.steps) {
                tracing::warn!(error = %e, "insert_steps DB call failed (ignored)");
            }

            let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
                task_id: self.plan.task_id.clone().into(),
                agent_name: String::new(),
                plan_id: self.plan.plan_id.clone(),
                step_count: new_plan.steps.len(),
            });

            // Mettre à jour l'état interne : conserver uniquement les steps complétés + nouveaux.
            self.plan
                .steps
                .retain(|s| completed_outputs.contains_key(&s.step_id));
            self.plan.steps.extend(new_plan.steps);

            self.execute_remaining(completed_outputs, tool_proxy, llm_router, budget, reasoner)
                .await
        }) // end Box::pin
    }

    /// Exécute les steps restants (non encore complétés) après une replanification.
    ///
    /// Détermine les steps restants en filtrant `self.plan.steps` sur ceux absents
    /// de `completed_outputs`, effectue un tri topologique, puis exécute chacun.
    ///
    /// Cette fonction retourne un `Future` boxé pour permettre la récursion mutuelle
    /// avec [`replan_and_continue`](Self::replan_and_continue).
    fn execute_remaining<'a>(
        &'a mut self,
        mut completed_outputs: HashMap<String, String>,
        tool_proxy: &'a dyn ToolProxyTrait,
        llm_router: &'a LlmRouter,
        budget: &'a StepBudget,
        reasoner: &'a Reasoner,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + 'a>> {
        Box::pin(async move {
            let remaining: Vec<PlanStep> = self
                .plan
                .steps
                .iter()
                .filter(|s| !completed_outputs.contains_key(&s.step_id))
                .cloned()
                .collect();

            let order = match topological_sort(&remaining) {
                Ok(o) => o,
                Err(_) => {
                    if let Err(e) = self.db.fail_plan(&self.plan.plan_id, "INVALID_REPLAN") {
                        tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                    }
                    return AIPResult::failed("INVALID_REPLAN", "Circular dependency in replan");
                }
            };

            for step_id in order {
                let step = match remaining.iter().find(|s| s.step_id == step_id) {
                    Some(s) => s.clone(),
                    None => continue,
                };

                if budget.is_exhausted() {
                    if let Err(e) = self
                        .db
                        .fail_plan(&self.plan.plan_id, "STEP_BUDGET_EXCEEDED")
                    {
                        tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                    }
                    let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                        task_id: self.plan.task_id.clone().into(),
                        plan_id: self.plan.plan_id.clone(),
                        reason: "STEP_BUDGET_EXCEEDED".to_string(),
                    });
                    return AIPResult::failed(
                        "STEP_BUDGET_EXCEEDED",
                        "Budget épuisé lors de la replanification",
                    );
                }

                let step_num = completed_outputs.len() + 1;
                let total = self.plan.steps.len();
                let _ = self.event_bus.send(RuntimeEvent::StepStarted {
                    task_id: self.plan.task_id.clone().into(),
                    plan_id: self.plan.plan_id.clone(),
                    step_id: step_id.clone(),
                    step_num,
                    total,
                    desc: step.description.clone(),
                });
                if let Err(e) = self.db.start_step(&self.plan.plan_id, &step_id) {
                    tracing::warn!(error = %e, step_id = %step_id, "start_step DB call failed (ignored)");
                }

                // persist rendered input + tool name before execution.
                let rendered_input = interpolate_outputs(&step.description, &completed_outputs);
                if let Err(e) = self.db.save_step_input(
                    &step_id,
                    &self.plan.plan_id,
                    &rendered_input,
                    &self.obs_config,
                ) {
                    tracing::warn!(error = %e, step_id = %step_id, "save_step_input DB call failed (ignored)");
                }
                let actual_tool = step.tool_hint.as_deref().unwrap_or("llm");
                if let Err(e) = self
                    .db
                    .save_step_tool(&step_id, &self.plan.plan_id, actual_tool)
                {
                    tracing::warn!(error = %e, step_id = %step_id, "save_step_tool DB call failed (ignored)");
                }

                // build StepContext for execute_remaining steps.
                let step_ctx = StepContext {
                    previous_outputs: completed_outputs.clone(),
                    step_index: completed_outputs.len(),
                    total_steps: self.plan.steps.len(),
                    remaining_budget: budget.to_budget_view(),
                };

                let started = Instant::now();
                let result = self
                    .execute_step(&step, &step_ctx, tool_proxy, llm_router)
                    .await;
                let duration_ms = started.elapsed().as_millis() as u64;
                budget.increment_steps();

                // persist duration unconditionally.
                if let Err(e) =
                    self.db
                        .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
                {
                    tracing::warn!(error = %e, step_id = %step_id, "save_step_duration DB call failed (ignored)");
                }

                match result {
                    Ok(output) => {
                        // persist observability output.
                        if let Err(e) = self.db.save_step_output(
                            &step_id,
                            &self.plan.plan_id,
                            &output,
                            &self.obs_config,
                        ) {
                            tracing::warn!(error = %e, step_id = %step_id, "save_step_output DB call failed (ignored)");
                        }
                        if let Err(e) = self.db.complete_step(&self.plan.plan_id, &step_id, &output)
                        {
                            tracing::warn!(error = %e, step_id = %step_id, "complete_step DB call failed (ignored)");
                        }
                        let _ = self.event_bus.send(RuntimeEvent::StepCompleted {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            step_id: step_id.clone(),
                            duration_ms,
                        });
                        completed_outputs.insert(step_id, output);
                    }

                    Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                        if let Err(db_err) =
                            self.db
                                .save_step_error(&step_id, &self.plan.plan_id, &e.to_string())
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                        }
                        if let Err(db_err) =
                            self.db
                                .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                        }
                        let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            step_id: step_id.clone(),
                            error: e.to_string(),
                            retryable: true,
                        });
                        return self
                            .replan_and_continue(
                                step_id,
                                e.to_string(),
                                completed_outputs,
                                tool_proxy,
                                llm_router,
                                budget,
                                reasoner,
                            )
                            .await;
                    }

                    Err(ref e) if e.is_retryable() => {
                        // replan_count >= max_replans : MAX_REPLAN_EXCEEDED.
                        if let Err(db_err) =
                            self.db
                                .save_step_error(&step_id, &self.plan.plan_id, &e.to_string())
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                        }
                        if let Err(db_err) =
                            self.db
                                .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                        }
                        if let Err(db_err) =
                            self.db.fail_plan(&self.plan.plan_id, "MAX_REPLAN_EXCEEDED")
                        {
                            tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                        }
                        let _ = self.event_bus.send(RuntimeEvent::StepFailed {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            step_id: step_id.clone(),
                            error: e.to_string(),
                            retryable: true,
                        });
                        let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            reason: "MAX_REPLAN_EXCEEDED".to_string(),
                        });
                        return AIPResult::failed(
                            "MAX_REPLAN_EXCEEDED",
                            &format!("{} replanifications dépassées", self.max_replans),
                        );
                    }

                    // Rejet humain dans execute_remaining → plan stoppé.
                    Err(StepError::RejectedByUser { ref reason }) => {
                        if let Err(db_err) =
                            self.db
                                .save_step_error(&step_id, &self.plan.plan_id, reason)
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                        }
                        if let Err(db_err) = self.db.fail_step(&self.plan.plan_id, &step_id, reason)
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                        }
                        if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, "REJECTED") {
                            tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                        }
                        let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            reason: "REJECTED".to_string(),
                        });
                        return AIPResult::failed("REJECTED", reason);
                    }

                    // Channel fermé dans execute_remaining → runtime en arrêt.
                    Err(StepError::ApprovalChannelClosed) => {
                        if let Err(db_err) = self.db.save_step_error(
                            &step_id,
                            &self.plan.plan_id,
                            "approval_channel_closed",
                        ) {
                            tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                        }
                        if let Err(db_err) = self.db.fail_step(
                            &self.plan.plan_id,
                            &step_id,
                            "approval_channel_closed",
                        ) {
                            tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                        }
                        if let Err(db_err) = self
                            .db
                            .fail_plan(&self.plan.plan_id, "APPROVAL_CHANNEL_CLOSED")
                        {
                            tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                        }
                        let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            reason: "APPROVAL_CHANNEL_CLOSED".to_string(),
                        });
                        return AIPResult::failed(
                            "APPROVAL_CHANNEL_CLOSED",
                            "Approval channel closed — runtime shutting down",
                        );
                    }

                    Err(e) => {
                        if let Err(db_err) =
                            self.db
                                .save_step_error(&step_id, &self.plan.plan_id, &e.to_string())
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "save_step_error DB call failed (ignored)");
                        }
                        if let Err(db_err) =
                            self.db
                                .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                        {
                            tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                        }
                        if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, &e.to_string()) {
                            tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                        }
                        let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                            task_id: self.plan.task_id.clone().into(),
                            plan_id: self.plan.plan_id.clone(),
                            reason: e.to_string(),
                        });
                        return AIPResult::failed("STEP_FAILED", &e.to_string());
                    }
                }
            }

            if let Err(e) = self.db.complete_plan(&self.plan.plan_id) {
                tracing::warn!(error = %e, "complete_plan DB call failed (ignored)");
            }
            let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                task_id: self.plan.task_id.clone().into(),
                plan_id: self.plan.plan_id.clone(),
                step_count: completed_outputs.len(),
                duration_ms: 0,
            });

            AIPResult::completed_with_steps(completed_outputs)
        }) // end Box::pin
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// Interpole les outputs des steps précédents dans la description d'un step.
///
/// Remplace chaque occurrence de `{{step_id}}` par le contenu de l'output
/// du step correspondant. Les placeholders non reconnus sont laissés intacts.
///
/// # Exemple
///
/// ```text
/// "Analyser {{s1}} et {{s2}}" + {s1: "42 pages", s2: "3 images"}
/// → "Analyser 42 pages et 3 images"
/// ```
pub fn interpolate_outputs(description: &str, outputs: &HashMap<String, String>) -> String {
    let mut result = description.to_string();
    for (step_id, output) in outputs {
        result = result.replace(&format!("{{{{{step_id}}}}}"), output);
    }
    result
}

/// Construit un [`ContextBundle`] minimal pour la replanification.
///
/// Le bundle contient uniquement le `task_id` du plan ; les autres champs
/// (`memory_snapshot`, `available_tools`, `manifest_system_prompt`) sont vides.
/// Le Reasoner utilise ce contexte pour construire le prompt replanner.
fn build_replan_context(plan: &ExecutionPlan) -> ContextBundle {
    use apollia_core::task::AIPTask;

    ContextBundle {
        task: AIPTask {
            task_id: plan.task_id.clone(),
            ..AIPTask::default()
        },
        memory_snapshot: None,
        execution_mode: ExecutionMode::Orchestrated,
        available_tools: vec![],
        manifest_system_prompt: None,
        llm_backend_names: vec![],
    }
}

/// Truncates a string to at most `max_chars` Unicode characters.
///
/// If the string exceeds `max_chars`, it is truncated and `"…"` is appended.
/// UTF-8 safe: operates on `char` boundaries, not bytes.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, PendingApprovals, TaskStatus};
    use apollia_llm::{CompletionRequest, CompletionResponse, FinishReason, LlmError, TokenUsage};
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    /// Construit un `AgentManifest` minimal pour les tests.
    fn make_manifest() -> AgentManifest {
        serde_json::from_str(
            r#"{"name":"test","version":"0.1.0","description":"test","tools_required":[]}"#,
        )
        .expect("minimal manifest must deserialize")
    }

    // ── Mock ToolProxy ────────────────────────────────────────────────────────

    struct MockToolProxy {
        response: String,
    }

    #[async_trait::async_trait]
    impl ToolProxyTrait for MockToolProxy {
        async fn invoke(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(self.response.clone())
        }
    }

    struct FailingToolProxy;

    #[async_trait::async_trait]
    impl ToolProxyTrait for FailingToolProxy {
        async fn invoke(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            Err("tool timeout".to_string())
        }
    }

    // ── Mock CompletionModel ─────────────────────────────────────────────────

    struct MockCompletionModel {
        queue: Mutex<VecDeque<String>>,
    }

    impl MockCompletionModel {
        fn new(responses: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                queue: Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
            })
        }
    }

    #[async_trait::async_trait]
    impl apollia_llm::CompletionModel for MockCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            let content = {
                let mut q = self.queue.lock().expect("mock lock");
                q.pop_front().unwrap_or_else(|| "mock response".to_string())
            };
            Ok(CompletionResponse {
                content,
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError("mock does not stream".into()))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "mock"
        }

        fn model_id(&self) -> &str {
            "mock-model"
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_plan(steps: Vec<(&str, &[&str])>) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: "plan-001".into(),
            task_id: "task-001".into(),
            steps: steps
                .into_iter()
                .map(|(id, deps)| PlanStep {
                    step_id: id.into(),
                    description: format!("Step {id}"),
                    tool_hint: Some("mock_tool".into()),
                    depends_on: deps.iter().map(|s| s.to_string()).collect(),
                    model_hint: None,
                })
                .collect(),
        }
    }

    fn make_actor(
        plan: ExecutionPlan,
    ) -> (ActorLoop, tokio::sync::broadcast::Receiver<RuntimeEvent>) {
        let (bus_tx, bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");
        let actor = ActorLoop::new(plan, 2, db, bus_tx, make_manifest());
        (actor, bus_rx)
    }

    // ── Exécution séquentielle dans l'ordre topologique ───────────────

    /// GIVEN un plan (s1, s2→s1, s3→s2) et un ToolProxy mock qui retourne "ok"
    /// WHEN actor.execute() est appelé
    /// THEN AIPResult::Completed est retourné
    ///   ET les 3 steps sont dans l'output
    #[tokio::test]
    async fn test_ac1_execution_sequentielle() {
        // GIVEN
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"]), ("s3", &["s2"])]);
        let (mut actor, _rx) = make_actor(plan);
        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {result:?}"
        );
    }

    // ── StepBudget épuisé au step 3/5 ─────────────────────────────────

    /// GIVEN un plan de 5 steps et un StepBudget avec max_steps = 2
    /// WHEN actor.execute() est appelé
    /// THEN AIPResult::failed("STEP_BUDGET_EXCEEDED", _) est retourné
    #[tokio::test]
    async fn test_ac2_budget_epuise() {
        // GIVEN
        let plan = make_plan(vec![
            ("s1", &[]),
            ("s2", &[]),
            ("s3", &[]),
            ("s4", &[]),
            ("s5", &[]),
        ]);
        let (mut actor, _rx) = make_actor(plan);
        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::with_max(2);
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err = result.error.expect("expected error");
        assert_eq!(
            err.code, "STEP_BUDGET_EXCEEDED",
            "expected STEP_BUDGET_EXCEEDED, got: {}",
            err.code
        );
    }

    // ── Replanification déclenchée sur step retryable ─────────────────

    /// GIVEN un plan (s1 ok, s2 fail retryable, s3 pending) et un Reasoner mock
    ///        qui retourne un plan alternatif (s2b, s3)
    /// WHEN actor.execute() est appelé
    /// THEN PlanReplanning { attempt: 1 } est émis
    ///   ET l'exécution continue avec le plan alternatif
    #[tokio::test]
    async fn test_ac3_replanification_declenchee() {
        // GIVEN
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"]), ("s3", &["s2"])]);
        let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        // Proxy qui réussit pour s1, échoue pour s2
        struct SelectiveProxy {
            fail_next: std::sync::atomic::AtomicBool,
        }
        #[async_trait::async_trait]
        impl ToolProxyTrait for SelectiveProxy {
            async fn invoke(&self, _: &str, _: &serde_json::Value) -> Result<String, String> {
                if self
                    .fail_next
                    .swap(false, std::sync::atomic::Ordering::SeqCst)
                {
                    Err("tool timeout".into())
                } else {
                    Ok("ok".into())
                }
            }
        }
        // s1 succeeds, s2 fails, then s2b/s3 succeed
        let proxy = SelectiveProxy {
            fail_next: std::sync::atomic::AtomicBool::new(false),
        };
        // Set fail for s2 — we'll modify by position:
        // Proxy réussit toujours (default), on utilise FailingToolProxy pour s2 uniquement.
        // Simplification : proxy qui échoue au 2ème appel.
        struct NthFailProxy {
            call: std::sync::atomic::AtomicU32,
            fail_at: u32,
        }
        #[async_trait::async_trait]
        impl ToolProxyTrait for NthFailProxy {
            async fn invoke(&self, _: &str, _: &serde_json::Value) -> Result<String, String> {
                let n = self.call.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == self.fail_at {
                    Err("tool timeout".into())
                } else {
                    Ok(format!("output-{n}"))
                }
            }
        }

        let proxy2 = NthFailProxy {
            call: std::sync::atomic::AtomicU32::new(0),
            fail_at: 1, // s2 is the 2nd call (0-indexed)
        };

        // Plan alternatif fourni par le Reasoner mock
        let replacement_plan = r#"{"steps":[
            {"step_id":"s2b","description":"Retry step","tool_hint":"mock_tool","depends_on":[]},
            {"step_id":"s3","description":"Final step","tool_hint":"mock_tool","depends_on":["s2b"]}
        ]}"#;
        let model = MockCompletionModel::new(vec![replacement_plan]);
        let reasoner = Reasoner::new(model, 10);
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, make_manifest());
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();

        // WHEN
        let result = actor
            .execute(&proxy2, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — vérifie PlanReplanning dans le bus
        let mut found_replanning = false;
        while let Ok(event) = bus_rx.try_recv() {
            if let RuntimeEvent::PlanReplanning { attempt, .. } = event {
                assert_eq!(attempt, 1, "expected attempt=1");
                found_replanning = true;
            }
        }
        assert!(found_replanning, "PlanReplanning event not emitted");
        assert!(
            result.status == TaskStatus::Completed || result.status == TaskStatus::Failed,
            "unexpected status: {:?}",
            result.status
        );
        // L'exécution s'est replanifiée (pas MAX_REPLAN ni STEP_FAILED permanent)
        if let Some(ref err) = result.error {
            assert_ne!(
                err.code, "STEP_FAILED",
                "unexpected STEP_FAILED after replan"
            );
        }
    }

    // ── MAX_REPLAN_EXCEEDED après 2 replanifications ──────────────────

    /// GIVEN un plan où chaque step fail (retryable) et max_replans = 2
    /// WHEN actor.execute() est appelé
    /// THEN AIPResult::failed("MAX_REPLAN_EXCEEDED", _) est retourné
    #[tokio::test]
    async fn test_ac4_max_replan_exceeded() {
        // GIVEN
        let plan = make_plan(vec![("s1", &[])]);
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        // Le Reasoner retourne toujours un plan avec un step qui va échouer.
        // On simule en fournissant 3 plans identiques (un par replan attempt).
        // Chaque plan a un step s1 qui va échouer → replan → échoue encore.
        let failing_plan = r#"{"steps":[{"step_id":"s1b","description":"retry","tool_hint":"mock_tool","depends_on":[]}]}"#;
        let model = MockCompletionModel::new(vec![failing_plan, failing_plan]);
        let reasoner = Reasoner::new(model, 10);
        let proxy = FailingToolProxy;
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, make_manifest());
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err = result.error.expect("expected error");
        assert_eq!(
            err.code, "MAX_REPLAN_EXCEEDED",
            "expected MAX_REPLAN_EXCEEDED, got: {}",
            err.code
        );
    }

    // ── interpolate_outputs ───────────────────────────────────────────────────

    /// GIVEN une description avec {{s1}} et {{s2}} et des outputs correspondants
    /// WHEN interpolate_outputs() est appelé
    /// THEN les placeholders sont remplacés par les outputs
    #[test]
    fn test_interpolate_outputs() {
        // GIVEN
        let desc = "Analyser {{s1}} et {{s2}}";
        let mut outputs = HashMap::new();
        outputs.insert("s1".into(), "résultat 1".into());
        outputs.insert("s2".into(), "résultat 2".into());

        // WHEN
        let result = interpolate_outputs(desc, &outputs);

        // THEN
        assert_eq!(result, "Analyser résultat 1 et résultat 2");
    }

    // ── StepError::is_retryable ───────────────────────────────────────────────

    #[test]
    fn test_step_error_is_retryable() {
        assert!(StepError::ToolCallFailed("timeout".into()).is_retryable());
        assert!(StepError::LlmCallFailed("network error".into()).is_retryable());
        assert!(!StepError::NoLlmBackend.is_retryable());
        assert!(!StepError::ToolNotFound("bash".into()).is_retryable());
    }

    // ── Propagation manifest vers ActorLoop ───────────────────────────

    /// ÉTANT DONNÉ un AgentManifest avec tools_requiring_approval=["smtp"]
    /// QUAND un ActorLoop est créé avec ce manifest
    /// ALORS self.manifest.tools_requiring_approval contient "smtp"
    #[test]
    fn test_ac3_manifest_propagated_to_actor_loop() {
        // GIVEN
        let manifest: AgentManifest = serde_json::from_str(
            r#"{
                "name":"test","version":"0.1.0","description":"test","tools_required":[],
                "tools_requiring_approval":["smtp"]
            }"#,
        )
        .expect("manifest must deserialize");
        let plan = make_plan(vec![("s1", &[])]);
        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        // WHEN
        let actor = ActorLoop::new(plan, 2, db, bus_tx, manifest);

        // THEN
        assert!(
            actor
                .manifest
                .tools_requiring_approval
                .contains(&"smtp".to_string()),
            "expected 'smtp' in tools_requiring_approval"
        );
    }

    // ── HITL Mode Orchestré ──────────────────────────────────────

    /// Construit un `AgentManifest` avec `tools_requiring_approval` pour les tests HITL.
    fn make_manifest_with_approval(tools: &[&str]) -> AgentManifest {
        let tools_json = tools
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(",");
        serde_json::from_str(&format!(
            r#"{{"name":"hitl-agent","version":"0.1.0","description":"test","tools_required":[],
               "tools_requiring_approval":[{tools_json}]}}"#
        ))
        .expect("manifest must deserialize")
    }

    /// Construit un plan mono-step avec l'outil donné.
    fn make_plan_with_tool(step_id: &str, tool_name: &str, task_id: &str) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: format!("plan-{step_id}"),
            task_id: task_id.into(),
            steps: vec![PlanStep {
                step_id: step_id.into(),
                description: format!("Step {step_id} using {tool_name}"),
                tool_hint: Some(tool_name.into()),
                depends_on: vec![],
                model_hint: None,
            }],
        }
    }

    // Step avec outil sensible → suspension avant exécution
    //
    // ÉTANT DONNÉ un manifest avec tools_requiring_approval=["smtp"] et un step "s3" avec tool_hint="smtp"
    // QUAND execute() est appelé SANS résoudre le oneshot
    // ALORS l'outil "smtp" n'est PAS appelé avant l'approbation,
    //       RuntimeEvent::TaskInputRequired{step_id: Some("s3")} est émis
    #[tokio::test]
    async fn test_ac1_step_sensitive_tool_suspends_before_execution() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = make_plan_with_tool("s3", "smtp", "task-ac1");
        let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        struct CountingProxy(Arc<AtomicU32>);
        #[async_trait::async_trait]
        impl ToolProxyTrait for CountingProxy {
            async fn invoke(
                &self,
                _tool_name: &str,
                _input: &serde_json::Value,
            ) -> Result<String, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("tool output".into())
            }
        }

        let pending_clone = pending.clone();
        let call_count_for_bus = call_count.clone();
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest)
            .with_pending_approvals(Some(pending.clone()));

        let proxy = CountingProxy(call_count_clone);
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // ActorLoop is !Send (PlanRepository wraps RefCell<Connection>) — use tokio::join!
        // so both futures run on the same task without spawning.
        //
        // The observer future: waits for TaskInputRequired, asserts tool not called yet,
        // then resolves the oneshot to unblock actor.execute().
        let observer_fut = async move {
            let mut found_input_required = false;
            let mut found_step_id = false;
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
            loop {
                match tokio::time::timeout_at(deadline, bus_rx.recv()).await {
                    Ok(Ok(event)) => {
                        if let RuntimeEvent::TaskInputRequired { ref step_id, .. } = event {
                            found_input_required = true;
                            found_step_id = step_id.as_deref() == Some("s3");
                            break;
                        }
                    }
                    _ => break,
                }
            }

            // THEN — l'outil smtp n'a pas encore été appelé
            assert_eq!(
                call_count_for_bus.load(Ordering::SeqCst),
                0,
                "smtp tool must NOT be called before approval"
            );
            assert!(found_input_required, "TaskInputRequired event not emitted");
            assert!(found_step_id, "step_id should be Some(\"s3\")");

            // Résoudre pour débloquer actor.execute()
            let _ = pending_clone.resolve(
                "task-ac1::s3",
                apollia_core::result::InputResponseData {
                    approved: false,
                    reason: Some("test cleanup".into()),
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".into(),
                },
            );
        };

        tokio::join!(
            actor.execute(&proxy, &llm, &budget, &resilience, &reasoner),
            observer_fut
        );
    }

    // Approbation → step exécuté normalement
    //
    // ÉTANT DONNÉ un step "s3" avec tool_hint="smtp" suspendu
    // QUAND PendingApprovals.resolve(approved=true)
    // ALORS l'outil "smtp" est appelé, le plan se complète
    #[tokio::test]
    async fn test_ac2_approve_executes_step() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = make_plan_with_tool("s3", "smtp", "task-ac2");
        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        struct CountingProxy(Arc<AtomicU32>);
        #[async_trait::async_trait]
        impl ToolProxyTrait for CountingProxy {
            async fn invoke(
                &self,
                _tool_name: &str,
                _input: &serde_json::Value,
            ) -> Result<String, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("smtp sent".into())
            }
        }

        let pending_clone = pending.clone();
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest)
            .with_pending_approvals(Some(pending.clone()));

        let proxy = CountingProxy(call_count_clone);
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // ActorLoop is !Send — use tokio::join! instead of tokio::spawn.
        // The resolver future sleeps briefly then approves, unblocking actor.execute().
        let resolve_fut = async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            pending_clone
                .resolve(
                    "task-ac2::s3",
                    apollia_core::result::InputResponseData {
                        approved: true,
                        reason: None,
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve must succeed");
        };

        // WHEN
        let (result, _) = tokio::join!(
            actor.execute(&proxy, &llm, &budget, &resilience, &reasoner),
            resolve_fut
        );
        let result = result;

        // THEN — le plan se complète avec succès
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "plan should complete after approval: {result:?}"
        );
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "smtp tool must be called exactly once after approval"
        );
    }

    // Rejet → plan stoppé, AIPResult::failed("REJECTED") retourné,
    //         steps suivants non exécutés
    //
    // ÉTANT DONNÉ un plan [s1:file_io (non-sensible), s2:smtp (sensible)]
    // QUAND l'opérateur rejette s2
    // ALORS s2 n'est pas exécuté, plan retourne failed("REJECTED", reason)
    #[tokio::test]
    async fn test_ac3_reject_stops_plan() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = ExecutionPlan {
            plan_id: "plan-ac3".into(),
            task_id: "task-ac3".into(),
            steps: vec![
                PlanStep {
                    step_id: "s1".into(),
                    description: "Lire fichier".into(),
                    tool_hint: Some("file_io".into()),
                    depends_on: vec![],
                    model_hint: None,
                },
                PlanStep {
                    step_id: "s2".into(),
                    description: "Envoyer email".into(),
                    tool_hint: Some("smtp".into()),
                    depends_on: vec!["s1".into()],
                    model_hint: None,
                },
            ],
        };
        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        struct CountingProxy(Arc<AtomicU32>);
        #[async_trait::async_trait]
        impl ToolProxyTrait for CountingProxy {
            async fn invoke(
                &self,
                _tool_name: &str,
                _input: &serde_json::Value,
            ) -> Result<String, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("ok".into())
            }
        }

        let pending_clone = pending.clone();
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest)
            .with_pending_approvals(Some(pending.clone()));

        let proxy = CountingProxy(call_count_clone);
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // ActorLoop is !Send — use tokio::join! instead of tokio::spawn.
        // The resolver future sleeps briefly then rejects s2, causing the plan to fail.
        let resolve_fut = async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            pending_clone
                .resolve(
                    "task-ac3::s2",
                    apollia_core::result::InputResponseData {
                        approved: false,
                        reason: Some("Email non approuvé".into()),
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve must succeed");
        };

        // WHEN
        let (result, _) = tokio::join!(
            actor.execute(&proxy, &llm, &budget, &resilience, &reasoner),
            resolve_fut
        );
        let result = result;

        // THEN — plan retourne Failed avec code REJECTED
        assert_eq!(result.status, TaskStatus::Failed);
        let err = result.error.expect("error must be set");
        assert_eq!(err.code, "REJECTED");
        assert!(
            err.message.contains("Email non approuvé"),
            "reason must be propagated: {}",
            err.message
        );
        // s1 (file_io, non-sensible) a été exécuté mais s2 (smtp) a été rejeté avant appel
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "only s1 should be called — s2 rejected before tool call"
        );
    }

    // Step avec outil NON sensible → aucune suspension
    //
    // ÉTANT DONNÉ un step utilisant "file_io" absent de tools_requiring_approval
    // QUAND execute() est appelé avec PendingApprovals configuré
    // ALORS aucun TaskInputRequired émis, step s'exécute directement
    #[tokio::test]
    async fn test_ac5_non_sensitive_tool_no_suspension() {
        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = make_plan_with_tool("s1", "file_io", "task-ac5");
        let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let mut actor =
            ActorLoop::new(plan, 2, db, bus_tx, manifest).with_pending_approvals(Some(pending));

        let proxy = MockToolProxy {
            response: "file content".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN — exécuter sans aucun resolve (aucune suspension attendue)
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — le plan se complète directement sans suspension
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "non-sensitive tool must execute without suspension: {result:?}"
        );

        // THEN — aucun TaskInputRequired n'a été émis
        let mut found = false;
        while let Ok(event) = bus_rx.try_recv() {
            if matches!(event, RuntimeEvent::TaskInputRequired { .. }) {
                found = true;
            }
        }
        assert!(
            !found,
            "TaskInputRequired must NOT be emitted for non-sensitive tool"
        );
    }

    // StepError — les nouvelles variantes ne sont pas retryables
    #[test]
    fn test_step_error_rejected_and_closed_not_retryable() {
        assert!(!StepError::RejectedByUser {
            reason: "Non".into()
        }
        .is_retryable());
        assert!(!StepError::ApprovalChannelClosed.is_retryable());
    }

    // ── Multi-model dispatch via model_hint ─────────────────────

    /// Construit un plan avec un step LLM et un `model_hint` optionnel.
    fn make_llm_plan(step_id: &str, model_hint: Option<&str>) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: "plan-llm".into(),
            task_id: "task-llm".into(),
            steps: vec![PlanStep {
                step_id: step_id.into(),
                description: format!("LLM step {step_id}"),
                tool_hint: None,
                depends_on: vec![],
                model_hint: model_hint.map(String::from),
            }],
        }
    }

    /// Mock `CompletionModel` qui tags sa response with the backend name,
    /// so tests can verify which backend handled the request.
    struct TaggedMockModel {
        tag: String,
    }

    #[async_trait::async_trait]
    impl apollia_llm::CompletionModel for TaggedMockModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: format!("response-from-{}", self.tag),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError("mock does not stream".into()))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            &self.tag
        }

        fn model_id(&self) -> &str {
            "tagged-mock"
        }
    }

    /// Step avec model_hint dispatche vers le backend nommé.
    ///
    /// GIVEN un LlmRouter avec backends "default" et "fast"
    ///   ET un PlanStep LLM avec model_hint = Some("fast")
    /// WHEN actor.execute() est appelé
    /// THEN la réponse provient du backend "fast"
    #[tokio::test]
    async fn test_story227_ac1_model_hint_dispatches_to_named_backend() {
        // GIVEN
        let plan = make_llm_plan("s1", Some("fast"));
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mut backends = HashMap::new();
        backends.insert(
            "default".to_string(),
            Arc::new(TaggedMockModel {
                tag: "default".into(),
            }) as Arc<dyn apollia_llm::CompletionModel>,
        );
        backends.insert(
            "fast".to_string(),
            Arc::new(TaggedMockModel { tag: "fast".into() })
                as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "default");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("response-from-fast"),
            "expected output from 'fast' backend, got: {output_debug}"
        );
    }

    /// Step avec model_hint inconnu fallback vers le défaut avec warning.
    ///
    /// GIVEN un LlmRouter avec uniquement un backend "default"
    ///   ET un PlanStep LLM avec model_hint = Some("unknown-backend")
    /// WHEN actor.execute() est appelé
    /// THEN la réponse provient du backend "default" (fallback)
    #[tokio::test]
    async fn test_story227_ac2_unknown_model_hint_falls_back_to_default() {
        // GIVEN
        let plan = make_llm_plan("s1", Some("unknown-backend"));
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mut backends = HashMap::new();
        backends.insert(
            "default".to_string(),
            Arc::new(TaggedMockModel {
                tag: "default".into(),
            }) as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "default");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — fallback vers default, pas d'erreur
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("response-from-default"),
            "expected output from 'default' backend (fallback), got: {output_debug}"
        );
    }

    /// Step sans model_hint utilise le défaut (rétrocompatible).
    ///
    /// GIVEN un LlmRouter avec backends "default" et "fast"
    ///   ET un PlanStep LLM avec model_hint = None
    /// WHEN actor.execute() est appelé
    /// THEN la réponse provient du backend "default"
    #[tokio::test]
    async fn test_story227_ac3_no_model_hint_uses_default() {
        // GIVEN
        let plan = make_llm_plan("s1", None);
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mut backends = HashMap::new();
        backends.insert(
            "default".to_string(),
            Arc::new(TaggedMockModel {
                tag: "default".into(),
            }) as Arc<dyn apollia_llm::CompletionModel>,
        );
        backends.insert(
            "fast".to_string(),
            Arc::new(TaggedMockModel { tag: "fast".into() })
                as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "default");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("response-from-default"),
            "expected output from 'default' backend, got: {output_debug}"
        );
    }

    /// Steps de type outil ignorent model_hint.
    ///
    /// GIVEN un PlanStep avec tool_hint = Some("bash_executor") et model_hint = Some("fast")
    /// WHEN actor.execute() est appelé
    /// THEN l'outil est exécuté normalement (model_hint ignoré)
    #[tokio::test]
    async fn test_story227_ac4_tool_step_ignores_model_hint() {
        // GIVEN — step outil avec model_hint défini
        let plan = ExecutionPlan {
            plan_id: "plan-tool".into(),
            task_id: "task-tool".into(),
            steps: vec![PlanStep {
                step_id: "s1".into(),
                description: "Tool step".into(),
                tool_hint: Some("bash_executor".into()),
                depends_on: vec![],
                model_hint: Some("fast".into()),
            }],
        };
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let proxy = MockToolProxy {
            response: "tool-output".into(),
        };
        // LlmRouter vide — si model_hint était utilisé, ça échouerait
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — l'outil est exécuté normalement, model_hint ignoré
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("tool-output"),
            "expected tool output, got: {output_debug}"
        );
    }

    // ── StepContext per-step observation ─────────────────────────

    // Step with dependency receives the output of the predecessor.
    //
    // GIVEN a plan with s1 and s2 depending on s1
    // WHEN s1 completes with output "result_A"
    // THEN s2 receives a StepContext with previous_outputs = {"s1": "result_A"}
    #[tokio::test]
    async fn test_story229_ac1_step_with_dependency_receives_previous_output() {
        // GIVEN — plan: s1 → s2 (s2 depends on s1).
        // A capturing proxy records the input it receives.
        struct CapturingProxy {
            calls: Mutex<Vec<(String, String)>>,
        }
        #[async_trait::async_trait]
        impl ToolProxyTrait for CapturingProxy {
            async fn invoke(
                &self,
                tool_name: &str,
                input: &serde_json::Value,
            ) -> Result<String, String> {
                let input_str = input.to_string();
                self.calls
                    .lock()
                    .expect("lock")
                    .push((tool_name.to_string(), input_str));
                Ok(format!("result_{tool_name}"))
            }
        }

        let plan = ExecutionPlan {
            plan_id: "plan-229-ac1".into(),
            task_id: "task-229-ac1".into(),
            steps: vec![
                PlanStep {
                    step_id: "s1".into(),
                    description: "Step s1".into(),
                    tool_hint: Some("tool_a".into()),
                    depends_on: vec![],
                    model_hint: None,
                },
                PlanStep {
                    step_id: "s2".into(),
                    description: "Combine {{s1}}".into(),
                    tool_hint: Some("tool_b".into()),
                    depends_on: vec!["s1".into()],
                    model_hint: None,
                },
            ],
        };

        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let proxy = CapturingProxy {
            calls: Mutex::new(vec![]),
        };
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — completed, and s2 received the interpolated output from s1.
        assert_eq!(result.status, TaskStatus::Completed);
        let calls = proxy.calls.lock().expect("lock");
        assert_eq!(calls.len(), 2);
        // s2's input should have {{s1}} replaced with "result_tool_a"
        assert!(
            calls[1].1.contains("result_tool_a"),
            "s2 input should contain s1 output, got: {}",
            calls[1].1
        );
    }

    // Step without dependency receives an empty StepContext.
    //
    // GIVEN a plan with a single step s1 without dependencies
    // WHEN s1 starts
    // THEN s1 receives a StepContext with previous_outputs = {}
    #[tokio::test]
    async fn test_story229_ac2_step_without_dependency_receives_empty_context() {
        // We verify this by checking that a standalone LLM step does NOT receive
        // a system message with "Previous step results:" (since context is empty).
        struct CapturingModel {
            received: Mutex<Vec<Vec<ChatMessage>>>,
        }
        #[async_trait::async_trait]
        impl apollia_llm::CompletionModel for CapturingModel {
            async fn complete(
                &self,
                req: CompletionRequest,
            ) -> Result<apollia_llm::CompletionResponse, apollia_llm::LlmError> {
                self.received
                    .lock()
                    .expect("lock")
                    .push(req.messages.clone());
                Ok(apollia_llm::CompletionResponse {
                    content: "llm output".into(),
                    tool_calls: vec![],
                    usage: TokenUsage {
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        cost_usd: None,
                        ..Default::default()
                    },
                    finish_reason: FinishReason::Stop,
                    latency_ms: 0,
                    ttft_ms: None,
                })
            }
            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<
                Pin<
                    Box<
                        dyn futures::Stream<
                                Item = Result<apollia_llm::StreamChunk, apollia_llm::LlmError>,
                            > + Send,
                    >,
                >,
                apollia_llm::LlmError,
            > {
                Err(apollia_llm::LlmError::InferenceError(
                    "mock does not stream".into(),
                ))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "capturing-mock"
            }
            fn model_id(&self) -> &str {
                "capturing-model"
            }
        }

        let model = Arc::new(CapturingModel {
            received: Mutex::new(vec![]),
        });
        let model_for_check = model.clone();

        // Single LLM step (tool_hint = None → LLM path).
        let plan = ExecutionPlan {
            plan_id: "plan-229-ac2".into(),
            task_id: "task-229-ac2".into(),
            steps: vec![PlanStep {
                step_id: "s1".into(),
                description: "Summarize the document".into(),
                tool_hint: None,
                depends_on: vec![],
                model_hint: None,
            }],
        };

        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let mut backends = HashMap::new();
        backends.insert(
            "capturing-mock".to_string(),
            model as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "capturing-mock");
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — completed, and the LLM received only a user message (no system context).
        assert_eq!(result.status, TaskStatus::Completed);
        let received = model_for_check.received.lock().expect("lock");
        assert_eq!(received.len(), 1, "expected 1 LLM call");
        let messages = &received[0];
        // No system message should be present since previous_outputs is empty.
        assert_eq!(
            messages.len(),
            1,
            "expected only 1 message (user), got: {messages:?}"
        );
    }

    // Budget view reflects consumed steps.
    //
    // GIVEN a budget with max_steps = 10 and 3 steps already consumed
    // WHEN StepContext is built for the 4th step
    // THEN remaining_budget reflects 7 steps remaining
    #[test]
    fn test_story229_ac3_budget_view_reflects_consumed_steps() {
        // GIVEN
        let budget = StepBudget::with_max(10);
        budget.increment_steps();
        budget.increment_steps();
        budget.increment_steps();

        // WHEN
        let view = budget.to_budget_view();

        // THEN — the view should NOT be exhausted (7 steps remain).
        assert!(
            !view.is_exhausted(),
            "budget should not be exhausted with 7 steps remaining"
        );
    }

    // ── Per-step episodic memory ─────────────────────────────────

    /// Helper: creates a manifest with `memory_namespace` set.
    fn make_manifest_with_memory(namespace: &str) -> AgentManifest {
        let json = format!(
            r#"{{"name":"test-agent","version":"0.1.0","description":"test","tools_required":[],"memory_namespace":"{namespace}"}}"#,
        );
        serde_json::from_str(&json).expect("manifest with memory_namespace must deserialize")
    }

    /// Helper: creates an ActorLoop with a real MemoryManager backed by a temp dir.
    fn make_actor_with_memory(
        plan: ExecutionPlan,
        manifest: AgentManifest,
        memory_dir: &std::path::Path,
    ) -> (ActorLoop, tokio::sync::broadcast::Receiver<RuntimeEvent>) {
        let (bus_tx, bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, &manifest.name).expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mm = apollia_memory::manager::MemoryManager::new(
            memory_dir,
            manifest.memory_namespace.clone(),
            vec![],
        );
        let mm = Arc::new(Mutex::new(mm));

        let actor = ActorLoop::new(plan, 2, db, bus_tx, manifest).with_memory_manager(Some(mm));
        (actor, bus_rx)
    }

    // Episodic entry created after step completion.
    //
    // GIVEN an agent with memory_namespace configured and a plan of 3 steps
    // WHEN all steps complete successfully
    // THEN an episodic entry exists for each step in the agent's namespace
    #[tokio::test]
    async fn test_story230_ac1_episodic_entry_after_step() {
        // GIVEN
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = make_manifest_with_memory("test-ns");
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"]), ("s3", &["s2"])]);
        let (mut actor, _rx) = make_actor_with_memory(plan, manifest, tmp.path());
        let proxy = MockToolProxy {
            response: "analysis done".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — plan completed
        assert_eq!(result.status, TaskStatus::Completed);

        // Wait briefly for spawn_blocking tasks to complete.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify episodic entries exist via MemoryManager.
        let mm = actor.memory_manager.as_ref().expect("memory_manager set");
        let mut guard = mm.lock().expect("lock");
        let store = guard.store("test-ns").expect("open store");
        let episodic = apollia_memory::episodic::EpisodicMemory::new(store);
        let entries = episodic.history("test-ns", 100, None).expect("history");

        assert_eq!(
            entries.len(),
            3,
            "expected 3 episodic entries, got {}",
            entries.len()
        );
        // Verify content contains step id and output.
        assert!(
            entries.iter().any(|e| e.content.contains("step s1:")),
            "expected entry for s1"
        );
        assert!(
            entries.iter().any(|e| e.content.contains("analysis done")),
            "expected output in entry"
        );
    }

    // No write when memory_namespace is None.
    //
    // GIVEN an agent without memory_namespace
    // WHEN steps complete
    // THEN no memory write is attempted (no crash, no error)
    #[tokio::test]
    async fn test_story230_ac2_no_write_without_namespace() {
        // GIVEN — default manifest has no memory_namespace
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"])]);
        let (mut actor, _rx) = make_actor(plan);
        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN — execute completes without memory_manager, no crash.
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
        assert!(
            actor.memory_manager.is_none(),
            "memory_manager should be None for default actor"
        );
    }

    // Memory write failure does not block execution.
    //
    // GIVEN a memory_manager pointing to an invalid/read-only path
    // WHEN steps complete and the memory write fails
    // THEN the plan still completes successfully (warning logged)
    #[tokio::test]
    async fn test_story230_ac3_memory_failure_does_not_block() {
        // GIVEN — use a non-existent directory that will cause SQLite to fail
        let manifest = make_manifest_with_memory("test-ns");
        let bad_path = std::path::PathBuf::from("/nonexistent/path/that/cannot/exist");
        let mm = apollia_memory::manager::MemoryManager::new(
            &bad_path,
            Some("test-ns".to_string()),
            vec![],
        );
        let mm = Arc::new(Mutex::new(mm));

        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"])]);
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest).with_memory_manager(Some(mm));

        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN — memory write will fail but execution should continue
        let result = actor
            .execute(&proxy, &llm, &budget, &resilience, &reasoner)
            .await;

        // THEN — plan completed despite memory failure
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "plan should complete even if memory write fails"
        );
    }
}
