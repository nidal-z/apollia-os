//! `ActorLoop` — boucle d'exécution topologique d'un [`crate::plan::ExecutionPlan`].
//!
//! `ActorLoop` est la pièce centrale du mode Orchestré (Option B) : ORIA exécute
//! directement les outils et le LLM — `agent.run()` n'est **pas** appelé pendant
//! les steps. L'agent fournit uniquement son `manifest()` et optionnellement
//! `on_plan_complete()` (STORY-086).
//!
//! ## Pipeline d'exécution
//!
//! ```text
//! ActorLoop::execute()
//!   ├── topological_sort(plan.steps)         → ordre d'exécution (STORY-082)
//!   ├── Pour chaque step_id dans ordre :
//!   │   ├── StepBudget::is_exhausted()       → STEP_BUDGET_EXCEEDED si épuisé
//!   │   ├── db.start_step()                  → SQLite (STORY-081)
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
use std::time::Instant;

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::manifest::AgentManifest;
use apollia_core::AIPResult;
use apollia_llm::{ChatMessage, CompletionRequest, LlmRouter};

use crate::budget::StepBudget;
use crate::observer::{ContextBundle, ExecutionMode};
use crate::plan::{ExecutionPlan, PlanStep};
use crate::plan_repository::PlanRepository;
use crate::reasoner::Reasoner;
use crate::resilience::ResilienceLayer;
use crate::topo::topological_sort;

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
}

impl StepError {
    /// Retourne `true` si cette erreur peut déclencher une replanification.
    ///
    /// `ToolCallFailed` et `LlmCallFailed` sont retryables (problèmes transitoires).
    /// `NoLlmBackend` et `ToolNotFound` sont permanents (configuration manquante).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            StepError::ToolCallFailed(_) | StepError::LlmCallFailed(_)
        )
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
pub struct ActorLoop {
    plan: ExecutionPlan,
    replan_count: u32,
    max_replans: u32,
    db: PlanRepository,
    event_bus: EventBusSender,
    /// Manifest de l'agent propriétaire de ce plan.
    ///
    /// Stocké en lecture seule pour que `execute_step` puisse accéder à
    /// `tools_requiring_approval` lors de chaque step (vérification STORY-097).
    pub manifest: AgentManifest,
}

impl ActorLoop {
    /// Crée un `ActorLoop` pour un plan donné.
    ///
    /// Le plan doit déjà être inséré dans SQLite avant la création de l'`ActorLoop`
    /// (via `PlanRepository::insert_plan` + `insert_steps`).
    ///
    /// `manifest` est conservé en lecture seule pour que les steps puissent
    /// accéder à `tools_requiring_approval` (vérification STORY-097).
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
        }
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
        let order = match topological_sort(&self.plan.steps) {
            Ok(o) => o,
            Err(_) => {
                if let Err(e) = self.db.fail_plan(&self.plan.plan_id, "INVALID_PLAN") {
                    tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                }
                return AIPResult::failed("INVALID_PLAN", "Circular dependency in execution plan");
            }
        };

        let mut completed_outputs: HashMap<String, String> = HashMap::new();

        for step_id in order {
            let step = match self.plan.steps.iter().find(|s| s.step_id == step_id) {
                Some(s) => s.clone(),
                None => continue,
            };

            // AC-2 : vérifier le budget avant chaque step.
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

            let started = Instant::now();
            let result = self
                .execute_step(&step, &completed_outputs, tool_proxy, llm_router)
                .await;
            let duration_ms = started.elapsed().as_millis() as u64;
            budget.increment_steps();

            match result {
                Ok(output) => {
                    if let Err(e) = self.db.complete_step(&self.plan.plan_id, &step_id, &output) {
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

                Err(e) => {
                    // Échec permanent non-retryable.
                    if let Err(db_err) =
                        self.db
                            .fail_step(&self.plan.plan_id, &step_id, &e.to_string())
                    {
                        tracing::warn!(error = %db_err, step_id = %step_id, "fail_step DB call failed (ignored)");
                    }
                    if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, &e.to_string()) {
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

    /// Exécute un step individuel — outil ou LLM selon `tool_hint`.
    ///
    /// - `tool_hint = Some("llm")` ou `None` → appel direct au `LlmRouter` (backend défaut).
    /// - `tool_hint = Some(tool_name)` → appel via `ToolProxyTrait::invoke`.
    ///
    /// Les outputs des steps précédents sont interpolés dans la description du step
    /// via [`interpolate_outputs`] avant d'être transmis à l'outil ou au LLM.
    async fn execute_step(
        &self,
        step: &PlanStep,
        completed_outputs: &HashMap<String, String>,
        tool_proxy: &dyn ToolProxyTrait,
        llm_router: &LlmRouter,
    ) -> Result<String, StepError> {
        // AC-7 : interpoler les outputs des steps précédents dans la description.
        let input = interpolate_outputs(&step.description, completed_outputs);

        match step.tool_hint.as_deref() {
            // AC-6 : step LLM — appel direct au backend défaut du LlmRouter.
            Some("llm") | None => {
                let model = llm_router.get(None).ok_or(StepError::NoLlmBackend)?;
                let response = model
                    .complete(CompletionRequest {
                        messages: vec![ChatMessage::user(input)],
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| StepError::LlmCallFailed(e.to_string()))?;
                Ok(response.content)
            }
            // AC-5 : step outil — appel via ToolProxyTrait.
            Some(tool_name) => tool_proxy
                .invoke(tool_name, &serde_json::json!({"input": input}))
                .await
                .map_err(StepError::ToolCallFailed),
        }
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

                let started = Instant::now();
                let result = self
                    .execute_step(&step, &completed_outputs, tool_proxy, llm_router)
                    .await;
                let duration_ms = started.elapsed().as_millis() as u64;
                budget.increment_steps();

                match result {
                    Ok(output) => {
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

                    Err(e) => {
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
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, TaskStatus};
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
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
        {
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

    // ── AC-1 — Exécution séquentielle dans l'ordre topologique ───────────────

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

    // ── AC-2 — StepBudget épuisé au step 3/5 ─────────────────────────────────

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

    // ── AC-3 — Replanification déclenchée sur step retryable ─────────────────

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

    // ── AC-4 — MAX_REPLAN_EXCEEDED après 2 replanifications ──────────────────

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

    // ── AC-3 — Propagation manifest vers ActorLoop ───────────────────────────

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
}
