//! Throttling and channel dispatch for the notification engine.
//!
//! Split out of `engine.rs`: the loop and its handle stay in the parent, the
//! per-(channel, event) throttle table and every path that writes to a channel
//! or to the notification log live here.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::NotificationConfig;
use crate::engine::{Notification, NotificationChannel};

/// Throttling state for a `(channel, event)` pair.
///
/// Set by [`apply_throttle`] before each dispatch and flushed by
/// [`flush_recaps`] at the end of the window. Everything is local to
/// [`super::run_engine_loop`]: the map is never shared across tasks, which eliminates
/// the Arc<Mutex>.
#[derive(Debug, Default)]
pub(super) struct ThrottleState {
    /// Last actual emission (used to compute the end of the window).
    /// `None` = no emission yet; the first one always goes through.
    pub(super) last_sent_at: Option<Instant>,
    /// Number of notifications dropped since the last emission.
    pub(super) dropped_count: u32,
    /// Sample of the last dropped notification, used for the recap (we keep the
    /// timestamp and metadata of the most recent drop).
    pub(super) recap_sample: Option<Notification>,
}
/// Result of a throttle check for a given channel.
enum ThrottleDecision {
    /// No throttling configured for this channel: send.
    NoThrottle,
    /// Window elapsed or first emission: send and rearm.
    Send,
    /// Still within the window: drop silently, accumulate for the recap.
    Drop,
}
/// Decides what to do for the given `(channel, event)` pair.
///
/// Updates the corresponding throttle entry:
/// - `NoThrottle`: `min_interval_seconds == 0`, so the entry is not created.
/// - `Send`: records `last_sent_at = now` and resets the counter to zero.
/// - `Drop`: increments `dropped_count` and keeps a sample of the notification
///   for the recap.
fn apply_throttle(
    throttle: &mut HashMap<(String, String), ThrottleState>,
    channel_id: &str,
    notif: &Notification,
    min_interval_seconds: u32,
    now: Instant,
) -> ThrottleDecision {
    if min_interval_seconds == 0 {
        return ThrottleDecision::NoThrottle;
    }
    let key = (channel_id.to_string(), notif.event.clone());
    let entry = throttle.entry(key).or_default();
    let window = Duration::from_secs(min_interval_seconds as u64);
    let due = entry
        .last_sent_at
        .map(|t| now.saturating_duration_since(t) >= window)
        .unwrap_or(true);
    if due {
        entry.last_sent_at = Some(now);
        entry.dropped_count = 0;
        entry.recap_sample = None;
        ThrottleDecision::Send
    } else {
        entry.dropped_count = entry.dropped_count.saturating_add(1);
        entry.recap_sample = Some(notif.clone());
        ThrottleDecision::Drop
    }
}
/// Dispatches a notification, applying the per-(channel, event) throttle.
///
/// Wrapper around [`send_to_channel`] that consults the throttle table before
/// each send. For channels without throttling, the behavior is identical to
/// plain dispatch.
pub(super) async fn dispatch_with_throttle(
    config: &NotificationConfig,
    channels: &[Box<dyn NotificationChannel>],
    notif: &Notification,
    throttle: &mut HashMap<(String, String), ThrottleState>,
    now: Instant,
) -> HashMap<String, Option<String>> {
    let mut results = HashMap::new();
    for channel in channels {
        if !channel.accepts(&notif.event, config) {
            continue;
        }
        let min_interval = config
            .channels
            .iter()
            .find(|c| c.id == channel.id())
            .map(|c| c.min_interval_seconds)
            .unwrap_or(0);
        match apply_throttle(throttle, channel.id(), notif, min_interval, now) {
            ThrottleDecision::NoThrottle | ThrottleDecision::Send => {
                send_to_channel(channel.as_ref(), notif, &mut results).await;
            }
            ThrottleDecision::Drop => {
                tracing::debug!(
                    channel_id = channel.id(),
                    event = %notif.event,
                    detail = "the notification joins the next digest",
                    "notification.throttled"
                );
            }
        }
    }
    results
}
/// Sends `notif` to `channel` and records the result in `results`.
///
/// Shared helper between throttled dispatch and recap flush.
async fn send_to_channel(
    channel: &dyn NotificationChannel,
    notif: &Notification,
    results: &mut HashMap<String, Option<String>>,
) {
    match channel.send(notif).await {
        Ok(()) => {
            results.insert(channel.id().to_string(), None);
        }
        Err(err) => {
            tracing::warn!(
                channel_id = channel.id(),
                error = %err,
                event = %notif.event,
                detail = "the dispatch continues on the other channels",
                "notification.channel.failed"
            );
            results.insert(channel.id().to_string(), Some(err.to_string()));
        }
    }
}
/// Emits the recaps for elapsed throttle windows.
///
/// For each `(channel_id, event_name)` entry with `dropped_count > 0` whose
/// window has elapsed, builds a summary notification and dispatches it **only**
/// to the relevant channel. That same emission rearms the entry
/// (`dropped_count = 0`, `last_sent_at = now`).
pub(super) async fn flush_recaps(
    config: &NotificationConfig,
    channels: &[Box<dyn NotificationChannel>],
    throttle: &mut HashMap<(String, String), ThrottleState>,
    log_db_path: Option<&std::path::Path>,
    now: Instant,
) {
    // Collect keys due to flush: a borrow-checker dance to avoid holding a mut
    // borrow on `throttle` across the async send.
    let due_keys: Vec<(String, String, u32, u32, Notification)> = throttle
        .iter()
        .filter_map(|((channel_id, event_name), state)| {
            if state.dropped_count == 0 {
                return None;
            }
            let min_interval = config
                .channels
                .iter()
                .find(|c| &c.id == channel_id)
                .map(|c| c.min_interval_seconds)?;
            if min_interval == 0 {
                return None;
            }
            let last = state.last_sent_at?;
            if now.saturating_duration_since(last) < Duration::from_secs(min_interval as u64) {
                return None;
            }
            let sample = state.recap_sample.clone()?;
            Some((
                channel_id.clone(),
                event_name.clone(),
                state.dropped_count,
                min_interval,
                sample,
            ))
        })
        .collect();

    for (channel_id, event_name, dropped_count, min_interval, sample) in due_keys {
        let Some(channel) = channels.iter().find(|c| c.id() == channel_id) else {
            // Channel disappeared after reload: clear the entry and move on.
            throttle.remove(&(channel_id, event_name));
            continue;
        };

        let recap = build_recap_notification(&sample, dropped_count, min_interval);
        let mut results = HashMap::new();
        send_to_channel(channel.as_ref(), &recap, &mut results).await;

        if let Some(state) = throttle.get_mut(&(channel_id.clone(), event_name.clone())) {
            state.dropped_count = 0;
            state.last_sent_at = Some(now);
            state.recap_sample = None;
        }

        if let Some(db_path) = log_db_path {
            let db_path = db_path.to_path_buf();
            let recap_clone = recap.clone();
            tokio::task::spawn_blocking(move || {
                write_notification_log(&db_path, &recap_clone, &results);
            });
        }
    }
}
/// Builds the summary notification for an elapsed throttle window.
///
/// Reuses the `task_id` / `agent` / `severity` of the last sample, which are
/// representative on a homogeneous aggregate in practice. The `message` is
/// produced backend-side (the UI has human labels per event_name if it wants to
/// re-localize).
fn build_recap_notification(
    sample: &Notification,
    dropped_count: u32,
    window_seconds: u32,
) -> Notification {
    let total = dropped_count.saturating_add(1); // initial drop plus those during the window
    let message = format!(
        "{} '{}' events over the last {} seconds",
        total, sample.event, window_seconds,
    );
    Notification {
        event: sample.event.clone(),
        timestamp: chrono::Utc::now(),
        task_id: sample.task_id.clone(),
        agent: sample.agent.clone(),
        message,
        metadata: sample.metadata.clone(),
        severity: sample.severity,
    }
}
/// Dispatches a notification to every channel that accepts it.
///
/// For each channel, calls [`NotificationChannel::accepts`] then
/// [`NotificationChannel::send`]. Errors are logged at `warn!` without
/// interrupting dispatch to the remaining channels.
///
/// Returns a map `channel_id -> Option<error_message>` for the channels that
/// accepted the notification (`None` = success, `Some(msg)` = error).
pub(super) async fn dispatch_notif(
    config: &NotificationConfig,
    channels: &[Box<dyn NotificationChannel>],
    notif: &Notification,
) -> HashMap<String, Option<String>> {
    let mut results = HashMap::new();
    for channel in channels {
        if channel.accepts(&notif.event, config) {
            match channel.send(notif).await {
                Ok(()) => {
                    results.insert(channel.id().to_string(), None);
                }
                Err(err) => {
                    tracing::warn!(
                        channel_id = channel.id(),
                        error = %err,
                        event = %notif.event,
                        detail = "the dispatch continues on the other channels",
                        "notification.channel.failed"
                    );
                    results.insert(channel.id().to_string(), Some(err.to_string()));
                }
            }
        }
    }
    results
}
/// Writes an entry into `notification_logs` (SQLite table in `hitl.db`).
///
/// `channel_results`: map `channel_id -> None` (success) or `Some(msg)` (error).
/// The table is created idempotently if it does not exist.
/// Errors are logged at `warn!` without propagation; logging is best-effort.
pub(super) fn write_notification_log(
    db_path: &std::path::Path,
    notif: &Notification,
    channel_results: &HashMap<String, Option<String>>,
) {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "notification.logs.open.failed");
            return;
        }
    };

    // `notification_logs` lives in `hitl.db`, whose schema (and
    // `PRAGMA user_version`) is owned by `apollia_tools::hitl_schema`; going
    // through it also refuses a database written by a newer binary.
    if let Err(e) = apollia_tools::hitl_schema::open_hitl_schema(&conn) {
        tracing::warn!(error = %e, "notification.logs.migration.failed");
        return;
    }

    // Serialize the per-channel results: { "desktop": "ok" | "error..." }
    let channels_json: serde_json::Map<String, serde_json::Value> = channel_results
        .iter()
        .map(|(id, err)| {
            let status = match err {
                None => serde_json::Value::String("ok".into()),
                Some(msg) => serde_json::Value::String(msg.clone()),
            };
            (id.clone(), status)
        })
        .collect();

    // First channel in error populates the global `error` field.
    let global_error: Option<String> = channel_results
        .values()
        .find_map(|e| e.as_deref().map(str::to_string));

    let id = uuid::Uuid::new_v4().to_string();
    let sent_at = notif.timestamp.to_rfc3339();
    let channels_str = serde_json::to_string(&channels_json).unwrap_or_else(|_| "{}".into());

    if let Err(e) = conn.execute(
        "INSERT INTO notification_logs (id, event_name, task_id, agent_id, sent_at, channels, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            notif.event,
            notif.task_id,
            notif.agent,
            sent_at,
            channels_str,
            global_error,
        ],
    ) {
        tracing::warn!(error = %e, "notification.logs.insert.failed");
    }
}
