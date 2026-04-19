//! Acteur d'agrégation des métriques de session (US-SP42-047 Pattern P11).
//!
//! `SessionMetricsActor` souscrit à l'`EventBus` et maintient une map
//! `session_id -> SessionMetrics`. Il réagit aux événements suivants :
//!
//! - [`RuntimeEvent::LlmCallCompleted`] → incrémente tokens (non-meta).
//! - [`RuntimeEvent::ChatToolCallStarted`] / [`RuntimeEvent::ChatToolCallCompleted`]
//!   → mesure la latence côté actor (pas de `duration_ms` dans l'événement).
//! - [`RuntimeEvent::ContextCompacted`] → enregistre un `SummarizationEvent`.
//! - [`RuntimeEvent::MetaLlmBudgetExceeded`] → flagge l'alerte bloquante et
//!   ré-émet `SessionMetricsUpdated`.
//!
//! À chaque mise à jour, émet [`RuntimeEvent::SessionMetricsUpdated`]
//! avec le snapshot complet et le niveau d'alerte courant. Le frontend
//! consomme cet événement pour rafraîchir `SessionMetricsPanel`.

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

/// Snapshot partagé permettant à l'API/commands d'exposer l'état courant sans
/// ré-abonner un nouveau receiver.
///
/// Mis à jour à chaque événement consommé par l'actor. Cloné via `Arc`.
pub type SessionMetricsStore = Arc<Mutex<HashMap<String, SessionMetrics>>>;

/// Tool call en cours — sert à mesurer la latence côté actor.
struct InFlightToolCall {
    session_id: String,
    tool_name: String,
    started_at: Instant,
}

/// Handle de l'acteur : détient le store partagé et le handle de la task Tokio.
pub struct SessionMetricsActor {
    store: SessionMetricsStore,
    handle: JoinHandle<()>,
}

impl SessionMetricsActor {
    /// Démarre l'acteur. Retourne un `SessionMetricsActor` clonable via `store()`.
    ///
    /// L'acteur s'arrête quand l'`EventBus` se ferme.
    pub fn spawn(
        mut rx: EventBusReceiver,
        bus: EventBusSender,
        thresholds: SessionThresholds,
        context_window_max: u64,
        token_budget: u64,
    ) -> Self {
        let store: SessionMetricsStore = Arc::new(Mutex::new(HashMap::new()));
        let store_task = Arc::clone(&store);

        let handle = tokio::spawn(async move {
            // Map tool_call_id (message_id) -> in-flight pour calcul de durée.
            let mut in_flight: HashMap<String, InFlightToolCall> = HashMap::new();

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let updates = process_event(
                            &event,
                            &store_task,
                            &mut in_flight,
                            thresholds,
                            context_window_max,
                            token_budget,
                        );
                        for (session_id, metrics, alert) in updates {
                            let _ = bus.send(RuntimeEvent::SessionMetricsUpdated {
                                session_id,
                                metrics,
                                alert,
                            });
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "SessionMetricsActor lagged, events dropped"
                        );
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

    /// Accès au store partagé (snapshots par session_id).
    pub fn store(&self) -> SessionMetricsStore {
        Arc::clone(&self.store)
    }

    /// Attend la fin de la task. Utile dans les tests.
    #[cfg(test)]
    pub async fn join(self) {
        let _ = self.handle.await;
    }

    /// Abandonne la task — utilisé au shutdown quand on ne veut pas attendre.
    pub fn abort(self) {
        self.handle.abort();
    }
}

/// Variante detached : spawn l'acteur et ne retourne que le [`SessionMetricsStore`].
///
/// La task Tokio vit tant que l'`EventBus` reste ouvert. Utile dans les intégrations
/// (Tauri desktop) où l'on souhaite juste exposer le store partagé via `app.manage(...)`
/// sans garder de handle explicite.
pub fn spawn_detached(
    rx: EventBusReceiver,
    bus: EventBusSender,
    thresholds: SessionThresholds,
    context_window_max: u64,
    token_budget: u64,
) -> SessionMetricsStore {
    let actor = SessionMetricsActor::spawn(rx, bus, thresholds, context_window_max, token_budget);
    let store = actor.store();
    // Dropping `actor` n'annule pas la task Tokio : `JoinHandle` n'a pas de `Drop`
    // qui avorte. La task continue de vivre jusqu'à la fermeture de l'`EventBus`.
    drop(actor);
    store
}

/// Traite un événement et retourne la liste des updates à émettre
/// (session_id, snapshot, niveau d'alerte).
///
/// Isolé dans une fonction pure-ish pour pouvoir tester la logique sans
/// spawner d'acteur Tokio.
fn process_event(
    event: &RuntimeEvent,
    store: &SessionMetricsStore,
    in_flight: &mut HashMap<String, InFlightToolCall>,
    thresholds: SessionThresholds,
    context_window_max: u64,
    token_budget: u64,
) -> Vec<(String, SessionMetrics, BudgetAlertLevel)> {
    match event {
        RuntimeEvent::LlmCallCompleted {
            task_id,
            prompt_tokens,
            completion_tokens,
            ..
        } => {
            // Pas de session_id dans l'événement : on utilise task_id comme clé
            // par défaut. Si task_id est absent, on retombe sur la session globale.
            let session_id = task_id.clone().unwrap_or_else(|| "global".to_string());
            let mut guard = store.lock().expect("SessionMetrics store poisoned");
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
            let expected_ms =
                apollia_llm::tool_performance_hints::lookup(&call.tool_name)
                    .map(|h| h.expected_duration_ms);
            let timing = ToolTiming::new(&call.tool_name, expected_ms, actual_ms);

            let mut guard = store.lock().expect("SessionMetrics store poisoned");
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
            // `ContextCompacted` ne porte pas de session_id — on applique à toutes
            // les sessions actives (cas courant : une session active à la fois).
            let mut guard = store.lock().expect("SessionMetrics store poisoned");
            let mut updates = Vec::new();
            // Estimation grossière : 1 token ≈ 4 caractères.
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
            let mut guard = store.lock().expect("SessionMetrics store poisoned");
            let m = guard
                .entry(session_id.clone())
                .or_insert_with(|| default_metrics(context_window_max, token_budget));
            m.tokens_meta = m.tokens_meta.max(*tokens_used);
            if *budget > 0 {
                m.token_budget = *budget;
            }
            // Force l'alerte Block dès que l'événement arrive.
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
        // GIVEN un store vide et un LlmCallCompleted avec task_id "task-1"
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
        };

        // WHEN
        let updates = process_event(
            &event,
            &store,
            &mut in_flight,
            SessionThresholds::default(),
            200_000,
            10_000,
        );

        // THEN — une mise à jour émise, tokens cumulés dans la session "task-1"
        assert_eq!(updates.len(), 1);
        let (sid, m, alert) = &updates[0];
        assert_eq!(sid, "task-1");
        assert_eq!(m.tokens_in, 100);
        assert_eq!(m.tokens_out, 40);
        assert_eq!(*alert, BudgetAlertLevel::Ok);
    }

    #[test]
    fn tool_call_pair_emits_timing() {
        // GIVEN un ChatToolCallStarted suivi d'un ChatToolCallCompleted
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

        // WHEN — started puis completed
        let u1 = process_event(
            &started,
            &store,
            &mut in_flight,
            SessionThresholds::default(),
            0,
            0,
        );
        assert!(u1.is_empty(), "started ne doit pas émettre de snapshot");
        assert!(in_flight.contains_key("msg-1"));

        let u2 = process_event(
            &completed,
            &store,
            &mut in_flight,
            SessionThresholds::default(),
            0,
            0,
        );

        // THEN — un timing ajouté pour la session
        assert_eq!(u2.len(), 1);
        let (sid, m, _alert) = &u2[0];
        assert_eq!(sid, "sess-1");
        assert_eq!(m.tool_timings.len(), 1);
        assert_eq!(m.tool_timings[0].tool_name, "file_read");
        assert!(!in_flight.contains_key("msg-1"));
    }

    #[test]
    fn completed_without_started_is_a_noop() {
        // GIVEN un completed orphelin (pas de started préalable)
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
            SessionThresholds::default(),
            0,
            0,
        );
        // THEN — aucune update
        assert!(updates.is_empty());
    }

    #[test]
    fn threshold_crossing_reports_warning_then_block() {
        // GIVEN un budget de 1000 et des appels successifs
        let store = store();
        let mut in_flight = HashMap::new();
        let th = SessionThresholds::default();

        let mk = |tokens: u32| RuntimeEvent::LlmCallCompleted {
            backend: "b".into(),
            model: "m".into(),
            task_id: Some("s".into()),
            step_id: None,
            prompt_tokens: tokens,
            completion_tokens: 0,
            latency_ms: 10,
            cost_usd: None,
        };

        // WHEN premier appel sous warning
        let u1 = process_event(&mk(500), &store, &mut in_flight, th, 0, 1000);
        assert_eq!(u1[0].2, BudgetAlertLevel::Ok);

        // AND deuxième appel franchit 80 %
        let u2 = process_event(&mk(350), &store, &mut in_flight, th, 0, 1000);
        // THEN warning
        assert_eq!(u2[0].2, BudgetAlertLevel::Warning);

        // AND troisième appel franchit 100 %
        let u3 = process_event(&mk(200), &store, &mut in_flight, th, 0, 1000);
        // THEN block
        assert_eq!(u3[0].2, BudgetAlertLevel::Block);
    }

    #[test]
    fn context_compacted_pushes_summarization_event() {
        // GIVEN une session avec du contexte
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

        // WHEN un ContextCompacted arrive (résumé de 400 chars remplaçant 20 messages)
        let event = RuntimeEvent::ContextCompacted {
            summary_chars: 400,
            original_messages: 20,
        };
        let updates = process_event(
            &event,
            &store,
            &mut in_flight,
            SessionThresholds::default(),
            10_000,
            0,
        );

        // THEN un SummarizationEvent a été poussé et le contexte réduit
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
            SessionThresholds::default(),
            0,
            0,
        );
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].2, BudgetAlertLevel::Block);
        assert_eq!(updates[0].1.tokens_meta, 12_000);
        assert_eq!(updates[0].1.token_budget, 10_000);
    }

    #[tokio::test]
    async fn actor_aggregates_multi_tool_scenario_and_emits_alert() {
        // GIVEN un acteur spawné avec un budget bas pour déclencher warning
        let (tx, rx) = EventBus::new();
        let mut consumer = tx.subscribe();
        let actor = SessionMetricsActor::spawn(
            rx,
            tx.clone(),
            SessionThresholds::default(),
            10_000,
            1_000,
        );

        // WHEN — on envoie un scénario multi-tools + LLM
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
        })
        .unwrap();

        // THEN — on observe au moins une SessionMetricsUpdated avec alert Warning ou Block
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
}
