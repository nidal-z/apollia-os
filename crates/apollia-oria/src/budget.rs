//! StepBudget — runtime-enforced tri-dimensional execution budget.
//!
//! The StepBudget is the most important safety mechanism of Apollia OS
//! (Principle #7: Non-negotiable guardrails). It bounds agent execution
//! along three dimensions:
//!
//! 1. **max_steps** — maximum iterations of the agent's ReAct loop
//! 2. **max_tool_calls** — maximum tool invocations via ToolProxy
//! 3. **wall_clock_limit** — absolute wall-clock timeout
//!
//! Thread-safe via `AtomicU32` counters, shared as `Arc<StepBudget>`
//! between ORIAEngine and ToolProxy.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use apollia_core::StepBudgetConfig;

/// Budget d'execution tri-dimensionnel applique par le runtime.
///
/// Thread-safe grace a `AtomicU32` pour les compteurs et `Instant` pour le chrono.
/// Partage via `Arc<StepBudget>` entre ORIAEngine et ToolProxy.
///
/// Quand le budget est épuisé (steps ou tool_calls), le sender `exhaustion_tx` est
/// consommé pour notifier les waiters via [`wait_for_exhaustion`]. La dimension
/// wall_clock est gérée par un `tokio::time::sleep` sur la durée restante.
///
/// [`wait_for_exhaustion`]: StepBudget::wait_for_exhaustion
pub struct StepBudget {
    /// Nombre maximal de steps autorisees.
    pub max_steps: u32,
    /// Nombre maximal d'appels outils autorisees.
    pub max_tool_calls: u32,
    /// Duree maximale d'execution.
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
    /// Cree un nouveau StepBudget a partir de la config.
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

    /// Cree un StepBudget avec valeurs effectives = min(agent, runtime) par dimension.
    pub fn from_capped(agent: &StepBudgetConfig, runtime: &StepBudgetConfig) -> Self {
        let capped = StepBudgetConfig {
            max_steps: agent.max_steps.min(runtime.max_steps),
            max_tool_calls: agent.max_tool_calls.min(runtime.max_tool_calls),
            wall_clock_secs: agent.wall_clock_secs.min(runtime.wall_clock_secs),
        };
        Self::new(&capped)
    }

    /// Retourne `true` si au moins une des trois dimensions est epuisee.
    pub fn is_exhausted(&self) -> bool {
        self.current_steps.load(Ordering::Relaxed) >= self.max_steps
            || self.current_tool_calls.load(Ordering::Relaxed) >= self.max_tool_calls
            || self.started_at.elapsed() >= self.wall_clock_limit
    }

    /// Incremente le compteur de steps.
    ///
    /// Si ce compteur atteint `max_steps`, notifie les waiters de [`wait_for_exhaustion`].
    ///
    /// [`wait_for_exhaustion`]: StepBudget::wait_for_exhaustion
    pub fn increment_steps(&self) {
        let prev = self.current_steps.fetch_add(1, Ordering::Relaxed);
        if prev + 1 >= self.max_steps {
            self.try_notify_exhaustion();
        }
    }

    /// Incremente le compteur d'appels outils.
    ///
    /// Si ce compteur atteint `max_tool_calls`, notifie les waiters de [`wait_for_exhaustion`].
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

    /// Nombre de steps restantes.
    pub fn steps_left(&self) -> u32 {
        self.max_steps
            .saturating_sub(self.current_steps.load(Ordering::Relaxed))
    }

    /// Nombre d'appels outils restants.
    pub fn tool_calls_left(&self) -> u32 {
        self.max_tool_calls
            .saturating_sub(self.current_tool_calls.load(Ordering::Relaxed))
    }

    /// Duree ecoulee depuis le debut de l'execution.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Crée un `StepBudget` sans limite effective — réservé aux tests unitaires.
    ///
    /// Toutes les dimensions sont fixées à leur maximum pratique :
    /// `max_steps = u32::MAX`, `max_tool_calls = u32::MAX`, `wall_clock = 24 h`.
    pub fn unlimited() -> Self {
        Self::new(&StepBudgetConfig {
            max_steps: u32::MAX,
            max_tool_calls: u32::MAX,
            wall_clock_secs: 86_400,
        })
    }

    /// Crée un `StepBudget` limité à `max_steps` steps — réservé aux tests unitaires.
    ///
    /// Les autres dimensions (`max_tool_calls`, `wall_clock`) sont fixées à leur
    /// maximum pratique pour ne pas interférer avec le test.
    pub fn with_max(max_steps: u32) -> Self {
        Self::new(&StepBudgetConfig {
            max_steps,
            max_tool_calls: u32::MAX,
            wall_clock_secs: 86_400,
        })
    }

    /// Crée un snapshot du budget exposant steps, tool_calls et temps écoulé.
    ///
    /// Les compteurs sont capturés au moment de l'appel. `started_at` est partagé
    /// par copie — `elapsed_secs()` sur la vue reflète toujours le bon temps écoulé.
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

    /// Description lisible de la raison d'epuisement (pour les messages d'erreur).
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
        // GIVEN un StepBudgetConfig avec des valeurs par defaut
        let config = default_config();

        // WHEN on cree un StepBudget
        let budget = StepBudget::new(&config);

        // THEN is_exhausted() retourne false
        assert!(!budget.is_exhausted());
        assert_eq!(budget.steps_left(), 10);
        assert_eq!(budget.tool_calls_left(), 20);
    }

    #[test]
    fn test_steps_exhausted() {
        // GIVEN un budget avec max_steps = 2
        let config = StepBudgetConfig {
            max_steps: 2,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);

        // WHEN on incremente 2 fois
        budget.increment_steps();
        budget.increment_steps();

        // THEN is_exhausted() retourne true et steps_left() == 0
        assert!(budget.is_exhausted());
        assert_eq!(budget.steps_left(), 0);
    }

    #[test]
    fn test_tool_calls_exhausted() {
        // GIVEN un budget avec max_tool_calls = 3
        let config = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 3,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);

        // WHEN on incremente tool_calls 3 fois
        budget.increment_tool_calls();
        budget.increment_tool_calls();
        budget.increment_tool_calls();

        // THEN is_exhausted() retourne true et tool_calls_left() == 0
        assert!(budget.is_exhausted());
        assert_eq!(budget.tool_calls_left(), 0);
    }

    #[test]
    fn test_wall_clock_exhausted() {
        // GIVEN un budget avec wall_clock_limit = 1ms
        let config = StepBudgetConfig {
            max_steps: 100,
            max_tool_calls: 100,
            wall_clock_secs: 0, // 0 seconds = immediately exhausted
        };
        let budget = StepBudget::new(&config);

        // WHEN (immediate, wall_clock_limit = 0s so already expired)
        // THEN is_exhausted() retourne true
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

        // WHEN on cree via from_capped()
        let budget = StepBudget::from_capped(&agent, &runtime);

        // THEN max_steps=10, max_tool_calls=20, wall_clock=300
        assert_eq!(budget.max_steps, 10);
        assert_eq!(budget.max_tool_calls, 20);
        assert_eq!(budget.wall_clock_limit, Duration::from_secs(300));
    }

    #[test]
    fn test_exhaustion_reason_steps() {
        // GIVEN un budget avec max_steps = 1
        let config = StepBudgetConfig {
            max_steps: 1,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        };
        let budget = StepBudget::new(&config);

        // WHEN on incremente 1 fois
        budget.increment_steps();

        // THEN exhaustion_reason() contient "steps"
        let reason = budget.exhaustion_reason().expect("should have reason");
        assert!(reason.contains("steps"), "reason was: {reason}");
    }

    #[test]
    fn test_thread_safety_concurrent_increments() {
        // GIVEN un Arc<StepBudget> avec max_steps = 1000
        let config = StepBudgetConfig {
            max_steps: 1000,
            max_tool_calls: 10000,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));

        // WHEN 10 threads incrementent 100 fois chacun
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

        // THEN current_steps == 1000 et is_exhausted() == true
        assert!(budget.is_exhausted());
        assert_eq!(budget.steps_left(), 0);
    }

    /// `wait_for_exhaustion` completes via the oneshot when `increment_steps` exhausts the budget.
    #[tokio::test]
    async fn test_budget_exhaustion_oneshot_notification() {
        // GIVEN un budget avec max_steps = 1
        let config = StepBudgetConfig {
            max_steps: 1,
            max_tool_calls: 100,
            wall_clock_secs: 60,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let budget_clone = Arc::clone(&budget);

        // WHEN on incrémente dans une tâche concurrente et attend l'épuisement
        let waiter = tokio::spawn(async move { budget_clone.wait_for_exhaustion().await });

        // Give the waiter time to start
        tokio::task::yield_now().await;

        // THEN incrémenter les steps déclenche la notification oneshot
        budget.increment_steps();

        // La future doit se compléter sans atteindre le wall_clock_limit (60s)
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("wait_for_exhaustion should complete within 1s, not poll for 60s")
            .expect("task should not panic");
    }
}
