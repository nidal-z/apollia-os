//! `CronTrigger` — source de déclenchement basée sur une expression cron.
//!
//! Spawne une tâche Tokio indépendante qui calcule le prochain instant de
//! déclenchement via la crate `cron` v0.12 et dort jusqu'à ce moment.

use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig};

/// Source de déclenchement basée sur une expression cron.
pub struct CronTrigger;

impl CronTrigger {
    /// Spawne une tâche Tokio qui fire au prochain instant calculé par l'expression cron.
    ///
    /// La tâche boucle indéfiniment et envoie un [`TriggerEvent`] à chaque fire.
    /// Si le channel est fermé, la tâche se termine proprement.
    /// Retourne le `JoinHandle<()>` pour abort lors du hot reload.
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Guard : extraire le schedule depuis la source
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
                        error = %e,
                        "invalid cron schedule, source will not fire"
                    );
                    return;
                }
            };

            while let Some(next) = schedule.upcoming(Utc).next() {
                // Si le prochain fire est dans le passé, délai minimal de 100ms
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
                    // Engine dropped — arrêt propre de la source
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
            pipeline: None,
            enabled: true,
            on_busy: OnBusyPolicy::Queue,
            source: TriggerSourceConfig::Cron {
                schedule: schedule.into(),
            },
            input_template: InputTemplate("{{fired_at}}".into()),
        }
    }

    #[tokio::test]
    async fn test_ac1_cron_fires_within_timeout() {
        // GIVEN — expression every-second (6 champs avec secondes)
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = make_def_cron("test-cron", "* * * * * *");

        // WHEN
        let _handle = CronTrigger::spawn(def, tx);

        // THEN — au moins 1 événement reçu dans les 3 secondes
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
