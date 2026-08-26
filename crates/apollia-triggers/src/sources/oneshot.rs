//! `OneshotTrigger`: single trigger source at a precise date/time.
//!
//! Spawns a Tokio task that sleeps until `fire_at`, sends a [`TriggerEvent`]
//! exactly once, then terminates.

use chrono::Utc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig};

/// Single trigger source: fires exactly once at `fire_at`.
pub struct OneshotTrigger;

impl OneshotTrigger {
    /// Spawns a Tokio task that fires once at the configured time.
    ///
    /// If `fire_at` is in the past, the task fires immediately (zero delay).
    /// The task terminates for good after sending the event.
    /// Returns the `JoinHandle<()>` for abort during hot reload.
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Extract fire_at from the source.
            let fire_at = match &def.source {
                TriggerSourceConfig::Oneshot { fire_at } => *fire_at,
                _ => {
                    tracing::error!(
                        trigger = %def.id,
                        "trigger.oneshot.source.mismatch"
                    );
                    return;
                }
            };

            // Delay until fire_at; if in the past, ZERO (fire immediately).
            let wait = (fire_at - Utc::now())
                .to_std()
                .unwrap_or(std::time::Duration::ZERO);

            tokio::time::sleep(wait).await;

            let fired_at = Utc::now();
            let event = TriggerEvent {
                trigger_id: def.id.clone(),
                agent: def.agent.clone(),
                payload: TriggerPayload::Timer {
                    scheduled_at: fire_at,
                    fired_at,
                },
                fired_at,
            };

            // Fire-and-forget: if the channel is closed, ignore silently.
            tx.send(event).await.ok();
            // Ends here: oneshot, a single execution.
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputTemplate, OnBusyPolicy, TriggerSourceConfig};
    use std::time::Duration;

    #[tokio::test]
    async fn test_oneshot_fires_exactly_once() {
        // GIVEN fire in 100ms
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let fire_at = Utc::now() + chrono::Duration::milliseconds(100);
        let def = TriggerDefinition {
            id: "oneshot".into(),
            agent: "agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::Oneshot { fire_at },
            input_template: InputTemplate("once".into()),
        };
        let handle = OneshotTrigger::spawn(def, tx);

        // WHEN letting 300ms pass
        tokio::time::sleep(Duration::from_millis(300)).await;

        // THEN exactly 1 event, task finished
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1, "oneshot must fire exactly once");
        assert!(handle.is_finished(), "JoinHandle must be finished");
    }

    #[tokio::test]
    async fn test_oneshot_past_fires_immediately() {
        // GIVEN fire_at in the past
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let fire_at = Utc::now() - chrono::Duration::seconds(10);
        let def = TriggerDefinition {
            id: "past-oneshot".into(),
            agent: "agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::Oneshot { fire_at },
            input_template: InputTemplate("past".into()),
        };
        let handle = OneshotTrigger::spawn(def, tx);

        // WHEN short delay to let the task run
        tokio::time::sleep(Duration::from_millis(50)).await;

        // THEN 1 event received, task finished
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1, "past oneshot must fire exactly once");
        assert!(handle.is_finished());
    }
}
