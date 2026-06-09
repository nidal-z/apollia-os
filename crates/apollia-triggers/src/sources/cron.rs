//! `CronTrigger`: trigger source based on a cron expression.
//!
//! Spawns an independent Tokio task that computes the next fire instant via the
//! `cron` crate v0.12 and sleeps until that moment.

use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig};

/// Trigger source based on a cron expression.
pub struct CronTrigger;

impl CronTrigger {
    /// Spawns a Tokio task that fires at the next instant computed by the cron expression.
    ///
    /// The task loops indefinitely and sends a [`TriggerEvent`] on each fire.
    /// If the channel is closed, the task terminates cleanly.
    /// Returns the `JoinHandle<()>` for abort during hot reload.
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Extract the schedule from the source.
            let schedule_str = match &def.source {
                TriggerSourceConfig::Cron { schedule } => schedule.clone(),
                _ => {
                    tracing::error!(
                        trigger = %def.id,
                        "CronTrigger::spawn called with non-Cron source"
                    );
                    return;
                }
            };

            let schedule = match Schedule::from_str(&schedule_str) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(
                        trigger = %def.id,
                        schedule = %schedule_str,
                        error = %e,
                        "invalid cron schedule, source will not fire \
                         (fix it with `apollia-os trigger update {} --detail \"<expr>\"`)",
                        def.id
                    );
                    return;
                }
            };

            while let Some(next) = schedule.upcoming(Utc).next() {
                // If the next fire is in the past, use a minimal 100ms delay.
                let duration = (next - Utc::now()).to_std().unwrap_or_else(|_| {
                    tracing::warn!(
                        trigger = %def.id,
                        "next cron fire is in the past, delaying 100ms"
                    );
                    std::time::Duration::from_millis(100)
                });

                tokio::time::sleep(duration).await;

                let fired_at = Utc::now();
                let event = TriggerEvent {
                    trigger_id: def.id.clone(),
                    agent: def.agent.clone(),
                    payload: TriggerPayload::Timer {
                        scheduled_at: next,
                        fired_at,
                    },
                    fired_at,
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
    use tokio::time::timeout;

    fn make_def_cron(id: &str, schedule: &str) -> TriggerDefinition {
        TriggerDefinition {
            id: id.into(),
            agent: "test-agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::Cron {
                schedule: schedule.into(),
            },
            input_template: InputTemplate("{{fired_at}}".into()),
        }
    }

    #[tokio::test]
    async fn test_cron_fires_within_timeout() {
        // GIVEN an every-second expression (6 fields including seconds)
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = make_def_cron("test-cron", "* * * * * *");

        // WHEN
        let _handle = CronTrigger::spawn(def, tx);

        // THEN at least 1 event received within 3 seconds
        let event = timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(event.is_ok(), "no event received within 3s");
        let event = event.unwrap().unwrap();
        assert_eq!(event.trigger_id, "test-cron");
        assert!(
            matches!(event.payload, TriggerPayload::Timer { .. }),
            "expected Timer payload"
        );
    }
}
