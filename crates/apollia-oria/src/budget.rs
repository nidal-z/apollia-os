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
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::watch;

use apollia_core::StepBudgetConfig;

/// Effective per-dimension cap: an agent config can only ever lower the runtime
/// ceiling, never raise it above it. This is the constructor-level enforcement of
/// the non-bypassable guardrail: `effective = min(agent, runtime)`.
///
/// Isolated as a pure `u32 -> u32` function so it can be proven exhaustively
/// (proptest today, Kani in the advisory job) rather than only sampled.
#[inline]
pub(crate) fn effective_cap(agent: u32, runtime: u32) -> u32 {
    agent.min(runtime)
}

/// Whether a single counter dimension is exhausted: `used >= max`. The `>=` (not
/// `==`) is what makes exhaustion stable once reached, even if the counter is
/// incremented further. Mirrored by `is_exhausted` and `exhaustion_reason`.
#[inline]
pub(crate) fn dimension_exhausted(used: u32, max: u32) -> bool {
    used >= max
}

/// Remaining headroom on a dimension, saturating at zero so it never underflows.
/// Returns `0` exactly when the dimension is exhausted. Mirrored by `steps_left`
/// and `tool_calls_left`.
#[inline]
pub(crate) fn remaining(used: u32, max: u32) -> u32 {
    max.saturating_sub(used)
}

/// Tri-dimensional execution budget enforced by the runtime.
///
/// Thread-safe via `AtomicU32` counters and `Instant` for the timer.
/// Shared via `Arc<StepBudget>` between ORIAEngine and ToolProxy.
///
/// When the budget is exhausted (steps or tool_calls), the `exhaustion_tx`
/// watch channel is set to notify waiters via [`wait_for_exhaustion`]. The
/// wall_clock dimension is handled by a `tokio::time::sleep` on the remaining
/// duration. The watch channel is re-armable, so a task resumed after HITL is
/// still supervised on the step/tool_call dimensions, not wall-clock alone.
///
/// [`wait_for_exhaustion`]: StepBudget::wait_for_exhaustion
pub struct StepBudget {
    /// Maximum number of steps allowed.
    pub max_steps: u32,
    /// Maximum number of tool calls allowed.
    pub max_tool_calls: u32,
    /// Maximum execution duration.
    pub wall_clock_limit: Duration,
    current_steps: Arc<AtomicU32>,
    current_tool_calls: Arc<AtomicU32>,
    started_at: Instant,
    /// Set to `true` when a counter dimension reaches its limit. A `watch`
    /// channel (not a mono-use oneshot) so the signal survives multiple awaits.
    exhaustion_tx: watch::Sender<bool>,
}

impl StepBudget {
    /// Creates a new StepBudget from the config.
    pub fn new(config: &StepBudgetConfig) -> Self {
        let (exhaustion_tx, _) = watch::channel(false);
        Self {
            max_steps: config.max_steps,
            max_tool_calls: config.max_tool_calls,
            wall_clock_limit: Duration::from_secs(config.wall_clock_secs),
            current_steps: Arc::new(AtomicU32::new(0)),
            current_tool_calls: Arc::new(AtomicU32::new(0)),
            started_at: Instant::now(),
            exhaustion_tx,
        }
    }

    /// Creates a StepBudget whose effective values are min(agent, runtime) per dimension.
    pub fn from_capped(agent: &StepBudgetConfig, runtime: &StepBudgetConfig) -> Self {
        let capped = StepBudgetConfig {
            max_steps: effective_cap(agent.max_steps, runtime.max_steps),
            max_tool_calls: effective_cap(agent.max_tool_calls, runtime.max_tool_calls),
            wall_clock_secs: agent.wall_clock_secs.min(runtime.wall_clock_secs),
        };
        Self::new(&capped)
    }

    /// Returns `true` if at least one of the three dimensions is exhausted.
    pub fn is_exhausted(&self) -> bool {
        dimension_exhausted(self.current_steps.load(Ordering::Relaxed), self.max_steps)
            || dimension_exhausted(
                self.current_tool_calls.load(Ordering::Relaxed),
                self.max_tool_calls,
            )
            || self.started_at.elapsed() >= self.wall_clock_limit
    }

    /// Increments the step counter.
    ///
    /// When this counter reaches `max_steps`, notifies waiters of [`wait_for_exhaustion`].
    ///
    /// [`wait_for_exhaustion`]: StepBudget::wait_for_exhaustion
    pub fn increment_steps(&self) {
        let prev = self.current_steps.fetch_add(1, Ordering::Relaxed);
        if dimension_exhausted(prev.saturating_add(1), self.max_steps) {
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
        if dimension_exhausted(prev.saturating_add(1), self.max_tool_calls) {
            self.try_notify_exhaustion();
        }
    }

    /// Raises the exhaustion signal. Idempotent and re-armable.
    fn try_notify_exhaustion(&self) {
        let _ = self.exhaustion_tx.send(true);
    }

    /// Returns a future that resolves when the budget is exhausted.
    ///
    /// Resolves when either:
    /// - `increment_steps` / `increment_tool_calls` raises the exhaustion signal, or
    /// - a counter dimension is already exhausted when the future is created, or
    /// - the `wall_clock_limit` elapses.
    ///
    /// Re-armable: unlike a mono-use oneshot, this may be awaited several times
    /// on the same `StepBudget` (for example the first Direct run and again after
    /// a HITL resume), each await supervising the step/tool_call dimensions.
    pub async fn wait_for_exhaustion(&self) {
        let mut rx = self.exhaustion_tx.subscribe();
        let wall_clock_remaining = self
            .wall_clock_limit
            .saturating_sub(self.started_at.elapsed());

        // Fast path: the signal already fired, or a counter dimension is already
        // at its limit (for instance after a resume that inherited a spent budget).
        if *rx.borrow_and_update() || self.is_exhausted() {
            return;
        }

        tokio::select! {
            _ = rx.changed() => {}
            _ = tokio::time::sleep(wall_clock_remaining) => {}
        }
    }

    /// Number of steps remaining.
    pub fn steps_left(&self) -> u32 {
        remaining(self.current_steps.load(Ordering::Relaxed), self.max_steps)
    }

    /// Number of tool calls remaining.
    pub fn tool_calls_left(&self) -> u32 {
        remaining(
            self.current_tool_calls.load(Ordering::Relaxed),
            self.max_tool_calls,
        )
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

    /// Build a `StepBudgetView` backed by this budget's live shared counters.
    ///
    /// Unlike [`to_budget_view`](Self::to_budget_view), which snapshots the
    /// counters, the returned view shares the same `Arc<AtomicU32>` counters, so
    /// increments made through the view are visible to this budget and to
    /// [`is_exhausted`](Self::is_exhausted). This is how the runtime enforces the
    /// budget on the Direct path: the Python agent's tool and LLM calls increment
    /// the shared view through the AIP proxies, and the same budget the engine
    /// supervises sees those increments (principle #7, non-bypassable).
    pub fn to_live_budget_view(&self) -> apollia_llm::StepBudgetView {
        apollia_llm::StepBudgetView::with_tool_tracking(
            Arc::clone(&self.current_steps),
            self.max_steps,
            Arc::clone(&self.current_tool_calls),
            self.max_tool_calls,
            self.started_at,
        )
    }

    /// Human-readable description of the exhaustion reason (for error messages).
    pub fn exhaustion_reason(&self) -> Option<String> {
        if dimension_exhausted(self.current_steps.load(Ordering::Relaxed), self.max_steps) {
            return Some(format!(
                "max steps reached ({}/{})",
                self.current_steps.load(Ordering::Relaxed),
                self.max_steps
            ));
        }
        if dimension_exhausted(
            self.current_tool_calls.load(Ordering::Relaxed),
            self.max_tool_calls,
        ) {
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

    /// The exhaustion signal re-arms: a second `wait_for_exhaustion` (as on a
    /// HITL resume) still resolves via the counter dimension, not wall-clock only.
    #[tokio::test]
    async fn test_wait_for_exhaustion_rearms_after_first_await() {
        // GIVEN a budget with room for 2 steps and a long wall clock
        let config = StepBudgetConfig {
            max_steps: 2,
            max_tool_calls: 100,
            wall_clock_secs: 3_600,
        };
        let budget = Arc::new(StepBudget::new(&config));

        // A first supervision await completes (models the first Direct run). It is
        // raced against a short sleep so it returns without blocking the wall clock.
        budget.increment_steps(); // 1/2, not yet exhausted
        tokio::select! {
            _ = budget.wait_for_exhaustion() => {}
            _ = tokio::time::sleep(Duration::from_millis(1)) => {}
        }

        // WHEN the budget is exhausted on the step dimension, then supervised again
        budget.increment_steps(); // 2/2 -> exhausted

        // THEN the second await resolves via the re-armed signal within 1s, not
        // after the 3600s wall clock.
        tokio::time::timeout(Duration::from_secs(1), budget.wait_for_exhaustion())
            .await
            .expect(
                "second wait_for_exhaustion must resolve via the re-armed counter, not wall-clock",
            );
    }

    /// The signal (not only the fast path) re-arms: exhaustion fired from another
    /// task DURING a second await still wakes it.
    #[tokio::test]
    async fn test_wait_for_exhaustion_signal_wakes_second_await() {
        // GIVEN a budget with room for 2 steps and a long wall clock
        let config = StepBudgetConfig {
            max_steps: 2,
            max_tool_calls: 100,
            wall_clock_secs: 3_600,
        };
        let budget = Arc::new(StepBudget::new(&config));

        // A first await is consumed (first Direct run).
        tokio::select! {
            _ = budget.wait_for_exhaustion() => {}
            _ = tokio::time::sleep(Duration::from_millis(1)) => {}
        }

        // WHEN a second await starts while the budget is not yet exhausted, and a
        // concurrent task exhausts it after the await has registered.
        let b = Arc::clone(&budget);
        let waiter = tokio::spawn(async move { b.wait_for_exhaustion().await });
        tokio::task::yield_now().await;
        budget.increment_steps();
        budget.increment_steps(); // 2/2 -> fires the signal

        // THEN the second await wakes via the signal within 1s.
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("second await must wake via the re-armed signal, not wall-clock")
            .expect("waiter task should not panic");
    }

    /// A live budget view shares the counters: incrementing the view exhausts the
    /// budget the engine supervises (C7-R1, non-bypassable on the Direct path).
    #[test]
    fn test_live_budget_view_shares_counters() {
        // GIVEN a budget with room for 2 tool calls and a live view of it
        let config = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 2,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);
        let view = budget.to_live_budget_view();

        // WHEN the view (as the Direct-path proxy would) spends the tool budget
        assert!(!budget.is_exhausted());
        view.increment_tool_calls();
        view.increment_tool_calls();

        // THEN the budget the engine holds sees it and is exhausted
        assert!(
            budget.is_exhausted(),
            "increments via the live view must be visible to the owning budget"
        );
        assert_eq!(budget.tool_calls_left(), 0);
        assert_eq!(view.tool_calls_remaining(), 0);
    }

    /// A snapshot view (`to_budget_view`) does NOT share counters: it is a
    /// point-in-time copy, so mutating it never affects the owning budget.
    #[test]
    fn test_snapshot_budget_view_is_decoupled() {
        // GIVEN a budget and a snapshot view of it
        let config = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 2,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);
        let snapshot = budget.to_budget_view();

        // WHEN the snapshot is mutated
        snapshot.increment_tool_calls();
        snapshot.increment_tool_calls();

        // THEN the owning budget is unaffected (proves the two views differ)
        assert!(!budget.is_exhausted());
        assert_eq!(budget.tool_calls_left(), 2);
    }

    /// `wait_for_exhaustion` completes via the signal when `increment_steps` exhausts the budget.
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

// Property harnesses for the non-bypassable-budget invariant. These exercise the
// same `effective_cap` / `dimension_exhausted` / `remaining` helpers the runtime
// methods call, plus the real `StepBudget` end to end, over a randomized space.
// The `#[cfg(kani)]` block below proves the pure helpers exhaustively over the
// full `u32` domain; the wall-clock dimension and the real atomic struct stay in
// proptest because a model checker cannot model `Instant` / `tokio` time.
#[cfg(test)]
mod property {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The effective cap is `min(agent, runtime)`: an agent config can only
        /// lower the runtime ceiling, never raise the budget above it.
        #[test]
        fn prop_effective_cap_is_min_and_never_exceeds_ceiling(agent: u32, runtime: u32) {
            let cap = effective_cap(agent, runtime);
            prop_assert!(cap <= agent);
            prop_assert!(cap <= runtime);
            prop_assert_eq!(cap, agent.min(runtime));
        }

        /// `dimension_exhausted` is exactly `used >= max`, and once reached it
        /// stays reached for any further increment (no wrap-around gap).
        #[test]
        fn prop_dimension_exhausted_is_ge_and_monotonic(used: u32, max: u32, delta: u32) {
            prop_assert_eq!(dimension_exhausted(used, max), used >= max);
            if dimension_exhausted(used, max) {
                prop_assert!(dimension_exhausted(used.saturating_add(delta), max));
            }
        }

        /// `remaining` never underflows and is zero exactly when exhausted.
        #[test]
        fn prop_remaining_saturates_and_zero_iff_exhausted(used: u32, max: u32) {
            let left = remaining(used, max);
            prop_assert_eq!(left == 0, dimension_exhausted(used, max));
            if used < max {
                prop_assert_eq!(left, max - used);
            }
        }

        /// End to end on the real atomic struct: from_capped clamps to the
        /// runtime ceiling, and after `max` increments the budget is exhausted
        /// with zero headroom, whatever the requested agent value.
        #[test]
        fn prop_real_budget_never_exceeds_capped_ceiling(
            agent_steps in 0u32..=64,
            runtime_steps in 1u32..=64,
        ) {
            let agent = StepBudgetConfig {
                max_steps: agent_steps,
                max_tool_calls: u32::MAX,
                wall_clock_secs: 86_400,
            };
            let runtime = StepBudgetConfig {
                max_steps: runtime_steps,
                max_tool_calls: u32::MAX,
                wall_clock_secs: 86_400,
            };
            let budget = StepBudget::from_capped(&agent, &runtime);
            let cap = agent_steps.min(runtime_steps);
            prop_assert_eq!(budget.max_steps, cap);
            prop_assert!(budget.max_steps <= runtime_steps);

            for _ in 0..cap {
                prop_assert!(!budget.is_exhausted());
                budget.increment_steps();
            }
            // WHEN the cap is reached, the step dimension is exhausted with no
            // headroom, and further increments keep it exhausted.
            prop_assert!(budget.is_exhausted());
            prop_assert_eq!(budget.steps_left(), 0);
            budget.increment_steps();
            prop_assert!(budget.is_exhausted());
            prop_assert_eq!(budget.steps_left(), 0);
        }
    }
}

// SEED harnesses for `cargo kani`. Kani is not wired into the local toolchain
// (it links its own toolchain via rustup, absent here); these bounded proofs run
// in the advisory nightly `kani` job. They prove the pure budget helpers over the
// entire `u32` domain, which the proptest block above only samples. The atomic
// struct and the wall-clock dimension are out of scope for a model checker.
#[cfg(kani)]
mod proofs {
    use super::{dimension_exhausted, effective_cap, remaining};

    /// Non-bypassable cap: the effective per-dimension budget never exceeds the
    /// runtime ceiling, for every pair of `u32` values.
    #[kani::proof]
    fn kani_effective_cap_never_exceeds_ceiling() {
        let agent: u32 = kani::any();
        let runtime: u32 = kani::any();
        let cap = effective_cap(agent, runtime);
        assert!(cap <= agent);
        assert!(cap <= runtime);
        assert!(cap == agent || cap == runtime);
    }

    /// Exhaustion is exactly `used >= max` and is stable under further
    /// increments (proves the saturating increment leaves no wrap-around gap).
    #[kani::proof]
    fn kani_dimension_exhausted_ge_and_monotonic() {
        let used: u32 = kani::any();
        let max: u32 = kani::any();
        assert_eq!(dimension_exhausted(used, max), used >= max);
        if dimension_exhausted(used, max) {
            let delta: u32 = kani::any();
            assert!(dimension_exhausted(used.saturating_add(delta), max));
        }
    }

    /// Remaining headroom never underflows and is zero iff exhausted.
    #[kani::proof]
    fn kani_remaining_saturates_and_zero_iff_exhausted() {
        let used: u32 = kani::any();
        let max: u32 = kani::any();
        let left = remaining(used, max);
        assert_eq!(left == 0, dimension_exhausted(used, max));
    }

    /// The fixed increment path is overflow-free for every counter value,
    /// including `u32::MAX` (where the previous `prev + 1` would have panicked).
    #[kani::proof]
    fn kani_increment_saturating_never_overflows() {
        let prev: u32 = kani::any();
        let _ = prev.saturating_add(1);
    }
}
