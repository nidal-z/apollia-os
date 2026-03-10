//! `OneshotTrigger` — source de déclenchement unique à une date/heure précise.
//!
//! Spawne une tâche Tokio qui dort jusqu'à `fire_at`, envoie un [`TriggerEvent`]
//! exactement une fois, puis se termine.

use chrono::Utc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig};

/// Source de déclenchement unique : fire exactement une fois à `fire_at`.
pub struct OneshotTrigger;

impl OneshotTrigger {
    /// Spawne une tâche Tokio qui fire une seule fois à l'heure configurée.
    ///
    /// Si `fire_at` est dans le passé, la tâche fire immédiatement (délai nul).
    /// La tâche se termine définitivement après l'envoi de l'événement.
    /// Retourne le `JoinHandle<()>` pour abort lors du hot reload (STORY-073).
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Guard : extraire fire_at depuis la source
            let fire_at = match &def.source {
                TriggerSourceConfig::Oneshot { fire_at } => *fire_at,
                _ => {
                    tracing::error!(
                        trigger = %def.id,
                        "OneshotTrigger::spawn called with non-Oneshot source"
                    );
                    return;
                }
            };

            // Délai jusqu'au fire_at ; si dans le passé, ZERO (fire immédiat)
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

            // fire-and-forget : si le channel est fermé, on ignore silencieusement
            tx.send(event).await.ok();
            // Termine ici — oneshot, une seule exécution
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputTemplate, OnBusyPolicy, TriggerSourceConfig};
    use std::time::Duration;

    // ── AC-3 ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac3_oneshot_fires_exactly_once() {
        // GIVEN — fire dans 100ms
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let fire_at = Utc::now() + chrono::Duration::milliseconds(100);
        let def = TriggerDefinition {
            id: "oneshot".into(),
            agent: "agent".into(),
            pipeline: None,
            enabled: true,
            on_busy: OnBusyPolicy::Queue,
            source: TriggerSourceConfig::Oneshot { fire_at },
            input_template: InputTemplate("once".into()),
        };
        let handle = OneshotTrigger::spawn(def, tx);

        // WHEN — laisser passer 300ms
        tokio::time::sleep(Duration::from_millis(300)).await;

        // THEN — exactement 1 événement, tâche terminée
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1, "oneshot must fire exactly once");
        assert!(handle.is_finished(), "JoinHandle must be finished");
    }

    #[tokio::test]
    async fn test_oneshot_past_fires_immediately() {
        // GIVEN — fire_at dans le passé
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let fire_at = Utc::now() - chrono::Duration::seconds(10);
        let def = TriggerDefinition {
            id: "past-oneshot".into(),
            agent: "agent".into(),
            pipeline: None,
            enabled: true,
            on_busy: OnBusyPolicy::Queue,
            source: TriggerSourceConfig::Oneshot { fire_at },
            input_template: InputTemplate("past".into()),
        };
        let handle = OneshotTrigger::spawn(def, tx);

        // WHEN — court délai pour laisser la tâche s'exécuter
        tokio::time::sleep(Duration::from_millis(50)).await;

        // THEN — 1 événement reçu, tâche terminée
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1, "past oneshot must fire exactly once");
        assert!(handle.is_finished());
    }
}
