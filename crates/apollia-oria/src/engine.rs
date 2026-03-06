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

use crate::budget::StepBudget;
use crate::observer::ObserverError;

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
}

/// Interval for polling budget exhaustion during `execute_direct`.
const BUDGET_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Moteur d'execution ORIA (Observer-Reasoner-Actor).
///
/// Point d'entree principal pour l'execution des taches.
/// Gere le Mode Direct avec supervision StepBudget.
pub struct ORIAEngine;

impl ORIAEngine {
    /// Creates a new ORIAEngine.
    pub fn new() -> Self {
        Self
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
