//! ORIAEngine — execution engine for agent tasks.
//!
//! Entry point for running agent tasks. Currently supports Mode Direct
//! (single `agent.run()` call with StepBudget supervision).
//!
//! The engine uses `tokio::select!` to race the agent execution against
//! periodic budget checks, ensuring wall-clock timeouts are enforced.

use std::sync::Arc;
use std::time::Duration;

use apollia_core::{AIPResult, AIPTask};
use apollia_llm::CompletionModel;

use crate::budget::StepBudget;
use crate::observer::{ContextBundle, ObserverError};
use crate::reasoner::{ExecutionPlan, Reasoner, ReasonerError};

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
}

/// Interval for polling budget exhaustion during `execute_direct`.
const BUDGET_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Moteur d'execution ORIA (Observer-Reasoner-Actor).
///
/// Point d'entree principal pour l'execution des taches.
/// Gere le Mode Direct avec supervision StepBudget et le Mode Orchestrated via le Reasoner.
///
/// Le `Reasoner` est optionnel : sans LLM configuré, seul le Mode Direct est disponible.
/// Utiliser [`ORIAEngine::with_reasoner`] pour activer le Mode Orchestrated.
pub struct ORIAEngine {
    reasoner: Option<Reasoner>,
}

impl ORIAEngine {
    /// Crée un `ORIAEngine` sans LLM (Mode Direct uniquement).
    pub fn new() -> Self {
        Self { reasoner: None }
    }

    /// Configure le `ORIAEngine` avec un LLM pour activer le Mode Orchestrated.
    ///
    /// Utilise le pattern builder pour l'injection de dépendance (ADR-016).
    pub fn with_reasoner(mut self, model: Arc<dyn CompletionModel>) -> Self {
        self.reasoner = Some(Reasoner::new(model));
        self
    }

    /// Exécute le mode Orchestrated : appelle le Reasoner pour produire un [`ExecutionPlan`].
    ///
    /// Retourne [`ORIAError::NoLlmConfigured`] si aucun LLM n'a été injecté via
    /// [`with_reasoner`].
    ///
    /// Note : l'exécution réelle des [`PlanStep`] par des sous-agents est prévue Sprint 9+.
    /// Le paramètre `budget` est accepté pour conformité API et sera enforced lors de l'exécution.
    pub async fn execute_orchestrated(
        &self,
        bundle: &ContextBundle,
        _budget: &StepBudget,
    ) -> Result<ExecutionPlan, ORIAError> {
        let reasoner = self.reasoner.as_ref().ok_or(ORIAError::NoLlmConfigured)?;
        reasoner.plan(bundle).await.map_err(ORIAError::from)
    }

    /// Execute une tache en Mode Direct.
    ///
    /// 1. Verifie que le budget n'est pas deja epuise
    /// 2. Appelle `runner.call_run(task)`
    /// 3. Retourne `AIPResult` ou `ORIAError`
    ///
    /// Le StepBudget est supervise en parallele via `tokio::select!` :
    /// - branche 1 : `runner.call_run()` termine normalement
    /// - branche 2 : polling periodique de `budget.is_exhausted()` (100ms interval)
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

        let run_future = runner.call_run(task);

        tokio::select! {
            result = run_future => {
                result.map_err(ORIAError::BridgeError)
            }
            _ = Self::poll_budget_exhaustion(&budget) => {
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
// Tests — execute_orchestrated (AC-5)
// ─────────────────────────────────────────────

#[cfg(test)]
mod orchestrated_tests {
    use super::*;
    use apollia_core::{AIPInput, AIPPart, AIPTask, StepBudgetConfig, TextPart};
    use apollia_llm::{CompletionRequest, CompletionResponse, FinishReason, LlmError, TokenUsage};
    use std::pin::Pin;

    use crate::observer::{ContextBundle, ExecutionMode};

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
        ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
        {
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

    fn make_orchestrated_bundle() -> ContextBundle {
        ContextBundle {
            task: AIPTask {
                task_id: "task-001".into(),
                context_id: "ctx-001".into(),
                input: AIPInput {
                    parts: vec![AIPPart::Text(TextPart {
                        text: "Generate a multi-step plan".into(),
                    })],
                },
                history: vec![],
                timeout_seconds: None,
            },
            memory_snapshot: None,
            execution_mode: ExecutionMode::Orchestrated,
            available_tools: vec!["file_io".into(), "bash_executor".into()],
        }
    }

    fn make_budget() -> Arc<StepBudget> {
        let config = StepBudgetConfig {
            max_steps: 20,
            max_tool_calls: 50,
            wall_clock_secs: 600,
        };
        Arc::new(StepBudget::new(&config))
    }

    /// ÉTANT DONNÉ un `ORIAEngine` configuré avec un mock `CompletionModel`
    ///      ET un `ContextBundle` avec `ExecutionMode::Orchestrated`
    /// QUAND on appelle `engine.execute_orchestrated(&bundle, &budget).await`
    /// ALORS `Reasoner::plan()` est appelé 1 fois et le résultat est un `ExecutionPlan`
    #[tokio::test]
    async fn test_ac5_execute_orchestrated_returns_plan() {
        // GIVEN
        let valid_plan = r#"{"goal":"multi-step plan","steps":[{"id":"s1","description":"read config","tool":"file_io","depends_on":[]},{"id":"s2","description":"run script","tool":"bash_executor","depends_on":["s1"]}]}"#;
        let model = Arc::new(SimpleMockModel {
            response: valid_plan.into(),
        });
        let engine = ORIAEngine::new().with_reasoner(model);
        let bundle = make_orchestrated_bundle();
        let budget = make_budget();

        // WHEN
        let result = engine.execute_orchestrated(&bundle, &budget).await;

        // THEN
        let plan = result.expect("expected Ok(ExecutionPlan)");
        assert_eq!(plan.goal, "multi-step plan");
        assert_eq!(plan.steps.len(), 2);
    }

    /// ÉTANT DONNÉ un `ORIAEngine` sans LLM configuré
    /// QUAND on appelle `execute_orchestrated()`
    /// ALORS `Err(ORIAError::NoLlmConfigured)` est retourné
    #[tokio::test]
    async fn test_execute_orchestrated_no_llm_returns_error() {
        // GIVEN
        let engine = ORIAEngine::new(); // no reasoner
        let bundle = make_orchestrated_bundle();
        let budget = make_budget();

        // WHEN
        let result = engine.execute_orchestrated(&bundle, &budget).await;

        // THEN
        assert!(
            matches!(result, Err(ORIAError::NoLlmConfigured)),
            "expected NoLlmConfigured, got: {:?}",
            result
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPResult, StepBudgetConfig, TaskStatus};

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
}
