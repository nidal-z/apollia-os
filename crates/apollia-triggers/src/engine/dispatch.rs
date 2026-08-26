//! Trigger-event dispatch and the per-agent queue drain.
//!
//! Split out of `engine.rs`: the actor loop stays in the parent, the path that
//! turns a fired trigger into a submitted task, and the one that drains what
//! a busy agent queued, live here.

use std::time::Instant;

use chrono::Utc;

use apollia_core::{AIPInput, AIPPart, RuntimeEvent, TaskId, TextPart};

use crate::engine::{AgentQueue, QueuedTriggerEvent, TriggerEngine, TriggerEngineError};
use crate::types::{OnBusyPolicy, TriggerEvent};

impl TriggerEngine {
    /// Handles a `TriggerEvent`: delegates to `process_event` and ignores the result.
    pub(super) async fn handle_event(&mut self, event: TriggerEvent) {
        let _ = self.process_event(event).await;
    }
    /// Full event processing: policy evaluation, submission, `RuntimeEvent`
    /// emission, and persistence.
    ///
    /// Returns `Ok(task_id)` if a task was submitted, `Err` otherwise.
    pub(super) async fn process_event(
        &mut self,
        event: TriggerEvent,
    ) -> Result<TaskId, TriggerEngineError> {
        // 1. Find the definition.
        let def = match self
            .definitions
            .iter()
            .find(|d| d.id == event.trigger_id)
            .cloned()
        {
            Some(d) => d,
            None => {
                tracing::warn!(
                    trigger_id = %event.trigger_id,
                    "trigger.event.unknown"
                );
                return Err(TriggerEngineError::NotFound {
                    id: event.trigger_id.clone(),
                });
            }
        };

        // Skip if disabled.
        if !def.enabled {
            let reason = "trigger disabled".to_string();
            tracing::debug!(trigger_id = %event.trigger_id, %reason, "trigger.skipped");
            self.persist_skipped(&event, &reason).await;
            *self
                .skip_counts
                .entry(event.trigger_id.clone())
                .or_insert(0) += 1;
            return Err(TriggerEngineError::SubmitFailed(reason));
        }

        // Evaluate the OnBusyPolicy before submitting the task.
        match &def.on_busy {
            OnBusyPolicy::Skip => {
                let pending = self.task_router.pending_count(&def.agent).await;
                if pending > 0 {
                    let reason = "agent busy, on_busy=skip".to_string();
                    let _ = self.event_bus.send(RuntimeEvent::TriggerSkipped {
                        trigger_id: event.trigger_id.clone(),
                        reason: reason.clone(),
                    });
                    self.persist_skipped(&event, &reason).await;
                    *self
                        .skip_counts
                        .entry(event.trigger_id.clone())
                        .or_insert(0) += 1;
                    return Err(TriggerEngineError::SubmitFailed(reason));
                }
            }
            OnBusyPolicy::Queue { max_depth } => {
                let max_depth = *max_depth;
                let pending = self.task_router.pending_count(&def.agent).await;
                if pending > 0 {
                    let queued = QueuedTriggerEvent {
                        trigger_id: event.trigger_id.clone(),
                        payload: event.payload.clone(),
                        queued_at: Utc::now(),
                    };
                    let queue = self
                        .agent_queues
                        .entry(def.agent.clone())
                        .or_insert_with(|| AgentQueue::new(max_depth));
                    if queue.try_push(queued) {
                        tracing::info!(
                            trigger_id = %event.trigger_id,
                            agent = %def.agent,
                            queue_depth = queue.len(),
                            "trigger.queued"
                        );
                        return Err(TriggerEngineError::SubmitFailed(
                            "trigger queued for dispatch".into(),
                        ));
                    } else {
                        let _ = self.event_bus.send(RuntimeEvent::TriggerQueueFull {
                            trigger_id: event.trigger_id.clone(),
                        });
                        tracing::warn!(
                            trigger_id = %event.trigger_id,
                            agent = %def.agent,
                            max_depth,
                            reason = "the agent queue is full",
                            "trigger.dropped"
                        );
                        return Err(TriggerEngineError::SubmitFailed(
                            "trigger queue full".into(),
                        ));
                    }
                }
                // Agent free: drain the existing queue (FIFO) before submitting.
                self.drain_agent_queue(&def.agent).await;
            }
            OnBusyPolicy::Block => {
                // Submit directly; async blocking is not implemented.
            }
        }

        let text = def.input_template.render(&event.payload);
        let input = AIPInput {
            parts: vec![AIPPart::Text(TextPart { text })],
        };

        // 4. Submit the task and measure dispatch_ms.
        let dispatch_start = Instant::now();
        match self.task_router.submit(&def.agent, input).await {
            Ok(task_id) => {
                let dispatch_ms = dispatch_start.elapsed().as_millis() as i64;
                let _ = self.event_bus.send(RuntimeEvent::TriggerFired {
                    trigger_id: event.trigger_id.clone(),
                    agent: def.agent.clone(),
                    task_id: task_id.clone(),
                });
                self.persist_fired(&event, &task_id, dispatch_ms).await;
                *self
                    .fire_counts
                    .entry(event.trigger_id.clone())
                    .or_insert(0) += 1;
                self.last_fired.insert(event.trigger_id.clone(), Utc::now());
                Ok(task_id)
            }
            Err(e) => {
                let _ = self.event_bus.send(RuntimeEvent::TriggerError {
                    trigger_id: event.trigger_id.clone(),
                    error: e.clone(),
                });
                self.persist_error(&event, &e).await;
                tracing::error!(
                    trigger_id = %event.trigger_id,
                    error = %e,
                    "trigger.task.submit.failed"
                );
                Err(TriggerEngineError::SubmitFailed(e))
            }
        }
    }
    /// Drains an agent's queue and submits each trigger in FIFO order.
    ///
    /// Called either when the agent is detected free on a new trigger, or
    /// explicitly via [`TriggerCommand::NotifyAgentFree`].
    /// Failed submissions are logged without interrupting the drain.
    pub(super) async fn drain_agent_queue(&mut self, agent_id: &str) {
        let mut items = Vec::new();
        if let Some(queue) = self.agent_queues.get_mut(agent_id) {
            while let Some(item) = queue.pop() {
                items.push(item);
            }
        }
        if items.is_empty() {
            return;
        }
        tracing::debug!(
            agent = %agent_id,
            count = items.len(),
            "trigger.queue.drain.started"
        );
        for queued in items {
            let def = self
                .definitions
                .iter()
                .find(|d| d.id == queued.trigger_id)
                .cloned();
            let Some(def) = def else {
                tracing::warn!(
                    trigger_id = %queued.trigger_id,
                    detail = "the queued trigger is ignored",
                    "trigger.drain.definition.missing"
                );
                continue;
            };
            let text = def.input_template.render(&queued.payload);
            let input = AIPInput {
                parts: vec![AIPPart::Text(TextPart { text })],
            };
            let dispatch_start = Instant::now();
            match self.task_router.submit(&def.agent, input).await {
                Ok(task_id) => {
                    let dispatch_ms = dispatch_start.elapsed().as_millis() as i64;
                    let _ = self.event_bus.send(RuntimeEvent::TriggerFired {
                        trigger_id: queued.trigger_id.clone(),
                        agent: def.agent.clone(),
                        task_id: task_id.clone(),
                    });
                    let event = TriggerEvent {
                        trigger_id: queued.trigger_id.clone(),
                        agent: def.agent.clone(),
                        payload: queued.payload,
                        fired_at: queued.queued_at,
                    };
                    self.persist_fired(&event, &task_id, dispatch_ms).await;
                    *self
                        .fire_counts
                        .entry(queued.trigger_id.clone())
                        .or_insert(0) += 1;
                    self.last_fired
                        .insert(queued.trigger_id.clone(), Utc::now());
                }
                Err(e) => {
                    tracing::warn!(
                        trigger_id = %queued.trigger_id,
                        error = %e,
                        detail = "the queued trigger is lost",
                        "trigger.drain.submit.failed"
                    );
                    let _ = self.event_bus.send(RuntimeEvent::TriggerError {
                        trigger_id: queued.trigger_id.clone(),
                        error: e,
                    });
                }
            }
        }
    }
}
