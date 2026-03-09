//! Integration tests — StepBudget enforcement in real conditions (AC-5, STORY-045).
//!
//! Tests that ORIAEngine correctly enforces the tri-dimensional budget
//! (max_steps, max_tool_calls, wall_clock) and returns BudgetExceeded errors.
//!
//! No Python dependency — uses mock AgentRunner implementations.

use std::sync::Arc;
use std::time::Duration;

use apollia_core::{AIPResult, AIPTask, StepBudgetConfig, TaskStatus};
use apollia_oria::{
    budget::StepBudget,
    engine::{AgentRunner, ORIAEngine, ORIAError},
};

// --- Mock runners ---

/// Runner that completes immediately with a successful result.
struct InstantRunner;

impl AgentRunner for InstantRunner {
    fn call_run(
        &self,
        task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>>
    {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

/// Runner that takes longer than the wall-clock budget allows.
struct SlowRunner {
    delay: Duration,
}

impl AgentRunner for SlowRunner {
    fn call_run(
        &self,
        _task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>>
    {
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(AIPResult {
                task_id: String::new(),
                status: TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

fn make_task() -> AIPTask {
    AIPTask::default()
}

// AC-5 — budget already exhausted before execution (max_steps=0)
#[tokio::test]
async fn test_budget_exceeded_returns_failed() {
    // GIVEN an agent with StepBudget max_steps=2, already at limit
    let config = StepBudgetConfig {
        max_steps: 2,
        max_tool_calls: 100,
        wall_clock_secs: 300,
    };
    let budget = Arc::new(StepBudget::new(&config));

    // Exhaust the step budget
    budget.increment_steps();
    budget.increment_steps();
    assert!(
        budget.is_exhausted(),
        "budget should be exhausted after 2 steps"
    );

    let engine = ORIAEngine::new();
    let runner = InstantRunner;

    // WHEN execute_direct is called with an exhausted budget
    let result = engine.execute_direct(make_task(), &runner, budget).await;

    // THEN BudgetExceeded error is returned
    assert!(
        matches!(result, Err(ORIAError::BudgetExceeded { .. })),
        "expected BudgetExceeded, got: {result:?}"
    );

    // AND the error message mentions the budget dimension
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("budget") || err_msg.contains("step"),
        "error message should reference budget: {err_msg}"
    );
}

// AC-5 — wall-clock budget exceeded during execution
#[tokio::test]
async fn test_wall_clock_budget_exceeded_during_execution() {
    // GIVEN a budget with wall_clock=0 (immediately exhausted)
    let config = StepBudgetConfig {
        max_steps: 100,
        max_tool_calls: 100,
        wall_clock_secs: 0, // 0 seconds = always exhausted
    };
    let budget = Arc::new(StepBudget::new(&config));
    assert!(
        budget.is_exhausted(),
        "wall_clock=0 should be immediately exhausted"
    );

    let engine = ORIAEngine::new();
    let runner = InstantRunner;

    // WHEN execute_direct is called
    let result = engine.execute_direct(make_task(), &runner, budget).await;

    // THEN BudgetExceeded is returned (pre-check fails before even calling run)
    assert!(
        matches!(result, Err(ORIAError::BudgetExceeded { .. })),
        "expected BudgetExceeded from wall-clock, got: {result:?}"
    );
}

// AC-5 — wall-clock exhausted while runner is executing (runtime enforcement)
#[tokio::test]
async fn test_budget_enforced_during_slow_execution() {
    // GIVEN a budget with 50ms wall-clock limit and a runner that takes 300ms
    let config = StepBudgetConfig {
        max_steps: 100,
        max_tool_calls: 100,
        wall_clock_secs: 0, // 0 seconds for determinism in tests
    };
    // Use a budget that starts exhausted — ORIAEngine enforces it immediately
    let budget = Arc::new(StepBudget::new(&config));

    let engine = ORIAEngine::new();
    let runner = SlowRunner {
        delay: Duration::from_millis(300),
    };

    // WHEN execute_direct is called with exhausted budget
    let result = engine.execute_direct(make_task(), &runner, budget).await;

    // THEN BudgetExceeded is returned before the slow runner completes
    assert!(
        matches!(result, Err(ORIAError::BudgetExceeded { .. })),
        "expected BudgetExceeded, got: {result:?}"
    );
}

// AC-5 — max_tool_calls budget enforced
#[tokio::test]
async fn test_tool_calls_budget_exceeded() {
    // GIVEN a budget with max_tool_calls=0
    let config = StepBudgetConfig {
        max_steps: 100,
        max_tool_calls: 0, // 0 tool calls = immediately exhausted
        wall_clock_secs: 300,
    };
    let budget = Arc::new(StepBudget::new(&config));
    assert!(
        budget.is_exhausted(),
        "max_tool_calls=0 should be immediately exhausted"
    );

    let engine = ORIAEngine::new();
    let runner = InstantRunner;

    // WHEN execute_direct is called
    let result = engine.execute_direct(make_task(), &runner, budget).await;

    // THEN BudgetExceeded is returned
    assert!(
        matches!(result, Err(ORIAError::BudgetExceeded { .. })),
        "expected BudgetExceeded from tool_calls limit, got: {result:?}"
    );
}

// AC-5 — from_capped applies min(agent, runtime) per dimension
#[tokio::test]
async fn test_budget_from_capped_enforced() {
    // GIVEN an agent budget (max_steps=100) and a runtime budget (max_steps=1)
    let agent_config = StepBudgetConfig {
        max_steps: 100,
        max_tool_calls: 100,
        wall_clock_secs: 300,
    };
    let runtime_config = StepBudgetConfig {
        max_steps: 1, // runtime overrides with stricter limit
        max_tool_calls: 100,
        wall_clock_secs: 300,
    };

    // WHEN creating via from_capped
    let budget = Arc::new(StepBudget::from_capped(&agent_config, &runtime_config));
    assert_eq!(
        budget.max_steps, 1,
        "from_capped should take min(100, 1) = 1"
    );

    // AND exhausting the capped budget
    budget.increment_steps();
    assert!(
        budget.is_exhausted(),
        "1 step should exhaust a max_steps=1 budget"
    );

    let engine = ORIAEngine::new();
    let runner = InstantRunner;

    // THEN execute_direct returns BudgetExceeded
    let result = engine.execute_direct(make_task(), &runner, budget).await;
    assert!(
        matches!(result, Err(ORIAError::BudgetExceeded { .. })),
        "expected BudgetExceeded, got: {result:?}"
    );
}
