//! `IntervalTrigger`: periodic trigger source at a fixed interval.
//!
//! Spawns a Tokio task that sleeps `every`, then sends a [`TriggerEvent`] in a loop.

use chrono::Utc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{
    parse_interval, TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig,
};

/// Periodic trigger source at a fixed interval.
pub struct IntervalTrigger;

impl IntervalTrigger {
    /// Spawns a Tokio task that fires at each configured interval.
    ///
    /// The interval is parsed from the `every` string via [`parse_interval`].
    /// If the format is invalid, the task logs the error and terminates without panicking.
    /// Returns the `JoinHandle<()>` for abort during hot reload.
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Extract the duration from the source.
            let every_str = match &def.source {
                TriggerSourceConfig::Interval { every } => every.clone(),
                _ => {
                    tracing::error!(
                        trigger = %def.id,
                        "IntervalTrigger::spawn called with non-Interval source"
                    );
                    return;
                }
            };

            let duration = match parse_interval(&every_str) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(
                        trigger = %def.id,
                        error = %e,
                        "invalid interval format, source will not fire"
                    );
                    return;
                }
            };

            loop {
                tokio::time::sleep(duration).await;

                let now = Utc::now();
                let event = TriggerEvent {
                    trigger_id: def.id.clone(),
                    agent: def.agent.clone(),
                    payload: TriggerPayload::Timer {
                        scheduled_at: now,
                        fired_at: now,
                    },
                    fired_at: now,
                };

                if tx.send(event).await.is_err() {
                    // Engine dropped; shut the source down cleanly.
                    break;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputTemplate, OnBusyPolicy, TriggerSourceConfig};
    use std::time::Duration;

    #[tokio::test]
    async fn test_ac2_interval_fires_multiple_times() {
        // GIVEN a 100ms interval
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = TriggerDefinition {
            id: "interval-test".into(),
            agent: "agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::Interval {
                every: "100ms".into(),
            },
            input_template: InputTemplate("tick".into()),
        };

        // WHEN letting 350ms pass
        let _handle = IntervalTrigger::spawn(def, tx);
        tokio::time::sleep(Duration::from_millis(350)).await;

        // THEN at least 2 fires (3 plus or minus 1 expected)
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert!(count >= 2, "expected >= 2 fires, got {count}");
    }
}
