//! ORIAEngine — execution engine for agent tasks.
//!
//! Entry point for running agent tasks. Supports two modes:
//! - **Mode Direct** — single `agent.run()` call with `StepBudget` supervision.
//! - **Mode Orchestrated** — `Reasoner` generates a plan, `ActorLoop` executes
//!   each step via `ToolProxy`, and outputs are concatenated or forwarded to
//!   `on_plan_complete()`.
//!
//! The primary entry point is [`ORIAEngine::execute`], which classifies the task
//! and delegates to the appropriate mode. The lower-level [`ORIAEngine::execute_direct`]
//! remains available for callers that already hold an [`AgentRunner`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

use apollia_core::{
    AIPPart, AIPResult, AIPTask, AgentManifest, DataPart, EventBusSender, PendingApprovals,
    RuntimeEvent, StepBudgetConfig, TaskStatus,
};
use apollia_llm::{CompletionModel, LlmRouter};
use apollia_memory::manager::MemoryManager;

use crate::actor::{ActorLoop, ToolProxyTrait};
use crate::budget::StepBudget;
use crate::observer::{classify, ContextBundle, ExecutionMode, ObserverError};
use crate::plan::ExecutionPlan;
use crate::plan_cache::{compute_cache_key, PlanCacheRepository};
use crate::plan_repository::PlanRepository;
use crate::reasoner::{Reasoner, ReasonerError};
use crate::resilience::ResilienceLayer;

// ─────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────

/// Interval for polling budget exhaustion during `execute_direct`.
const BUDGET_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Maximum number of replanning attempts in orchestrated execution.
const MAX_REPLANS: u32 = 2;

// ─────────────────────────────────────────────
// Traits
// ─────────────────────────────────────────────

/// Trait abstracting agent execution for testability.
///
/// Concrete implementation dispatches to `AIPBridge::call_run()`.
/// Tests use a mock runner.
pub trait AgentRunner: Send + Sync {
    /// Executes the agent's `run(task)` method and returns the result.
    fn call_run(
        &self,
        task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>>;
}

/// Trait abstracting an agent that can be executed by `ORIAEngine::execute`.
///
/// Provides the `manifest()` for mode classification and tool resolution.
/// Optionally declares `on_plan_complete()` availability for orchestrated post-processing
/// (duck-typing detection, ADR-022 / ADR-003).
///
/// Minimum contract: implement `manifest()` only. All other methods have defaults.
pub trait AIPAgent: Send + Sync {
    /// Returns the agent's manifest declaring capabilities and execution mode.
    fn manifest(&self) -> AgentManifest;

    /// Returns `true` if the agent exposes an `on_plan_complete()` method.
    ///
    /// Detected via `hasattr` Python. Returns `false` by default —
    /// the automatic step-output concatenation is used as fallback.
    fn has_on_plan_complete(&self) -> bool {
        false
    }

    /// Calls `on_plan_complete(step_results)` on the agent.
    ///
    /// Invoked by `ORIAEngine::execute_orchestrated_plan()` when [`has_on_plan_complete`]
    /// returns `true`. The concrete Python implementation delegates to
    /// `AIPBridge::call_on_plan_complete()`.
    ///
    /// Default: concatenates step outputs automatically (same as the fallback path).
    ///
    /// [`has_on_plan_complete`]: AIPAgent::has_on_plan_complete
    fn call_on_plan_complete(
        &self,
        step_results: HashMap<String, String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + '_>> {
        Box::pin(async move { concat_outputs(&step_results) })
    }
}

// ─────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────

/// Erreurs de l'engine ORIA.
#[derive(Debug, thiserror::Error)]
pub enum ORIAError {
    /// Budget d'execution epuise.
    #[error("step budget exceeded: {reason}")]
    BudgetExceeded {
        /// Description lisible de la raison d'epuisement.
        reason: String,
    },

    /// Echec de l'execution de l'agent.
    #[error("agent execution failed: {0}")]
    ExecutionFailed(String),

    /// Erreur de l'Observer.
    #[error("observer error: {0}")]
    ObserverError(#[from] ObserverError),

    /// Erreur du bridge AIP.
    #[error("bridge error: {0}")]
    BridgeError(String),

    /// Aucun LLM configuré — impossible d'exécuter le mode Orchestrated.
    #[error("no LLM configured for orchestrated execution")]
    NoLlmConfigured,

    /// Erreur du Reasoner lors de la planification.
    #[error("planning failed: {0}")]
    PlanFailed(#[from] ReasonerError),

    /// Le oneshot channel d'approbation a été fermé avant réponse — runtime shutdown.
    #[error("approval channel closed before human response — runtime may be shutting down")]
    ApprovalChannelClosed,
}

// ─────────────────────────────────────────────
// NoopToolProxy — fallback when no proxy configured
// ─────────────────────────────────────────────

/// Tool proxy that always returns an error — used when no tool proxy is configured.
struct NoopToolProxy;

#[async_trait::async_trait]
impl ToolProxyTrait for NoopToolProxy {
    async fn invoke(&self, tool_name: &str, _input: &serde_json::Value) -> Result<String, String> {
        Err(format!(
            "No tool proxy configured — cannot invoke '{tool_name}'"
        ))
    }
}

// ─────────────────────────────────────────────
// ORIAEngine
// ─────────────────────────────────────────────

/// Moteur d'execution ORIA (Observer-Reasoner-Actor).
///
/// Point d'entrée unifié pour l'exécution des tâches agents.
/// Supporte deux modes :
/// - **Mode Direct** — `execute_direct()` avec supervision `StepBudget`.
/// - **Mode Orchestrated** — `execute()` → planification LLM + `ActorLoop`.
///
/// Utilise le pattern builder pour l'injection des dépendances (ADR-016).
/// Toutes les dépendances sont optionnelles — un engine sans LLM ne supporte
/// que le Mode Direct.
pub struct ORIAEngine {
    reasoner: Option<Reasoner>,
    tool_proxy: Option<Arc<dyn ToolProxyTrait>>,
    llm_router: LlmRouter,
    resilience: ResilienceLayer,
    event_bus: EventBusSender,
    runtime_config: StepBudgetConfig,
    db_path: Option<String>,
    /// Registre HITL des approbations en attente — partagé avec le `ResumeHandler`.
    ///
    /// Requis pour que `execute_direct()` suspende la tâche et attende la décision
    /// humaine. Si `None`, les résultats `InputRequired` sont retournés
    /// tels quels sans suspension.
    pending_approvals: Option<Arc<PendingApprovals>>,
    /// Repository SQLite HITL — persiste le prompt et le contexte lors de la suspension.
    ///
    /// Si `None`, la persistance est ignorée (warning tracé) mais l'exécution continue.
    task_repository: Option<Arc<apollia_tools::TaskRepository>>,
    /// Memory manager for automatic episodic recording per step.
    ///
    /// Passed to [`ActorLoop`] during orchestrated execution. When `Some`, each completed
    /// step records an episodic memory entry in the agent's namespace.
    memory_manager: Option<Arc<Mutex<MemoryManager>>>,
    /// Cache de plans d'exécution.
    ///
    /// Wrappé dans un `Mutex` car `rusqlite::Connection` n'est pas `Sync`.
    /// Les accès sont courts (lookup/store) et non concurrents en pratique.
    /// Un cache hit évite l'appel LLM et émet [`RuntimeEvent::PlanCacheHit`].
    /// Les erreurs de cache sont loguées en `warn` et n'empêchent jamais l'exécution.
    plan_cache: Option<Mutex<PlanCacheRepository>>,
}

impl ORIAEngine {
    /// Crée un `ORIAEngine` avec les valeurs par défaut (Mode Direct uniquement).
    ///
    /// Pour activer le Mode Orchestrated, chaîner avec [`with_reasoner`].
    /// Pour activer le HITL, chaîner avec [`with_pending_approvals`] et [`with_task_repository`].
    ///
    /// [`with_reasoner`]: ORIAEngine::with_reasoner
    /// [`with_pending_approvals`]: ORIAEngine::with_pending_approvals
    /// [`with_task_repository`]: ORIAEngine::with_task_repository
    pub fn new() -> Self {
        let (event_bus, _) = tokio::sync::broadcast::channel(64);
        Self {
            reasoner: None,
            tool_proxy: None,
            llm_router: LlmRouter::empty(),
            resilience: ResilienceLayer::new(3, Duration::from_secs(30)),
            event_bus,
            runtime_config: StepBudgetConfig::default(),
            db_path: None,
            pending_approvals: None,
            task_repository: None,
            memory_manager: None,
            plan_cache: None,
        }
    }

    /// Configure le `ORIAEngine` avec un LLM pour activer le Mode Orchestrated.
    ///
    /// `max_steps` borne la taille des plans générés par le Reasoner
    /// (principe #7 — Garde-fous non-négociables).
    pub fn with_reasoner(mut self, model: Arc<dyn CompletionModel>, max_steps: u32) -> Self {
        self.reasoner = Some(Reasoner::new(model, max_steps));
        self
    }

    /// Configure le `ToolProxy` pour l'exécution des steps orchestrés.
    ///
    /// Sans `ToolProxy`, les steps avec `tool_hint` échouent avec `NoopToolProxy`.
    pub fn with_tool_proxy(mut self, proxy: Arc<dyn ToolProxyTrait>) -> Self {
        self.tool_proxy = Some(proxy);
        self
    }

    /// Configure le `LlmRouter` pour la synthèse LLM dans les steps orchestrés.
    pub fn with_llm_router(mut self, router: LlmRouter) -> Self {
        self.llm_router = router;
        self
    }

    /// Injecte un `EventBusSender` pour diffuser les événements du plan sur le bus.
    pub fn with_event_bus(mut self, bus: EventBusSender) -> Self {
        self.event_bus = bus;
        self
    }

    /// Configure le budget runtime global (plafond appliqué via `StepBudget::from_capped`).
    pub fn with_runtime_config(mut self, config: StepBudgetConfig) -> Self {
        self.runtime_config = config;
        self
    }

    /// Configure le chemin SQLite pour la persistance des plans d'exécution.
    ///
    /// Si absent, un fallback `:memory:` est utilisé (pas de persistance entre redémarrages).
    pub fn with_db_path(mut self, path: impl Into<String>) -> Self {
        self.db_path = Some(path.into());
        self
    }

    /// Injecte le registre HITL des approbations en attente.
    ///
    /// Requis pour que `execute_direct()` suspende la tâche en status `input_required`
    /// et attende la décision humaine via un oneshot channel.
    /// Partagé entre le `ORIAEngine` et les routes REST via `AppState`.
    pub fn with_pending_approvals(mut self, pending: Arc<PendingApprovals>) -> Self {
        self.pending_approvals = Some(pending);
        self
    }

    /// Injecte le repository SQLite HITL pour persister le prompt et le contexte.
    ///
    /// Si absent, la persistance SQLite est ignorée mais l'exécution HITL continue
    /// (warning tracé — Principe #4 : fail fast uniquement pour les erreurs détectables).
    pub fn with_task_repository(mut self, repo: Arc<apollia_tools::TaskRepository>) -> Self {
        self.task_repository = Some(repo);
        self
    }

    /// Injecte un [`MemoryManager`] pour l'enregistrement épisodique per-step.
    ///
    /// Passé à l'[`ActorLoop`] lors de l'exécution orchestrée. Chaque step complété
    /// enregistre automatiquement une entrée épisodique dans le namespace de l'agent.
    pub fn with_memory_manager(mut self, mm: Arc<Mutex<MemoryManager>>) -> Self {
        self.memory_manager = Some(mm);
        self
    }

    /// Ajoute un cache de plans à l'engine.
    ///
    /// Quand configuré, [`execute_orchestrated_plan`] vérifie le cache avant d'appeler
    /// le Reasoner. Un cache hit évite l'appel LLM, clone le plan avec un nouveau
    /// `plan_id`, et émet [`RuntimeEvent::PlanCacheHit`] sur l'EventBus.
    /// Les erreurs de cache sont loguées en `warn` sans bloquer l'exécution.
    ///
    /// [`execute_orchestrated_plan`]: ORIAEngine::execute_orchestrated_plan
    pub fn with_plan_cache(mut self, repo: PlanCacheRepository) -> Self {
        self.plan_cache = Some(Mutex::new(repo));
        self
    }

    // ─── Point d'entrée unifié ────────────────────────────────────────────

    /// Point d'entrée principal — route la tâche vers le mode Direct ou Orchestrated.
    ///
    /// Le mode est déterminé par [`classify`] selon `manifest.execution_mode`
    /// (override explicite) ou les heuristiques de complexité.
    ///
    /// ## Mode Direct
    /// Non implémenté via ce point d'entrée — utiliser [`execute_direct`] directement
    /// avec un [`AgentRunner`] concret.
    ///
    /// ## Mode Orchestrated
    /// Délègue à [`execute_orchestrated_plan`] :
    /// validate → plan → persist → ActorLoop → concat outputs.
    pub async fn execute(&self, task: AIPTask, agent: &dyn AIPAgent) -> AIPResult {
        let manifest = agent.manifest();
        let mode = classify(&task, &manifest, None);

        match mode {
            ExecutionMode::Direct => {
                // Direct mode via AIPAgent not yet implemented here.
                // Callers should use execute_direct() with an AgentRunner.
                // Will be connected in a follow-up story.
                AIPResult::failed(
                    "DIRECT_MODE_NOT_AVAILABLE_VIA_AIP_AGENT",
                    "Direct mode requires an AgentRunner — use execute_direct() directly",
                )
            }
            ExecutionMode::Orchestrated => {
                self.execute_orchestrated_plan(task, agent, manifest).await
            }
        }
    }

    // ─── Mode Orchestrated ────────────────────────────────────────────────

    /// Exécution orchestrée complète : plan → persist → ActorLoop → concat.
    ///
    /// Implémente le pipeline ADR-022 Option B :
    /// 1. Valide `system_prompt` présent (fail fast — Principe #4)
    /// 2. Génère le plan via `Reasoner` (retry ×3 interne)
    /// 3. Persiste plan + steps dans SQLite (non-bloquant sur erreur)
    /// 4. Émet `RuntimeEvent::PlanGenerated`
    /// 5. Crée `StepBudget::from_capped(manifest, runtime)`
    /// 6. Exécute via `ActorLoop`
    /// 7. Concatène les outputs (ou stub `on_plan_complete`)
    async fn execute_orchestrated_plan(
        &self,
        task: AIPTask,
        agent: &dyn AIPAgent,
        manifest: AgentManifest,
    ) -> AIPResult {
        // ── AC-2 : validate system_prompt ────────────────────────────────
        if manifest.system_prompt.is_none() {
            return AIPResult::failed(
                "MISSING_SYSTEM_PROMPT",
                "execution_mode=orchestrated requires system_prompt in the agent manifest",
            );
        }

        // ── Build ContextBundle ───────────────────────────────────────────
        let available_tools: Vec<String> = manifest
            .tools_required
            .iter()
            .chain(manifest.tools_optional.iter())
            .cloned()
            .collect();

        let ctx = ContextBundle {
            task: task.clone(),
            memory_snapshot: None,
            execution_mode: ExecutionMode::Orchestrated,
            available_tools,
            manifest_system_prompt: manifest.system_prompt.clone(),
            llm_backend_names: vec![],
        };

        // ── AC-3 : get reasoner or fail ───────────────────────────────────
        let reasoner = match self.reasoner.as_ref() {
            Some(r) => r,
            None => {
                return AIPResult::failed(
                    "NO_LLM",
                    "Orchestrated mode requires a configured LLM (use with_reasoner())",
                )
            }
        };

        // ── Plan cache lookup ─────────────────────────────────
        let task_text = extract_task_text(&task);
        let cache_key = compute_cache_key(
            &manifest.name,
            &manifest.version,
            &ctx.available_tools,
            &task_text,
        );

        if let Some(ref cache_mutex) = self.plan_cache {
            let lookup_result = match cache_mutex.lock() {
                Ok(cache) => Some(cache.lookup(&cache_key)),
                Err(e) => {
                    tracing::warn!(error = %e, "plan cache mutex poisoned, skipping lookup");
                    None
                }
            };
            match lookup_result {
                Some(Ok(Some(cached_plan))) => {
                    let new_plan_id = uuid::Uuid::new_v4().to_string();
                    let plan = ExecutionPlan {
                        plan_id: new_plan_id,
                        task_id: task.task_id.clone(),
                        steps: cached_plan.steps,
                    };

                    let _ = self.event_bus.send(RuntimeEvent::PlanCacheHit {
                        task_id: task.task_id.clone().into(),
                        cache_key: cache_key.clone(),
                    });

                    return self.execute_cached_plan(plan, task, agent, manifest).await;
                }
                Some(Ok(None)) | None => { /* cache miss or lock error — proceed to Reasoner */ }
                Some(Err(e)) => {
                    tracing::warn!(error = %e, "plan cache lookup failed");
                }
            }
        }

        // ── Generate plan (Reasoner handles retries internally) ───────────
        let plan = match reasoner.plan(&ctx).await {
            Ok(p) => p,
            Err(e) => return AIPResult::failed("PLAN_FAILED", &e.to_string()),
        };

        // ── Store in cache ────────────────────────────────────
        if let Some(ref cache_mutex) = self.plan_cache {
            match cache_mutex.lock() {
                Ok(cache) => {
                    if let Err(e) =
                        cache.store(&cache_key, &plan, &manifest.name, &manifest.version)
                    {
                        tracing::warn!(error = %e, "plan cache store failed");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "plan cache mutex poisoned, skipping store");
                }
            }
        }

        let plan_id = plan.plan_id.clone();
        let step_count = plan.steps.len();
        let task_id_str = task.task_id.clone();

        // ── Persist plan in SQLite (non-blocking on error) ────────────────
        let db_path = self.db_path.as_deref().unwrap_or(":memory:");
        let repo = self.open_repo_with_plan(db_path, &plan, &manifest.name);

        // ── AC-5 : emit PlanGenerated ─────────────────────────────────────
        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan_id.clone(),
            step_count,
        });

        // ── AC-4 : create StepBudget via from_capped ─────────────────────
        let agent_budget = manifest.step_budget.clone().unwrap_or_default();
        let budget = StepBudget::from_capped(&agent_budget, &self.runtime_config);

        // ── Execute via ActorLoop ─────────────────────────────────────────
        let noop_proxy = NoopToolProxy;
        let tool_proxy: &dyn ToolProxyTrait = match &self.tool_proxy {
            Some(p) => p.as_ref(),
            None => &noop_proxy,
        };

        let plan_start = Instant::now();
        let mut actor = ActorLoop::new(
            plan,
            MAX_REPLANS,
            repo,
            self.event_bus.clone(),
            manifest.clone(),
        )
        .with_pending_approvals(self.pending_approvals.clone())
        .with_memory_manager(self.memory_manager.clone());
        let step_result = actor
            .execute(
                tool_proxy,
                &self.llm_router,
                &budget,
                &self.resilience,
                reasoner,
            )
            .await;
        let duration_ms = plan_start.elapsed().as_millis() as u64;

        // ── Post-process ──────────────────────────────────────────────────
        if step_result.status == TaskStatus::Completed {
            let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                task_id: task_id_str.into(),
                plan_id,
                step_count,
                duration_ms,
            });

            let outputs = extract_step_outputs(&step_result);

            // call on_plan_complete() if the agent exposes it,
            // otherwise fall back to automatic step-output concatenation (AC-6).
            if agent.has_on_plan_complete() {
                agent.call_on_plan_complete(outputs).await
            } else {
                concat_outputs(&outputs)
            }
        } else {
            step_result
        }
    }

    /// Exécute un plan récupéré depuis le cache.
    ///
    /// Identique au chemin post-Reasoner de [`execute_orchestrated_plan`] :
    /// persist → emit PlanGenerated → StepBudget → ActorLoop → concat.
    async fn execute_cached_plan(
        &self,
        plan: ExecutionPlan,
        task: AIPTask,
        agent: &dyn AIPAgent,
        manifest: AgentManifest,
    ) -> AIPResult {
        let plan_id = plan.plan_id.clone();
        let step_count = plan.steps.len();
        let task_id_str = task.task_id.clone();

        let db_path = self.db_path.as_deref().unwrap_or(":memory:");
        let repo = self.open_repo_with_plan(db_path, &plan, &manifest.name);

        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan_id.clone(),
            step_count,
        });

        let agent_budget = manifest.step_budget.clone().unwrap_or_default();
        let budget = StepBudget::from_capped(&agent_budget, &self.runtime_config);

        let noop_proxy = NoopToolProxy;
        let tool_proxy: &dyn ToolProxyTrait = match &self.tool_proxy {
            Some(p) => p.as_ref(),
            None => &noop_proxy,
        };

        let plan_start = Instant::now();
        let reasoner = match self.reasoner.as_ref() {
            Some(r) => r,
            None => {
                return AIPResult::failed(
                    "NO_LLM",
                    "Orchestrated mode requires a configured LLM (use with_reasoner())",
                );
            }
        };
        let mut actor = ActorLoop::new(
            plan,
            MAX_REPLANS,
            repo,
            self.event_bus.clone(),
            manifest.clone(),
        )
        .with_pending_approvals(self.pending_approvals.clone())
        .with_memory_manager(self.memory_manager.clone());
        let step_result = actor
            .execute(
                tool_proxy,
                &self.llm_router,
                &budget,
                &self.resilience,
                reasoner,
            )
            .await;
        let duration_ms = plan_start.elapsed().as_millis() as u64;

        if step_result.status == TaskStatus::Completed {
            let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                task_id: task_id_str.into(),
                plan_id,
                step_count,
                duration_ms,
            });

            let outputs = extract_step_outputs(&step_result);

            if agent.has_on_plan_complete() {
                agent.call_on_plan_complete(outputs).await
            } else {
                concat_outputs(&outputs)
            }
        } else {
            step_result
        }
    }

    /// Opens a `PlanRepository` at `db_path`, inserts the plan and its steps.
    ///
    /// Falls back to `:memory:` if `db_path` fails. Errors during `insert_plan`
    /// or `insert_steps` are logged but do not abort execution (Principle #4 —
    /// persistance non-bloquante).
    fn open_repo_with_plan(
        &self,
        db_path: &str,
        plan: &ExecutionPlan,
        agent_name: &str,
    ) -> PlanRepository {
        let repo = match PlanRepository::new(db_path) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "Failed to open PlanRepository — falling back to :memory:");
                PlanRepository::new(":memory:").expect("in-memory SQLite must always succeed")
            }
        };

        if let Err(e) = repo.insert_plan(plan, agent_name) {
            tracing::error!(error = %e, "Failed to persist plan (non-blocking)");
        }
        if let Err(e) = repo.insert_steps(&plan.plan_id, &plan.steps) {
            tracing::error!(error = %e, "Failed to persist plan steps (non-blocking)");
        }

        repo
    }

    // ─── Mode Direct ──────────────────────────────────────────────────────

    /// Exécute une tâche en Mode Direct avec support HITL.
    ///
    /// 1. Vérifie que le budget n'est pas déjà épuisé.
    /// 2. Appelle `runner.call_run(task)` avec supervision `StepBudget`.
    /// 3. Si `AIPResult::InputRequired` :
    ///    - Persiste prompt + context dans SQLite via `task_repository` (si configuré).
    ///    - Émet `RuntimeEvent::TaskInputRequired` sur l'EventBus.
    ///    - Enregistre un oneshot dans `pending_approvals` et **attend** la décision humaine.
    ///    - Si `approved=true` : reconstruit `AIPTask` avec `is_resumed=true` et rappelle `run()`.
    ///    - Si `approved=false` : retourne `AIPResult::failed("REJECTED", reason)`.
    /// 4. Sinon retourne le résultat directement.
    ///
    /// **AC-4 (StepBudget pausé pendant suspension)** : l'attente sur le oneshot est un
    /// `await` pur — le polling du budget ne tourne pas pendant la suspension.
    /// Le budget ne progresse pas tant que l'humain n'a pas répondu.
    pub async fn execute_direct(
        &self,
        task: AIPTask,
        runner: &dyn AgentRunner,
        budget: Arc<StepBudget>,
    ) -> Result<AIPResult, ORIAError> {
        // Check budget before starting
        if budget.is_exhausted() {
            let reason = budget
                .exhaustion_reason()
                .unwrap_or_else(|| "budget already exhausted".into());
            return Err(ORIAError::BudgetExceeded { reason });
        }

        // First run — with budget supervision
        let result = Self::run_with_budget(runner, task.clone(), &budget).await?;

        // Non-HITL path — return immediately
        if result.status != TaskStatus::InputRequired {
            return Ok(result);
        }

        // ── HITL Suspension ───────────────────────────────────────────────
        let (prompt, context) = match result.input_required_data {
            Some(data) => (data.prompt, data.context),
            None => ("Approbation requise".to_string(), serde_json::Value::Null),
        };

        // AC-1 : persist input_required in SQLite (non-blocking on error — Principle #4)
        if let Some(repo) = self.task_repository.as_ref() {
            if let Err(e) = repo
                .save_input_required(&task.task_id, None, &prompt, &context)
                .await
            {
                tracing::warn!(
                    task_id = %task.task_id,
                    error = %e,
                    "failed to persist input_required — continuing without DB record"
                );
            }

            // record suspended_at timestamp for HITL timing
            let suspended_at =
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            if let Err(e) = repo
                .save_suspended_at(&task.task_id, None, &suspended_at)
                .await
            {
                tracing::warn!(
                    task_id = %task.task_id,
                    error = %e,
                    "failed to persist suspended_at — continuing without timing record"
                );
            }
        }

        // AC-1 : broadcast TaskInputRequired on EventBus
        // step_id=None in Mode Direct — the whole task is suspended (not a specific step).
        let _ = self.event_bus.send(RuntimeEvent::TaskInputRequired {
            task_id: task.task_id.clone().into(),
            prompt: prompt.clone(),
            step_id: None,
        });

        tracing::info!(
            task_id = %task.task_id,
            %prompt,
            "task suspended — waiting for human approval"
        );

        // AC-5 : register on PendingApprovals — if not configured, degrade gracefully
        let pending = match self.pending_approvals.as_ref() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    task_id = %task.task_id,
                    "PendingApprovals not configured — returning InputRequired without suspension"
                );
                return Ok(AIPResult::input_required(&prompt, context));
            }
        };

        let rx = pending.register(&task.task_id);

        // AC-4 : plain await — StepBudget does NOT advance during suspension
        let response = rx.await.map_err(|_| ORIAError::ApprovalChannelClosed)?;

        tracing::info!(
            task_id = %task.task_id,
            approved = response.approved,
            "human approval received — resuming task"
        );

        // AC-3 : rejection → AIPResult::failed without calling run()
        if !response.approved {
            return Ok(AIPResult::failed(
                "REJECTED",
                response.reason.as_deref().unwrap_or("Refusé"),
            ));
        }

        // AC-2 : approval → rebuild AIPTask with is_resumed=true and call run() again
        let resumed_task = AIPTask {
            is_resumed: true,
            input_response: Some(response),
            ..task
        };

        // Run resumed task with budget protection
        Self::run_with_budget(runner, resumed_task, &budget).await
    }

    /// Exécute `runner.call_run(task)` avec supervision concurrente du `StepBudget`.
    ///
    /// Retourne immédiatement avec `ORIAError::BudgetExceeded` si le budget expire
    /// avant la fin de l'exécution. Utilisé pour le premier appel et pour la reprise
    /// après HITL.
    async fn run_with_budget(
        runner: &dyn AgentRunner,
        task: AIPTask,
        budget: &Arc<StepBudget>,
    ) -> Result<AIPResult, ORIAError> {
        tokio::select! {
            result = runner.call_run(task) => {
                result.map_err(ORIAError::BridgeError)
            }
            _ = Self::poll_budget_exhaustion(budget) => {
                let reason = budget
                    .exhaustion_reason()
                    .unwrap_or_else(|| "budget exhausted during execution".into());
                Err(ORIAError::BudgetExceeded { reason })
            }
        }
    }

    /// Polls `budget.is_exhausted()` at regular intervals until exhausted.
    async fn poll_budget_exhaustion(budget: &StepBudget) {
        let mut interval = tokio::time::interval(BUDGET_POLL_INTERVAL);
        loop {
            interval.tick().await;
            if budget.is_exhausted() {
                return;
            }
        }
    }
}

impl Default for ORIAEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────

/// Extracts the step outputs map from an `AIPResult::completed_with_steps` result.
///
/// Extrait le texte d'une tâche à partir de ses `input.parts`.
///
/// Concatène tous les `TextPart` séparés par un espace. Retourne une chaîne vide
/// si aucune partie textuelle n'est présente.
fn extract_task_text(task: &AIPTask) -> String {
    task.input
        .parts
        .iter()
        .filter_map(|p| {
            if let AIPPart::Text(t) = p {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `AIPResult::completed_with_steps` stores the `HashMap<step_id, output>` as
/// `AIPPart::Data`. Returns an empty map if the data cannot be parsed.
fn extract_step_outputs(result: &AIPResult) -> HashMap<String, String> {
    if let Some(AIPPart::Data(DataPart { data })) = result.output.first() {
        if let Ok(map) = serde_json::from_value::<HashMap<String, String>>(data.clone()) {
            return map;
        }
    }
    HashMap::new()
}

/// Concatène les outputs des steps en un `AIPResult::Completed`.
///
/// Les steps sont triés par `step_id` pour un résultat déterministe.
/// Séparateur : deux sauts de ligne (`\n\n`), aligné sur le format Markdown.
fn concat_outputs(outputs: &HashMap<String, String>) -> AIPResult {
    let mut sorted: Vec<(&String, &String)> = outputs.iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    let text = sorted
        .iter()
        .map(|(_, v)| v.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    AIPResult::completed(&text)
}

// ─────────────────────────────────────────────
// Tests — execute_direct
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPResult, PendingApprovals, StepBudgetConfig, TaskStatus};

    struct MockRunnerOk {
        result: AIPResult,
    }

    impl AgentRunner for MockRunnerOk {
        fn call_run(
            &self,
            _task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>,
        > {
            let result = self.result.clone();
            Box::pin(async move { Ok(result) })
        }
    }

    struct MockRunnerErr {
        message: String,
    }

    impl AgentRunner for MockRunnerErr {
        fn call_run(
            &self,
            _task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>,
        > {
            let msg = self.message.clone();
            Box::pin(async move { Err(msg) })
        }
    }

    fn make_task() -> AIPTask {
        AIPTask::default()
    }

    fn make_result() -> AIPResult {
        AIPResult {
            task_id: "task-001".into(),
            status: TaskStatus::Completed,
            output: vec![],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        }
    }

    #[tokio::test]
    async fn test_execute_direct_budget_already_exhausted() {
        // GIVEN un budget deja epuise (max_steps=0)
        let config = StepBudgetConfig {
            max_steps: 0,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let engine = ORIAEngine::new();
        let runner = MockRunnerOk {
            result: make_result(),
        };

        // WHEN on appelle execute_direct()
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN retourne ORIAError::BudgetExceeded
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ORIAError::BudgetExceeded { .. }),
            "expected BudgetExceeded, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_execute_direct_success() {
        // GIVEN un budget valide et un runner mock qui retourne Ok(AIPResult)
        let config = StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let engine = ORIAEngine::new();
        let runner = MockRunnerOk {
            result: make_result(),
        };

        // WHEN on appelle execute_direct()
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN retourne Ok(AIPResult) avec le resultat attendu
        assert!(result.is_ok());
        let aip_result = result.expect("should be ok");
        assert_eq!(aip_result.task_id, "task-001");
        assert!(matches!(aip_result.status, TaskStatus::Completed));
    }

    #[tokio::test]
    async fn test_execute_direct_bridge_error() {
        // GIVEN un budget valide et un runner mock qui retourne Err
        let config = StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let engine = ORIAEngine::new();
        let runner = MockRunnerErr {
            message: "Python exception: crash".into(),
        };

        // WHEN on appelle execute_direct()
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN retourne ORIAError::BridgeError
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ORIAError::BridgeError(_)),
            "expected BridgeError, got: {err}"
        );
    }

    // ── HITL tests ───────────────────────────────────────────

    /// Runner qui retourne InputRequired au premier appel, puis Completed au second.
    struct MockRunnerInputRequired {
        call_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    impl AgentRunner for MockRunnerInputRequired {
        fn call_run(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>,
        > {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move {
                if count == 0 {
                    // First call — return InputRequired
                    Ok(AIPResult::input_required(
                        "Confirmer l'envoi ?",
                        serde_json::json!({"devis": 42}),
                    ))
                } else {
                    // Second call (resumed) — verify is_resumed and return Completed
                    assert!(task.is_resumed, "task should be resumed on second call");
                    assert!(
                        task.input_response.is_some(),
                        "input_response should be set on resume"
                    );
                    Ok(AIPResult {
                        task_id: task.task_id,
                        status: TaskStatus::Completed,
                        output: vec![],
                        error: None,
                        artifacts: vec![],
                        input_required_data: None,
                    })
                }
            })
        }
    }

    fn make_budget() -> Arc<StepBudget> {
        Arc::new(StepBudget::new(&StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        }))
    }

    // InputRequired → TaskInputRequired émis sur EventBus + suspension enregistrée

    /// ÉTANT DONNÉ un agent qui retourne InputRequired
    /// QUAND execute_direct() reçoit ce résultat
    /// ALORS RuntimeEvent::TaskInputRequired est émis sur l'EventBus
    #[tokio::test]
    async fn test_ac1_input_required_emits_event() {
        // GIVEN
        let (tx, mut rx) = tokio::sync::broadcast::channel::<apollia_core::RuntimeEvent>(16);
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new()
            .with_event_bus(tx)
            .with_pending_approvals(pending.clone());
        let runner = MockRunnerInputRequired {
            call_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };
        let task = AIPTask {
            task_id: "t-0001".into(),
            ..AIPTask::default()
        };

        // WHEN — spawn execute_direct in background so we can resolve from this task
        let engine_ref = &engine;
        let runner_ref = &runner;
        let budget = make_budget();
        let task_clone = task.clone();
        let pending_clone = pending.clone();

        let handle = tokio::spawn(async move {
            // Resolve from another task after a short yield
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            pending_clone
                .resolve(
                    "t-0001",
                    apollia_core::InputResponseData {
                        approved: true,
                        reason: None,
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve failed");
        });

        let result = engine_ref
            .execute_direct(task_clone, runner_ref, budget)
            .await;

        handle.await.expect("background task failed");

        // THEN result is Ok (Completed from second run)
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().status, TaskStatus::Completed);

        // THEN TaskInputRequired was emitted
        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, apollia_core::RuntimeEvent::TaskInputRequired { .. }) {
                found = true;
                break;
            }
        }
        assert!(found, "expected TaskInputRequired event on EventBus");
    }

    // Approve → run() rappelé avec is_resumed=true

    /// ÉTANT DONNÉ une tâche suspendue en input_required
    /// QUAND PendingApprovals.resolve(approved=true)
    /// ALORS execute_direct() se débloque et rappelle run() avec is_resumed=true
    #[tokio::test]
    async fn test_ac2_approve_resumes_and_recalls_run() {
        // GIVEN
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let runner = MockRunnerInputRequired {
            call_count: call_count.clone(),
        };
        let task = AIPTask {
            task_id: "t-0002".into(),
            ..AIPTask::default()
        };

        // Spawn resolver
        let pending_for_resolver = pending.clone();
        let resolver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            pending_for_resolver
                .resolve(
                    "t-0002",
                    apollia_core::InputResponseData {
                        approved: true,
                        reason: None,
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve failed");
        });

        // WHEN
        let result = engine.execute_direct(task, &runner, make_budget()).await;
        resolver.await.expect("resolver task failed");

        // THEN result is Completed (run() called twice)
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, TaskStatus::Completed);
        // run() was called twice: first → InputRequired, second → Completed
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    // Reject → AIPResult::failed("REJECTED") sans rappeler run()

    /// ÉTANT DONNÉ une tâche suspendue en input_required
    /// QUAND PendingApprovals.resolve(approved=false, reason="Trop cher")
    /// ALORS execute_direct() retourne AIPResult::failed("REJECTED") sans rappeler run()
    #[tokio::test]
    async fn test_reject_returns_failed_without_run() {
        // GIVEN
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let runner = MockRunnerInputRequired {
            call_count: call_count.clone(),
        };
        let task = AIPTask {
            task_id: "t-0003".into(),
            ..AIPTask::default()
        };

        // Spawn resolver with rejection
        let pending_for_resolver = pending.clone();
        let resolver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            pending_for_resolver
                .resolve(
                    "t-0003",
                    apollia_core::InputResponseData {
                        approved: false,
                        reason: Some("Trop cher".into()),
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve failed");
        });

        // WHEN
        let result = engine.execute_direct(task, &runner, make_budget()).await;
        resolver.await.expect("resolver task failed");

        // THEN result is Failed with code REJECTED
        assert!(result.is_ok());
        let aip_result = result.unwrap();
        assert_eq!(aip_result.status, TaskStatus::Failed);
        let code = aip_result
            .error
            .as_ref()
            .map(|e| e.code.as_str())
            .unwrap_or("");
        assert_eq!(code, "REJECTED", "expected REJECTED error code");
        let msg = aip_result
            .error
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or("");
        assert!(
            msg.contains("Trop cher"),
            "reason should appear in message: {msg}"
        );
        // run() was called exactly once (first call only)
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

// ─────────────────────────────────────────────
// Tests — execute() Mode Orchestrated
// ─────────────────────────────────────────────

#[cfg(test)]
mod orchestrated_tests {
    use super::*;
    use apollia_core::{AgentManifest, StepBudgetConfig, TaskStatus};
    use apollia_llm::{
        CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage,
    };
    use std::pin::Pin;

    // ── Mock LLM model ──────────────────────────────────────────────────

    struct SimpleMockModel {
        response: String,
    }

    #[async_trait::async_trait]
    impl CompletionModel for SimpleMockModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: self.response.clone(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError(
                "mock does not support streaming".into(),
            ))
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

    // ── Mock LLM that always returns an error ───────────────────────────

    struct ErrorMockModel;

    #[async_trait::async_trait]
    impl CompletionModel for ErrorMockModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::InferenceError("planned LLM failure".into()))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError(
                "mock does not support streaming".into(),
            ))
        }

        fn is_available(&self) -> bool {
            false
        }

        fn backend_name(&self) -> &str {
            "error-mock"
        }

        fn model_id(&self) -> &str {
            "error-mock"
        }
    }

    // ── Mock ToolProxy ──────────────────────────────────────────────────

    struct MockToolProxy {
        output: String,
    }

    #[async_trait::async_trait]
    impl ToolProxyTrait for MockToolProxy {
        async fn invoke(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(self.output.clone())
        }
    }

    // ── Mock AIPAgent (no hook) ─────────────────────────────────────────

    struct MockAgent {
        manifest: AgentManifest,
    }

    impl AIPAgent for MockAgent {
        fn manifest(&self) -> AgentManifest {
            self.manifest.clone()
        }
    }

    // ── Mock AIPAgent with on_plan_complete hook ─────────────────────────

    struct MockAgentWithHook {
        manifest: AgentManifest,
    }

    impl AIPAgent for MockAgentWithHook {
        fn manifest(&self) -> AgentManifest {
            self.manifest.clone()
        }

        fn has_on_plan_complete(&self) -> bool {
            true
        }

        fn call_on_plan_complete(
            &self,
            _step_results: HashMap<String, String>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + '_>> {
            Box::pin(async move { AIPResult::completed("HOOK_CALLED") })
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Valid 2-step plan JSON returned by the mock LLM.
    fn two_step_plan_json() -> String {
        r#"{"steps":[
            {"step_id":"s1","description":"step one","tool_hint":"file_io","depends_on":[]},
            {"step_id":"s2","description":"step two","tool_hint":"bash_executor","depends_on":[]}
        ]}"#
        .to_string()
    }

    /// Valid 4-step plan JSON (for AC-5 step_count verification).
    fn four_step_plan_json() -> String {
        r#"{"steps":[
            {"step_id":"s1","description":"step 1","tool_hint":"file_io","depends_on":[]},
            {"step_id":"s2","description":"step 2","tool_hint":"file_io","depends_on":[]},
            {"step_id":"s3","description":"step 3","tool_hint":"bash_executor","depends_on":[]},
            {"step_id":"s4","description":"step 4","tool_hint":"bash_executor","depends_on":[]}
        ]}"#
        .to_string()
    }

    fn make_engine_with_mock(plan_json: String) -> ORIAEngine {
        let model = Arc::new(SimpleMockModel {
            response: plan_json,
        });
        let proxy = Arc::new(MockToolProxy {
            output: "mock output".into(),
        });
        ORIAEngine::new()
            .with_reasoner(model, 20)
            .with_tool_proxy(proxy)
    }

    fn orchestrated_manifest_with_prompt() -> AgentManifest {
        AgentManifest {
            name: "test-agent".into(),
            version: "1.0.0".into(),
            description: "Test orchestrated agent".into(),
            tools_required: vec!["file_io".into(), "bash_executor".into()],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: Some(StepBudgetConfig {
                max_steps: 20,
                max_tool_calls: 50,
                wall_clock_secs: 600,
            }),
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "orchestrated".into(),
            system_prompt: Some("Planifie les étapes nécessaires.".into()),
            tools_requiring_approval: vec![],
        }
    }

    // ── AC-1 : agent sans hook → concaténation automatique des outputs ──

    /// ÉTANT DONNÉ un agent execution_mode=orchestrated sans on_plan_complete()
    ///      ET un mock LLM retournant un plan de 2 steps
    ///      ET un mock ToolProxy retournant un output pour chaque step
    /// QUAND ORIAEngine::execute(task, &agent) est appelé
    /// ALORS AIPResult::Completed est retourné
    ///   ET RuntimeEvent::PlanCompleted a été émis
    #[tokio::test]
    async fn test_sans_hook_concatenation() {
        // GIVEN
        let engine = make_engine_with_mock(two_step_plan_json());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {:?}",
            result.error
        );
    }

    // ── AC-5 : hook on_plan_complete() appelé si présent ────────────────

    /// ÉTANT DONNÉ un agent avec on_plan_complete() qui retourne "HOOK_CALLED"
    ///      ET execute_orchestrated() qui retourne CompletedWithSteps
    /// QUAND ORIAEngine::execute(task, &agent) est appelé
    /// ALORS le résultat contient "HOOK_CALLED" (pas la concaténation auto)
    #[tokio::test]
    async fn test_hook_called_when_present() {
        // GIVEN
        let engine = make_engine_with_mock(two_step_plan_json());
        let agent = MockAgentWithHook {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN status Completed AND output contains "HOOK_CALLED"
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {:?}",
            result.error
        );
        let output_text = result.output.iter().find_map(|p| {
            if let apollia_core::AIPPart::Text(t) = p {
                Some(t.text.clone())
            } else {
                None
            }
        });
        assert_eq!(
            output_text.as_deref(),
            Some("HOOK_CALLED"),
            "expected hook output 'HOOK_CALLED', got: {output_text:?}"
        );
    }

    // ── AC-6 : concaténation utilisée si hook absent ─────────────────────

    /// ÉTANT DONNÉ un agent SANS on_plan_complete()
    ///      ET execute_orchestrated() qui retourne CompletedWithSteps
    /// QUAND ORIAEngine::execute(task, &agent) est appelé
    /// ALORS call_on_plan_complete() n'est PAS appelé
    ///   ET la concaténation automatique des outputs est retournée
    #[tokio::test]
    async fn test_concat_used_when_no_hook() {
        // GIVEN
        let engine = make_engine_with_mock(two_step_plan_json());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN status Completed AND output does NOT contain "HOOK_CALLED"
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {:?}",
            result.error
        );
        let has_hook_called = result.output.iter().any(|p| {
            if let apollia_core::AIPPart::Text(t) = p {
                t.text.contains("HOOK_CALLED")
            } else {
                false
            }
        });
        assert!(
            !has_hook_called,
            "hook output should NOT appear when agent has no on_plan_complete"
        );
    }

    // ── AC-2 : system_prompt absent → AIPResult::failed immédiat ────────

    /// ÉTANT DONNÉ un agent execution_mode=orchestrated SANS system_prompt
    /// QUAND ORIAEngine::execute(task, &agent) est appelé
    /// ALORS AIPResult::failed("MISSING_SYSTEM_PROMPT", _) est retourné
    ///   ET Reasoner.plan() n'est PAS appelé (aucun LLM configuré)
    #[tokio::test]
    async fn test_system_prompt_absent_retourne_failed() {
        // GIVEN — no LLM and no system_prompt (both should be caught at system_prompt check)
        let engine = ORIAEngine::new();
        let agent = MockAgent {
            manifest: AgentManifest {
                name: "no-prompt-agent".into(),
                version: "1.0.0".into(),
                description: "Agent without system_prompt".into(),
                tools_required: vec![],
                tools_optional: vec![],
                supports_streaming: false,
                supports_a2a: false,
                memory_namespace: None,
                shared_memory_namespaces: vec![],
                max_concurrent_tasks: 1,
                step_budget: None,
                network_allowlist: None,
                dangerous_tools_allowed: false,
                tags: vec![],
                skills: vec![],
                execution_mode: "orchestrated".into(),
                system_prompt: None, // ← absent
                tools_requiring_approval: vec![],
            },
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err_code = result.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
        assert_eq!(
            err_code, "MISSING_SYSTEM_PROMPT",
            "expected MISSING_SYSTEM_PROMPT, got: {err_code}"
        );
    }

    // ── AC-3 : Reasoner échoue → AIPResult::failed propagé ──────────────

    /// ÉTANT DONNÉ un mock LLM qui retourne toujours une erreur
    /// QUAND ORIAEngine::execute(task, &agent) est appelé
    /// ALORS AIPResult::failed("PLAN_FAILED", _) est retourné
    #[tokio::test]
    async fn test_reasoner_echec_retourne_failed() {
        // GIVEN
        let model = Arc::new(ErrorMockModel);
        let engine = ORIAEngine::new().with_reasoner(model, 20);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err_code = result.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
        assert_eq!(
            err_code, "PLAN_FAILED",
            "expected PLAN_FAILED, got: {err_code}"
        );
    }

    // ── AC-5 : PlanGenerated avec step_count correct ─────────────────────

    /// ÉTANT DONNÉ un plan de 4 steps généré par le Reasoner
    ///      ET un EventBus subscriber actif
    /// QUAND ORIAEngine::execute_orchestrated() est appelé
    /// ALORS le subscriber reçoit RuntimeEvent::PlanGenerated { step_count: 4, .. }
    #[tokio::test]
    async fn test_plan_generated_event_step_count() {
        // GIVEN
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(32);
        let engine = make_engine_with_mock(four_step_plan_json()).with_event_bus(tx);

        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let _ = engine.execute(task, &agent).await;

        // THEN — collect all events and look for PlanGenerated
        let mut found_step_count = None;
        while let Ok(event) = rx.try_recv() {
            if let RuntimeEvent::PlanGenerated { step_count, .. } = event {
                found_step_count = Some(step_count);
                break;
            }
        }

        assert_eq!(
            found_step_count,
            Some(4),
            "expected PlanGenerated with step_count=4"
        );
    }

    // ── Helper tests ────────────────────────────────────────────────────

    /// ÉTANT DONNÉ un AIPResult::completed_with_steps avec 2 outputs
    /// QUAND extract_step_outputs est appelé
    /// ALORS les 2 outputs sont correctement extraits
    #[test]
    fn test_extract_step_outputs_parses_data_part() {
        // GIVEN
        let mut steps = HashMap::new();
        steps.insert("s1".to_string(), "output one".to_string());
        steps.insert("s2".to_string(), "output two".to_string());
        let result = AIPResult::completed_with_steps(steps);

        // WHEN
        let outputs = extract_step_outputs(&result);

        // THEN
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs.get("s1").map(String::as_str), Some("output one"));
        assert_eq!(outputs.get("s2").map(String::as_str), Some("output two"));
    }

    /// ÉTANT DONNÉ une map vide
    /// QUAND concat_outputs est appelé
    /// ALORS AIPResult::Completed avec output vide est retourné
    #[test]
    fn test_concat_outputs_empty_map_returns_completed() {
        // GIVEN
        let outputs = HashMap::new();

        // WHEN
        let result = concat_outputs(&outputs);

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
    }

    // ─── Plan Cache Integration ──────────────────────────────

    /// ÉTANT DONNÉ deux versions différentes du même agent
    /// QUAND compute_cache_key est appelé avec "1.0" puis "1.1"
    /// ALORS les clés de cache sont différentes
    #[test]
    fn test_version_change_produces_different_key() {
        // GIVEN
        let tools = vec!["bash".to_string(), "file_io".to_string()];
        let text = "analyze logs";

        // WHEN
        let key_v1 = compute_cache_key("analyzer", "1.0", &tools, text);
        let key_v2 = compute_cache_key("analyzer", "1.1", &tools, text);

        // THEN
        assert_ne!(key_v1, key_v2);
        assert_eq!(key_v1.len(), 64, "SHA-256 hex should be 64 chars");
        assert_eq!(key_v2.len(), 64, "SHA-256 hex should be 64 chars");
    }

    /// ÉTANT DONNÉ une tâche avec des parties textuelles
    /// QUAND extract_task_text est appelé
    /// ALORS les textes sont concaténés avec un espace
    #[test]
    fn test_extract_task_text_concatenates_text_parts() {
        // GIVEN
        let task = AIPTask {
            input: apollia_core::AIPInput {
                parts: vec![
                    AIPPart::Text(apollia_core::TextPart {
                        text: "analyze".into(),
                    }),
                    AIPPart::Data(DataPart {
                        data: serde_json::json!({"key": "val"}),
                    }),
                    AIPPart::Text(apollia_core::TextPart {
                        text: "logs".into(),
                    }),
                ],
            },
            ..AIPTask::default()
        };

        // WHEN
        let text = extract_task_text(&task);

        // THEN
        assert_eq!(text, "analyze logs");
    }

    /// ÉTANT DONNÉ une tâche sans partie textuelle
    /// QUAND extract_task_text est appelé
    /// ALORS une chaîne vide est retournée
    #[test]
    fn test_extract_task_text_empty_when_no_text_parts() {
        // GIVEN
        let task = AIPTask::default();

        // WHEN
        let text = extract_task_text(&task);

        // THEN
        assert!(text.is_empty());
    }

    /// ÉTANT DONNÉ un PlanCacheHit event
    /// QUAND il est émis sur l'EventBus
    /// ALORS il est reçu avec les bons champs
    #[test]
    fn test_cache_hit_event_emits_on_bus() {
        // GIVEN
        let (tx, mut rx) = tokio::sync::broadcast::channel::<apollia_core::RuntimeEvent>(16);

        // WHEN
        let _ = tx.send(RuntimeEvent::PlanCacheHit {
            task_id: "task-42".into(),
            cache_key: "abc123".into(),
        });

        // THEN
        let event = rx.try_recv().expect("should receive event");
        match event {
            RuntimeEvent::PlanCacheHit { task_id, cache_key } => {
                assert_eq!(task_id.as_ref(), "task-42");
                assert_eq!(cache_key, "abc123");
            }
            other => panic!("expected PlanCacheHit, got: {other:?}"),
        }
    }
}
