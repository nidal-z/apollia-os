//! Session metrics aggregation actor.
//!
//! `SessionMetricsActor` subscribes to the `EventBus` and maintains a
//! `session_id -> SessionMetrics` map. It reacts to the following events:
//!
//! - [`RuntimeEvent::LlmCallCompleted`]: increments tokens (non-meta).
//! - [`RuntimeEvent::ChatToolCallStarted`] / [`RuntimeEvent::ChatToolCallCompleted`]:
//!   measures latency on the actor side (the event carries no `duration_ms`).
//! - [`RuntimeEvent::ContextCompacted`]: records a `SummarizationEvent`.
//! - [`RuntimeEvent::MetaLlmBudgetExceeded`]: flags the blocking alert and
//!   re-emits `SessionMetricsUpdated`.
//!
//! On each update, emits [`RuntimeEvent::SessionMetricsUpdated`] with the full
//! snapshot and the current alert level. The frontend consumes this event to
//! refresh the session metrics panel.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use apollia_core::events::RuntimeEvent;
use apollia_core::session_metrics::{
    BudgetAlertLevel, SessionMetrics, SessionThresholds, SummarizationEvent, ToolTiming,
};
use apollia_core::EventBusSender;
use tokio::task::JoinHandle;

use crate::eventbus::EventBusReceiver;

/// Shared snapshot letting the API and commands expose current state without
/// re-subscribing a new receiver.
///
/// Updated on each event consumed by the actor. Cloned via `Arc`.
pub type SessionMetricsStore = Arc<Mutex<HashMap<String, SessionMetrics>>>;

/// In-flight tool call, used to measure latency on the actor side.
struct InFlightToolCall {
    session_id: String,
    tool_name: String,
    started_at: Instant,
}

/// Actor handle: owns the shared store and the Tokio task handle.
pub struct SessionMetricsActor {
    store: SessionMetricsStore,
    handle: JoinHandle<()>,
}

impl SessionMetricsActor {
    /// Starts the actor. Returns a `SessionMetricsActor` whose store is cloneable via `store()`.
    ///
    /// The actor stops when the `EventBus` closes.
    pub fn spawn(
        mut rx: EventBusReceiver,
        bus: EventBusSender,
        thresholds: SessionThresholds,
        context_window_max: u64,
        token_budget: u64,
    ) -> Self {
        let store: SessionMetricsStore = Arc::new(Mutex::new(HashMap::new()));
        let store_task = Arc::clone(&store);
        let budget = MetricsBudget {
            thresholds,
            context_window_max,
            token_budget,
        };

        let handle = tokio::spawn(async move {
            // Map tool_call_id (message_id) -> in-flight call, for duration computation.
            let mut in_flight: HashMap<String, InFlightToolCall> = HashMap::new();

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let updates = process_event(&event, &store_task, &mut in_flight, budget);
                        for (session_id, metrics, alert) in updates {
                            let _ = bus.send(RuntimeEvent::SessionMetricsUpdated {
                                session_id,
                                metrics,
                                alert,
                            });
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "SessionMetricsActor lagged, events dropped");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("EventBus closed, stopping SessionMetricsActor");
                        break;
                    }
                }
            }
        });

        Self { store, handle }
    }

    /// Access to the shared store (snapshots keyed by session_id).
    pub fn store(&self) -> SessionMetricsStore {
        Arc::clone(&self.store)
    }

    /// Waits for the task to finish. Useful in tests.
    #[cfg(test)]
    pub async fn join(self) {
        let _ = self.handle.await;
    }

    /// Aborts the task, used at shutdown when waiting is not desired.
    pub fn abort(self) {
        self.handle.abort();
    }
}

/// Detached variant: spawns the actor and returns only the [`SessionMetricsStore`].
///
/// The Tokio task lives as long as the `EventBus` stays open. Useful in integrations
/// (Tauri desktop) where you only want to expose the shared store via `app.manage(...)`
/// without keeping an explicit handle.
pub fn spawn_detached(
    rx: EventBusReceiver,
    bus: EventBusSender,
    thresholds: SessionThresholds,
    context_window_max: u64,
    token_budget: u64,
) -> SessionMetricsStore {
    let actor = SessionMetricsActor::spawn(rx, bus, thresholds, context_window_max, token_budget);
    let store = actor.store();
    // Dropping `actor` does not cancel the Tokio task: `JoinHandle` has no `Drop`
    // that aborts. The task keeps living until the `EventBus` closes.
    drop(actor);
    store
}

/// Processes an event and returns the list of updates to emit
/// (session_id, snapshot, alert level).
///
/// Kept in a (mostly) pure function so the logic can be tested without
/// spawning a Tokio actor.
/// Budget parameters shared across all processed events.
#[derive(Debug, Clone, Copy)]
struct MetricsBudget {
    thresholds: SessionThresholds,
    context_window_max: u64,
    token_budget: u64,
}

fn process_event(
    event: &RuntimeEvent,
    store: &SessionMetricsStore,
    in_flight: &mut HashMap<String, InFlightToolCall>,
    budget: MetricsBudget,
) -> Vec<(String, SessionMetrics, BudgetAlertLevel)> {
    let MetricsBudget {
        thresholds,
        context_window_max,
        token_budget,
    } = budget;
    match event {
        RuntimeEvent::LlmCallCompleted {
            task_id,
            prompt_tokens,
            completion_tokens,
            ..
        } => {
            // The event carries no session_id, so task_id is used as the key.
            // When task_id is absent, fall back to the global session.
            let session_id = task_id.clone().unwrap_or_else(|| "global".to_string());
            let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
            let m = guard
                .entry(session_id.clone())
                .or_insert_with(|| default_metrics(context_window_max, token_budget));
            m.record_llm_call(*prompt_tokens, *completion_tokens, 0, false);
            let alert = thresholds.evaluate(m.tokens_used_for_budget(), m.token_budget);
            vec![(session_id, m.clone(), alert)]
        }

        RuntimeEvent::ChatToolCallStarted {
            session_id,
            message_id,
            tool_name,
            ..
        } => {
            in_flight.insert(
                message_id.clone(),
                InFlightToolCall {
                    session_id: session_id.clone(),
                    tool_name: tool_name.clone(),
                    started_at: Instant::now(),
                },
            );
            vec![]
        }

        RuntimeEvent::ChatToolCallCompleted { message_id, .. } => {
            let Some(call) = in_flight.remove(message_id) else {
                return vec![];
            };
            let actual_ms = call.started_at.elapsed().as_millis() as u64;
            let expected_ms = apollia_llm::tool_performance_hints::lookup(&call.tool_name)
                .map(|h| h.expected_duration_ms);
            let timing = ToolTiming::new(&call.tool_name, expected_ms, actual_ms);

            let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
            let m = guard
                .entry(call.session_id.clone())
                .or_insert_with(|| default_metrics(context_window_max, token_budget));
            m.push_tool_timing(timing);
            let alert = thresholds.evaluate(m.tokens_used_for_budget(), m.token_budget);
            vec![(call.session_id, m.clone(), alert)]
        }

        RuntimeEvent::ContextCompacted {
            summary_chars,
            original_messages,
        } => {
            // `ContextCompacted` carries no session_id, so it applies to every
            // active session (the common case is a single active session).
            let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
            let mut updates = Vec::new();
            // Rough estimate: 1 token is approximately 4 characters.
            let summary_tokens = (*summary_chars as u64) / 4;
            for (session_id, m) in guard.iter_mut() {
                let tokens_before = m.context_window_used;
                let tokens_saved = tokens_before.saturating_sub(summary_tokens);
                m.push_summarization(SummarizationEvent {
                    messages_summarized_count: *original_messages,
                    tokens_saved,
                    summary_excerpt: format!("[{summary_chars} chars résumés]"),
                });
                m.set_context_window_used(summary_tokens);
                let alert = thresholds.evaluate(m.tokens_used_for_budget(), m.token_budget);
                updates.push((session_id.clone(), m.clone(), alert));
            }
            updates
        }

        RuntimeEvent::MetaLlmBudgetExceeded {
            session_id,
            tokens_used,
            budget,
        } => {
            let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
            let m = guard
                .entry(session_id.clone())
                .or_insert_with(|| default_metrics(context_window_max, token_budget));
            m.tokens_meta = m.tokens_meta.max(*tokens_used);
            if *budget > 0 {
                m.token_budget = *budget;
            }
            // Force the Block alert as soon as the event arrives.
            vec![(session_id.clone(), m.clone(), BudgetAlertLevel::Block)]
        }

        _ => vec![],
    }
}

fn default_metrics(context_window_max: u64, token_budget: u64) -> SessionMetrics {
    SessionMetrics {
        context_window_max,
        token_budget,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventbus::EventBus;

    fn store() -> SessionMetricsStore {
        Arc::new(Mutex::new(HashMap::new()))
    }

    #[test]
    fn llm_call_accumulates_tokens_under_task_id() {
        // GIVEN an empty store and an LlmCallCompleted with task_id "task-1"
        let store = store();
        let mut in_flight = HashMap::new();
        let event = RuntimeEvent::LlmCallCompleted {
            backend: "anthropic".into(),
            model: "sonnet".into(),
            task_id: Some("task-1".into()),
            step_id: None,
            prompt_tokens: 100,
            completion_tokens: 40,
            latency_ms: 200,
            cost_usd: Some(0.001),
            run_id: None,
        };

        // WHEN
        let updates = process_event(
            &event,
            &store,
            &mut in_flight,
            MetricsBudget {
                thresholds: SessionThresholds::default(),
                context_window_max: 200_000,
                token_budget: 10_000,
            },
        );

        // THEN one update is emitted, tokens accumulated in session "task-1"
        assert_eq!(updates.len(), 1);
        let (sid, m, alert) = &updates[0];
        assert_eq!(sid, "task-1");
        assert_eq!(m.tokens_in, 100);
        assert_eq!(m.tokens_out, 40);
        assert_eq!(*alert, BudgetAlertLevel::Ok);
    }

    #[test]
    fn tool_call_pair_emits_timing() {
        // GIVEN a ChatToolCallStarted followed by a ChatToolCallCompleted
        let store = store();
        let mut in_flight = HashMap::new();
        let started = RuntimeEvent::ChatToolCallStarted {
            session_id: "sess-1".into(),
            message_id: "msg-1".into(),
            tool_name: "file_read".into(),
            input_preview: "{}".into(),
            rationale: None,
        };
        let completed = RuntimeEvent::ChatToolCallCompleted {
            session_id: "sess-1".into(),
            message_id: "msg-1".into(),
            tool_name: "file_read".into(),
            success: true,
            output_preview: None,
            analysis: None,
        };

        // WHEN started then completed
        let u1 = process_event(
            &started,
            &store,
            &mut in_flight,
            MetricsBudget {
                thresholds: SessionThresholds::default(),
                context_window_max: 0,
                token_budget: 0,
            },
        );
        assert!(u1.is_empty(), "started ne doit pas émettre de snapshot");
        assert!(in_flight.contains_key("msg-1"));

        let u2 = process_event(
            &completed,
            &store,
            &mut in_flight,
            MetricsBudget {
                thresholds: SessionThresholds::default(),
                context_window_max: 0,
                token_budget: 0,
            },
        );

        // THEN a timing is added for the session
        assert_eq!(u2.len(), 1);
        let (sid, m, _alert) = &u2[0];
        assert_eq!(sid, "sess-1");
        assert_eq!(m.tool_timings.len(), 1);
        assert_eq!(m.tool_timings[0].tool_name, "file_read");
        assert!(!in_flight.contains_key("msg-1"));
    }

    #[test]
    fn completed_without_started_is_a_noop() {
        // GIVEN an orphan completed event (no prior started)
        let store = store();
        let mut in_flight = HashMap::new();
        let completed = RuntimeEvent::ChatToolCallCompleted {
            session_id: "sess-x".into(),
            message_id: "msg-x".into(),
            tool_name: "bash".into(),
            success: true,
            output_preview: None,
            analysis: None,
        };
        // WHEN
        let updates = process_event(
            &completed,
            &store,
            &mut in_flight,
            MetricsBudget {
                thresholds: SessionThresholds::default(),
                context_window_max: 0,
                token_budget: 0,
            },
        );
        // THEN no update
        assert!(updates.is_empty());
    }

    #[test]
    fn threshold_crossing_reports_warning_then_block() {
        // GIVEN a budget of 1000 and successive calls
        let store = store();
        let mut in_flight = HashMap::new();
        let budget = MetricsBudget {
            thresholds: SessionThresholds::default(),
            context_window_max: 0,
            token_budget: 1000,
        };

        let mk = |tokens: u32| RuntimeEvent::LlmCallCompleted {
            backend: "b".into(),
            model: "m".into(),
            task_id: Some("s".into()),
            step_id: None,
            prompt_tokens: tokens,
            completion_tokens: 0,
            latency_ms: 10,
            cost_usd: None,
            run_id: None,
        };

        // WHEN first call below the warning threshold
        let u1 = process_event(&mk(500), &store, &mut in_flight, budget);
        assert_eq!(u1[0].2, BudgetAlertLevel::Ok);

        // AND second call crosses 80%
        let u2 = process_event(&mk(350), &store, &mut in_flight, budget);
        // THEN warning
        assert_eq!(u2[0].2, BudgetAlertLevel::Warning);

        // AND third call crosses 100%
        let u3 = process_event(&mk(200), &store, &mut in_flight, budget);
        // THEN block
        assert_eq!(u3[0].2, BudgetAlertLevel::Block);
    }

    #[test]
    fn context_compacted_pushes_summarization_event() {
        // GIVEN a session with some context
        let store = store();
        {
            let mut g = store.lock().unwrap();
            g.insert(
                "sess".into(),
                SessionMetrics {
                    context_window_used: 8_000,
                    context_window_max: 10_000,
                    ..Default::default()
                },
            );
        }
        let mut in_flight = HashMap::new();

        // WHEN a ContextCompacted arrives (400-char summary replacing 20 messages)
        let event = RuntimeEvent::ContextCompacted {
            summary_chars: 400,
            original_messages: 20,
        };
        let updates = process_event(
            &event,
            &store,
            &mut in_flight,
            MetricsBudget {
                thresholds: SessionThresholds::default(),
                context_window_max: 10_000,
                token_budget: 0,
            },
        );

        // THEN a SummarizationEvent was pushed and the context shrank
        assert_eq!(updates.len(), 1);
        let (_, m, _) = &updates[0];
        assert_eq!(m.summarization_events.len(), 1);
        assert_eq!(m.summarization_events[0].messages_summarized_count, 20);
        assert!(m.summarization_events[0].tokens_saved > 0);
        assert!(m.context_window_used < 8_000);
    }

    #[test]
    fn meta_llm_budget_exceeded_forces_block() {
        let store = store();
        let mut in_flight = HashMap::new();
        let event = RuntimeEvent::MetaLlmBudgetExceeded {
            session_id: "meta-sess".into(),
            tokens_used: 12_000,
            budget: 10_000,
        };
        let updates = process_event(
            &event,
            &store,
            &mut in_flight,
            MetricsBudget {
                thresholds: SessionThresholds::default(),
                context_window_max: 0,
                token_budget: 0,
            },
        );
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].2, BudgetAlertLevel::Block);
        assert_eq!(updates[0].1.tokens_meta, 12_000);
        assert_eq!(updates[0].1.token_budget, 10_000);
    }

    #[tokio::test]
    async fn actor_aggregates_multi_tool_scenario_and_emits_alert() {
        // GIVEN an actor spawned with a low budget to trigger a warning
        let (tx, rx) = EventBus::new();
        let mut consumer = tx.subscribe();
        let actor =
            SessionMetricsActor::spawn(rx, tx.clone(), SessionThresholds::default(), 10_000, 1_000);

        // WHEN sending a multi-tool plus LLM scenario
        tx.send(RuntimeEvent::ChatToolCallStarted {
            session_id: "S".into(),
            message_id: "m1".into(),
            tool_name: "file_read".into(),
            input_preview: "".into(),
            rationale: None,
        })
        .unwrap();
        tx.send(RuntimeEvent::ChatToolCallCompleted {
            session_id: "S".into(),
            message_id: "m1".into(),
            tool_name: "file_read".into(),
            success: true,
            output_preview: None,
            analysis: None,
        })
        .unwrap();
        tx.send(RuntimeEvent::LlmCallCompleted {
            backend: "b".into(),
            model: "m".into(),
            task_id: Some("S".into()),
            step_id: None,
            prompt_tokens: 900,
            completion_tokens: 0,
            latency_ms: 50,
            cost_usd: None,
            run_id: None,
        })
        .unwrap();

        // THEN at least one SessionMetricsUpdated is observed with a Warning or Block alert
        let mut saw_alert = false;
        let timeout = tokio::time::Duration::from_millis(500);
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, consumer.recv()).await {
                Ok(Ok(RuntimeEvent::SessionMetricsUpdated { alert, .. })) => {
                    if matches!(alert, BudgetAlertLevel::Warning | BudgetAlertLevel::Block) {
                        saw_alert = true;
                        break;
                    }
                }
                Ok(Ok(_)) => continue,
                _ => break,
            }
        }
        assert!(saw_alert, "expected Warning or Block alert");

        actor.abort();
    }

    #[test]
    fn poisoned_mutex_does_not_panic() {
        // GIVEN a store whose mutex was poisoned by a thread that panicked while holding the lock
        let store: SessionMetricsStore = Arc::new(Mutex::new(HashMap::new()));
        let store_clone = Arc::clone(&store);
        let _ = std::panic::catch_unwind(move || {
            let _guard = store_clone.lock().unwrap();
            panic!("deliberate panic to poison the mutex");
        });
        assert!(
            store.is_poisoned(),
            "mutex should be poisoned after the thread panic"
        );

        // WHEN process_event is called on the poisoned store
        let event = RuntimeEvent::LlmCallCompleted {
            backend: "b".into(),
            model: "m".into(),
            task_id: Some("poison-sess".into()),
            step_id: None,
            prompt_tokens: 10,
            completion_tokens: 5,
            latency_ms: 1,
            cost_usd: None,
            run_id: None,
        };
        let mut in_flight = HashMap::new();
        // THEN no panic, process_event completes normally
        let updates = process_event(
            &event,
            &store,
            &mut in_flight,
            MetricsBudget {
                thresholds: SessionThresholds::default(),
                context_window_max: 0,
                token_budget: 0,
            },
        );
        assert_eq!(
            updates.len(),
            1,
            "update should succeed even on a poisoned mutex"
        );
    }
}
