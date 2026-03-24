//! `IntervalTrigger` — source de déclenchement périodique à intervalle fixe.
//!
//! Spawne une tâche Tokio qui dort `every` puis envoie un [`TriggerEvent`] en boucle.

use chrono::Utc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{
    parse_interval, TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig,
};

/// Source de déclenchement périodique à intervalle fixe.
pub struct IntervalTrigger;

impl IntervalTrigger {
    /// Spawne une tâche Tokio qui fire à chaque intervalle configuré.
    ///
    /// L'intervalle est parsé depuis la chaîne `every` via [`parse_interval`].
    /// Si le format est invalide, la tâche log l'erreur et se termine sans paniquer.
    /// Retourne le `JoinHandle<()>` pour abort lors du hot reload.
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Guard : extraire la durée depuis la source
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

    // ── AC-2 ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac2_interval_fires_multiple_times() {
        // GIVEN — intervalle de 100ms
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = TriggerDefinition {
            id: "interval-test".into(),
            agent: "agent".into(),
            pipeline: None,
            enabled: true,
            on_busy: OnBusyPolicy::Queue,
            source: TriggerSourceConfig::Interval {
                every: "100ms".into(),
            },
            input_template: InputTemplate("tick".into()),
        };

        // WHEN — laisser passer 350ms
        let _handle = IntervalTrigger::spawn(def, tx);
        tokio::time::sleep(Duration::from_millis(350)).await;

        // THEN — au moins 2 fires (3 ± 1 attendus)
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert!(count >= 2, "expected >= 2 fires, got {count}");
    }
}
