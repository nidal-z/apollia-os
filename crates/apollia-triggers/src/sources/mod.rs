//! Trigger sources for the `TriggerEngine`.
//!
//! Each source spawns an independent Tokio task (no shared state) and sends
//! [`crate::TriggerEvent`]s on the engine's internal channel.
//!
//! | Source       | Implemented    |
//! |--------------|----------------|
//! | `Cron`       | yes            |
//! | `Interval`   | yes            |
//! | `Oneshot`    | yes            |
//! | `FileWatch`  | yes            |
//! | `Webhook`    | axum route     |

pub mod cron;
pub mod file_watch;
pub mod interval;
pub mod oneshot;

pub use cron::CronTrigger;
pub use file_watch::FileWatchTrigger;
pub use interval::IntervalTrigger;
pub use oneshot::OneshotTrigger;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{TriggerDefinition, TriggerEvent, TriggerSourceConfig};

/// Spawns the appropriate source based on the definition's [`TriggerSourceConfig`].
///
/// - [`TriggerSourceConfig::Cron`]      -> [`CronTrigger::spawn`]
/// - [`TriggerSourceConfig::Interval`]  -> [`IntervalTrigger::spawn`]
/// - [`TriggerSourceConfig::Oneshot`]   -> [`OneshotTrigger::spawn`]
/// - [`TriggerSourceConfig::FileWatch`] -> [`FileWatchTrigger::spawn`]
/// - [`TriggerSourceConfig::Webhook`]   -> no autonomous spawn (axum route)
///
/// Returns a `JoinHandle<()>` in every case, allowing uniform abort during hot
/// reload.
pub fn spawn_source(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
    match &def.source {
        TriggerSourceConfig::Cron { .. } => CronTrigger::spawn(def, tx),
        TriggerSourceConfig::Interval { .. } => IntervalTrigger::spawn(def, tx),
        TriggerSourceConfig::Oneshot { .. } => OneshotTrigger::spawn(def, tx),
        TriggerSourceConfig::FileWatch { .. } => FileWatchTrigger::spawn(def, tx),
        TriggerSourceConfig::Webhook { .. } => {
            // No autonomous spawn; the axum route handles the event.
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

    // --- parse_interval --------------------------------------------------

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

    // --- spawn_source dispatch -------------------------------------------

    #[tokio::test]
    async fn test_spawn_source_webhook_no_panic() {
        // GIVEN a Webhook source (no autonomous spawn)
        let (tx, _rx) = mpsc::channel(4);
        let def = TriggerDefinition {
            id: "wh".into(),
            agent: "a".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Skip,
            source: TriggerSourceConfig::Webhook {
                secret: "s3cr3t".into(),
            },
            input_template: InputTemplate("{{body}}".into()),
        };
        // WHEN / THEN does not panic, returns a handle that finishes quickly
        let handle = spawn_source(def, tx);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(handle.is_finished());
    }
}
