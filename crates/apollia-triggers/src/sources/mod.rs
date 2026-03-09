//! Sources de déclenchement du `TriggerEngine`.
//!
//! Chaque source spawne une tâche Tokio indépendante (pas d'état partagé)
//! et envoie des [`crate::TriggerEvent`] sur le channel interne du moteur.
//!
//! | Source       | Implémentée | Story    |
//! |--------------|-------------|----------|
//! | `Cron`       | ✅          | STORY-067 |
//! | `Interval`   | ✅          | STORY-067 |
//! | `Oneshot`    | ✅          | STORY-067 |
//! | `FileWatch`  | stub        | STORY-068 |
//! | `Webhook`    | route axum  | STORY-069 |

pub mod cron;
pub mod interval;
pub mod oneshot;

pub use cron::CronTrigger;
pub use interval::IntervalTrigger;
pub use oneshot::OneshotTrigger;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{TriggerDefinition, TriggerEvent, TriggerSourceConfig};

/// Spawne la source appropriée selon le [`TriggerSourceConfig`] de la définition.
///
/// - [`TriggerSourceConfig::Cron`]      → [`CronTrigger::spawn`]
/// - [`TriggerSourceConfig::Interval`]  → [`IntervalTrigger::spawn`]
/// - [`TriggerSourceConfig::Oneshot`]   → [`OneshotTrigger::spawn`]
/// - [`TriggerSourceConfig::FileWatch`] → stub no-op (implémenté STORY-068)
/// - [`TriggerSourceConfig::Webhook`]   → pas de spawn autonome (route axum, STORY-069)
///
/// Retourne un `JoinHandle<()>` dans tous les cas, permettant un abort uniforme
/// lors du hot reload (STORY-073).
pub fn spawn_source(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
    match &def.source {
        TriggerSourceConfig::Cron { .. } => CronTrigger::spawn(def, tx),
        TriggerSourceConfig::Interval { .. } => IntervalTrigger::spawn(def, tx),
        TriggerSourceConfig::Oneshot { .. } => OneshotTrigger::spawn(def, tx),
        TriggerSourceConfig::FileWatch { .. } => {
            // Stub — implémenté STORY-068
            tracing::debug!(trigger = %def.id, "FileWatchTrigger stub (STORY-068)");
            tokio::spawn(async {})
        }
        TriggerSourceConfig::Webhook { .. } => {
            // Pas de spawn autonome — la route axum gère l'événement (STORY-069)
            tracing::debug!(trigger = %def.id, "Webhook source: no autonomous task");
            tokio::spawn(async {})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{parse_interval, InputTemplate, OnBusyPolicy, TriggerDefinitionError};
    use std::time::Duration;

    // ── AC-5 : parse_interval ──────────────────────────────────────────────

    #[test]
    fn test_ac5_parse_interval_valid_formats() {
        // GIVEN / WHEN / THEN
        assert_eq!(parse_interval("30m").unwrap(), Duration::from_secs(1_800));
        assert_eq!(parse_interval("1h").unwrap(), Duration::from_secs(3_600));
        assert_eq!(parse_interval("6h").unwrap(), Duration::from_secs(21_600));
        assert_eq!(parse_interval("1d").unwrap(), Duration::from_secs(86_400));
    }

    #[test]
    fn test_ac5_parse_interval_invalid_format() {
        // GIVEN / WHEN / THEN
        assert!(matches!(
            parse_interval("2w"),
            Err(TriggerDefinitionError::InvalidInterval { .. })
        ));
        assert!(matches!(
            parse_interval("abc"),
            Err(TriggerDefinitionError::InvalidInterval { .. })
        ));
        assert!(matches!(
            parse_interval(""),
            Err(TriggerDefinitionError::InvalidInterval { .. })
        ));
    }

    // ── spawn_source dispatch ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_spawn_source_webhook_no_panic() {
        // GIVEN — source Webhook (pas de spawn autonome)
        let (tx, _rx) = mpsc::channel(4);
        let def = TriggerDefinition {
            id: "wh".into(),
            agent: "a".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Drop,
            source: TriggerSourceConfig::Webhook {
                secret: "s3cr3t".into(),
            },
            input_template: InputTemplate("{{body}}".into()),
        };
        // WHEN / THEN — ne panique pas, retourne un handle terminé rapidement
        let handle = spawn_source(def, tx);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(handle.is_finished());
    }
}
