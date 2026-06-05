//! StepBudget: runtime-enforced tri-dimensional execution budget.
//!
//! The StepBudget is the most important safety mechanism of Apollia OS
//! (a non-negotiable guardrail). It bounds agent execution along three dimensions:
//!
//! 1. **max_steps**: maximum iterations of the agent's ReAct loop
//! 2. **max_tool_calls**: maximum tool invocations via ToolProxy
//! 3. **wall_clock_limit**: absolute wall-clock timeout
//!
//! Thread-safe via `AtomicU32` counters, shared as `Arc<StepBudget>`
//! between ORIAEngine and ToolProxy.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use apollia_core::StepBudgetConfig;

/// Tri-dimensional execution budget enforced by the runtime.
///
/// Thread-safe via `AtomicU32` counters and `Instant` for the timer.
/// Shared via `Arc<StepBudget>` between ORIAEngine and ToolProxy.
///
/// When the budget is exhausted (steps or tool_calls), the `exhaustion_tx` sender
/// is consumed to notify waiters via [`wait_for_exhaustion`]. The wall_clock
/// dimension is handled by a `tokio::time::sleep` on the remaining duration.
///
/// [`wait_for_exhaustion`]: StepBudget::wait_for_exhaustion
pub struct StepBudget {
    /// Maximum number of steps allowed.
    pub max_steps: u32,
    /// Maximum number of tool calls allowed.
    pub max_tool_calls: u32,
    /// Maximum execution duration.
    pub wall_clock_limit: Duration,
    current_steps: AtomicU32,
    current_tool_calls: AtomicU32,
    started_at: Instant,
    /// Sender fired once when steps or tool_calls reaches its limit.
    exhaustion_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// Receiver consumed once by [`wait_for_exhaustion`].
    exhaustion_rx: Mutex<Option<oneshot::Receiver<()>>>,
}

impl StepBudget {
    /// Creates a new StepBudget from the config.
    pub fn new(config: &StepBudgetConfig) -> Self {
        let (tx, rx) = oneshot::channel();
        Self {
            max_steps: config.max_steps,
            max_tool_calls: config.max_tool_calls,
            wall_clock_limit: Duration::from_secs(config.wall_clock_secs),
            current_steps: AtomicU32::new(0),
            current_tool_calls: AtomicU32::new(0),
            started_at: Instant::now(),
            exhaustion_tx: Mutex::new(Some(tx)),
            exhaustion_rx: Mutex::new(Some(rx)),
        }
    }

    /// Creates a StepBudget whose effective values are min(agent, runtime) per dimension.
    pub fn from_capped(agent: &StepBudgetConfig, runtime: &StepBudgetConfig) -> Self {
        let capped = StepBudgetConfig {
            max_steps: agent.max_steps.min(runtime.max_steps),
            max_tool_calls: agent.max_tool_calls.min(runtime.max_tool_calls),
            wall_clock_secs: agent.wall_clock_secs.min(runtime.wall_clock_secs),
        };
        Self::new(&capped)
    }

    /// Returns `true` if at least one of the three dimensions is exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.current_steps.load(Ordering::Relaxed) >= self.max_steps
            || self.current_tool_calls.load(Ordering::Relaxed) >= self.max_tool_calls
            || self.started_at.elapsed() >= self.wall_clock_limit
    }

    /// Increments the step counter.
    ///
    /// When this counter reaches `max_steps`, notifies waiters of [`wait_for_exhaustion`].
    ///
    /// [`wait_for_exhaustion`]: StepBudget::wait_for_exhaustion
    pub fn increment_steps(&self) {
        let prev = self.current_steps.fetch_add(1, Ordering::Relaxed);
        if prev + 1 >= self.max_steps {
            self.try_notify_exhaustion();
        }
    }

    /// Increments the tool call counter.
    ///
    /// When this counter reaches `max_tool_calls`, notifies waiters of [`wait_for_exhaustion`].
    ///
    /// [`wait_for_exhaustion`]: StepBudget::wait_for_exhaustion
    pub fn increment_tool_calls(&self) {
        let prev = self.current_tool_calls.fetch_add(1, Ordering::Relaxed);
        if prev + 1 >= self.max_tool_calls {
            self.try_notify_exhaustion();
        }
    }

    /// Fires the exhaustion oneshot at most once.
    fn try_notify_exhaustion(&self) {
        if let Ok(mut guard) = self.exhaustion_tx.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
    }

    /// Returns a future that resolves when the budget is exhausted.
    ///
    /// Resolves when either:
    /// - `increment_steps` / `increment_tool_calls` fires the exhaustion oneshot, or
    /// - the `wall_clock_limit` elapses.
    ///
    /// Can only be awaited once per `StepBudget` instance (the receiver is consumed).
    /// Subsequent calls fall back to waiting on the remaining wall-clock duration only.
    pub async fn wait_for_exhaustion(&self) {
        let rx = {
            let mut guard = self.exhaustion_rx.lock().unwrap_or_else(|p| p.into_inner());
            guard.take()
        };
        let wall_clock_remaining = self
            .wall_clock_limit
            .saturating_sub(self.started_at.elapsed());

        match rx {
            Some(rx) => {
                tokio::select! {
                    _ = rx => {}
                    _ = tokio::time::sleep(wall_clock_remaining) => {}
                }
            }
            None => {
                tokio::time::sleep(wall_clock_remaining).await;
            }
        }
    }

    /// Number of steps remaining.
    pub fn steps_left(&self) -> u32 {
        self.max_steps
            .saturating_sub(self.current_steps.load(Ordering::Relaxed))
    }

    /// Number of tool calls remaining.
    pub fn tool_calls_left(&self) -> u32 {
        self.max_tool_calls
            .saturating_sub(self.current_tool_calls.load(Ordering::Relaxed))
    }

    /// Time elapsed since execution started.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Create a `StepBudget` with no effective limit, reserved for unit tests.
    ///
    /// All dimensions are set to their practical maximum:
    /// `max_steps = u32::MAX`, `max_tool_calls = u32::MAX`, `wall_clock = 24 h`.
    pub fn unlimited() -> Self {
        Self::new(&StepBudgetConfig {
            max_steps: u32::MAX,
            max_tool_calls: u32::MAX,
            wall_clock_secs: 86_400,
        })
    }

    /// Create a `StepBudget` limited to `max_steps` steps, reserved for unit tests.
    ///
    /// The other dimensions (`max_tool_calls`, `wall_clock`) are set to their
    /// practical maximum so they do not interfere with the test.
    pub fn with_max(max_steps: u32) -> Self {
        Self::new(&StepBudgetConfig {
            max_steps,
            max_tool_calls: u32::MAX,
            wall_clock_secs: 86_400,
        })
    }

    /// Create a budget snapshot exposing steps, tool_calls and elapsed time.
    ///
    /// Counters are captured at call time. `started_at` is shared by copy, so
    /// `elapsed_secs()` on the view always reflects the correct elapsed time.
    pub fn to_budget_view(&self) -> apollia_llm::StepBudgetView {
        apollia_llm::StepBudgetView::with_tool_tracking(
            Arc::new(AtomicU32::new(self.current_steps.load(Ordering::Relaxed))),
            self.max_steps,
            Arc::new(AtomicU32::new(
                self.current_tool_calls.load(Ordering::Relaxed),
            )),
            self.max_tool_calls,
            self.started_at,
        )
    }

    /// Human-readable description of the exhaustion reason (for error messages).
    pub fn exhaustion_reason(&self) -> Option<String> {
        if self.current_steps.load(Ordering::Relaxed) >= self.max_steps {
            return Some(format!(
                "max steps reached ({}/{})",
                self.current_steps.load(Ordering::Relaxed),
                self.max_steps
            ));
        }
        if self.current_tool_calls.load(Ordering::Relaxed) >= self.max_tool_calls {
            return Some(format!(
                "max tool calls reached ({}/{})",
                self.current_tool_calls.load(Ordering::Relaxed),
                self.max_tool_calls
            ));
        }
        if self.started_at.elapsed() >= self.wall_clock_limit {
            return Some(format!(
                "wall clock limit exceeded ({:.1}s / {:.1}s)",
                self.started_at.elapsed().as_secs_f64(),
                self.wall_clock_limit.as_secs_f64()
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> StepBudgetConfig {
        StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        }
    }

    #[test]
    fn test_new_budget_not_exhausted() {
        // GIVEN a StepBudgetConfig with default values
        let config = default_config();

        // WHEN we create a StepBudget
        let budget = StepBudget::new(&config);

        // THEN is_exhausted() returns false
        assert!(!budget.is_exhausted());
        assert_eq!(budget.steps_left(), 10);
        assert_eq!(budget.tool_calls_left(), 20);
    }

    #[test]
    fn test_steps_exhausted() {
        // GIVEN a budget with max_steps = 2
        let config = StepBudgetConfig {
            max_steps: 2,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);

        // WHEN we increment twice
        budget.increment_steps();
        budget.increment_steps();

        // THEN is_exhausted() returns true and steps_left() == 0
        assert!(budget.is_exhausted());
        assert_eq!(budget.steps_left(), 0);
    }

    #[test]
    fn test_tool_calls_exhausted() {
        // GIVEN a budget with max_tool_calls = 3
        let config = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 3,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);

        // WHEN we increment tool_calls 3 times
        budget.increment_tool_calls();
        budget.increment_tool_calls();
        budget.increment_tool_calls();

        // THEN is_exhausted() returns true and tool_calls_left() == 0
        assert!(budget.is_exhausted());
        assert_eq!(budget.tool_calls_left(), 0);
    }

    #[test]
    fn test_wall_clock_exhausted() {
        // GIVEN a budget with wall_clock_limit = 0s
        let config = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 100,
            wall_clock_secs: 0, // 0 seconds = immediately exhausted
        };
        let budget = StepBudget::new(&config);

        // WHEN (immediate, wall_clock_limit = 0s so already expired)
        // THEN is_exhausted() returns true
        assert!(budget.is_exhausted());
    }

    #[test]
    fn test_from_capped_takes_minimum() {
        // GIVEN agent config (max_steps=100, max_tool_calls=50, wall_clock=600)
        let agent = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 50,
            wall_clock_secs: 600,
        };
        // AND runtime config (max_steps=10, max_tool_calls=20, wall_clock=300)
        let runtime = StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        };

        // WHEN we create it via from_capped()
        let budget = StepBudget::from_capped(&agent, &runtime);

        // THEN max_steps=10, max_tool_calls=20, wall_clock=300
        assert_eq!(budget.max_steps, 10);
        assert_eq!(budget.max_tool_calls, 20);
        assert_eq!(budget.wall_clock_limit, Duration::from_secs(300));
    }

    // An autonomy tier budget is clamped to the runtime ceiling by from_capped.
    #[test]
    fn test_autonomy_tier_budget_clamped_to_runtime_ceiling() {
        // GIVEN a BoundedAutonomous tier (max_steps = 300) and a ceiling at 100
        let lc = apollia_core::AutonomyLevelConfig::default_for(
            apollia_core::AutonomyLevel::BoundedAutonomous,
        );
        let ceiling = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 200,
            wall_clock_secs: 600,
        };

        // WHEN building the effective StepBudget via from_capped
        let budget = StepBudget::from_capped(&lc.budget, &ceiling);

        // THEN the runtime ceiling wins on the constrained dimension
        assert_eq!(budget.max_steps, 100);
        assert!(!budget.is_exhausted());
    }

    #[test]
    fn test_exhaustion_reason_steps() {
        // GIVEN a budget with max_steps = 1
        let config = StepBudgetConfig {
            max_steps: 1,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);

        // WHEN we increment once
        budget.increment_steps();

        // THEN exhaustion_reason() contains "steps"
        let reason = budget.exhaustion_reason().expect("should have reason");
        assert!(reason.contains("steps"), "reason was: {reason}");
    }

    #[test]
    fn test_thread_safety_concurrent_increments() {
        // GIVEN an Arc<StepBudget> with max_steps = 1000
        let config = StepBudgetConfig {
            max_steps: 1000,
            max_tool_calls: 10000,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));

        // WHEN 10 threads each increment 100 times
        let mut handles = vec![];
        for _ in 0..10 {
            let b = Arc::clone(&budget);
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    b.increment_steps();
                }
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }

        // THEN current_steps == 1000 and is_exhausted() == true
        assert!(budget.is_exhausted());
        assert_eq!(budget.steps_left(), 0);
    }

    /// `wait_for_exhaustion` completes via the oneshot when `increment_steps` exhausts the budget.
    #[tokio::test]
    async fn test_budget_exhaustion_oneshot_notification() {
        // GIVEN a budget with max_steps = 1
        let config = StepBudgetConfig {
            max_steps: 1,
            max_tool_calls: 100,
            wall_clock_secs: 60,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let budget_clone = Arc::clone(&budget);

        // WHEN we increment in a concurrent task and wait for exhaustion
        let waiter = tokio::spawn(async move { budget_clone.wait_for_exhaustion().await });

        // Give the waiter time to start
        tokio::task::yield_now().await;

        // THEN incrementing the steps triggers the oneshot notification
        budget.increment_steps();

        // The future must complete without reaching the wall_clock_limit (60s)
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("wait_for_exhaustion should complete within 1s, not poll for 60s")
            .expect("task should not panic");
    }
}
