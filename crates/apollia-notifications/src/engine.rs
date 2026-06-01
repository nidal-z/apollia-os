use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use tokio::time::interval;

use apollia_core::{EventBusSender, RuntimeEvent};

use crate::{config::NotificationConfig, config::Severity, event_filter};

/// Error returned by a notification channel during send.
///
/// Each variant corresponds to a channel category. The errors are logged at
/// `warn!` by the [`NotificationEngine`]; they never interrupt dispatch to the
/// other channels.
#[derive(Debug, thiserror::Error)]
pub enum NotifError {
    /// Desktop channel unavailable (OS notifications unsupported or permission denied).
    #[error("canal desktop indisponible : {0}")]
    DesktopUnavailable(String),
    /// Webhook call failed (network error, timeout, non-2xx HTTP code).
    #[error("webhook échoué : {0}")]
    WebhookFailed(String),
    /// Internal channel error (serialization, inconsistent state, etc.).
    #[error("erreur interne : {0}")]
    Internal(String),
    /// Malformed webhook URL: parsing failed.
    #[error("URL webhook invalide : {0}")]
    InvalidUrl(String),
    /// SSRF guard refused the send (URL pointing to loopback / RFC1918 /
    /// link-local / cloud metadata / `.local|.internal|localhost` domain).
    #[error("SSRF bloqué : {0}")]
    Ssrf(String),
}

/// Notification ready to be sent through one or more channels.
///
/// Produced by [`crate::event_filter::map_event`] from a [`RuntimeEvent`].
/// Distributed to each [`NotificationChannel`] that accepts it.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Name of the triggering event (e.g. `"task.input_required"`, `"task.failed"`).
    pub event: String,
    /// UTC timestamp of the notification.
    pub timestamp: DateTime<Utc>,
    /// Identifier of the relevant task, if applicable.
    pub task_id: Option<String>,
    /// Name or identifier of the relevant agent, if applicable.
    pub agent: Option<String>,
    /// Human-readable message for the user.
    pub message: String,
    /// Additional metadata (action URLs, identifiers, context).
    pub metadata: HashMap<String, String>,
    /// Notification severity.
    pub severity: Severity,
}

/// Trait implemented by each notification channel.
///
/// A channel is object-safe (`Box<dyn NotificationChannel>`) and thread-safe (`Send + Sync`).
/// The concrete channels are implemented in:
/// - [`crate::channels::desktop`]: native OS notifications.
/// - [`crate::channels::webhook`]: HTTP POST requests.
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Unique channel identifier as configured in `apollia.toml`.
    fn id(&self) -> &str;

    /// Returns `true` if this channel accepts the named event.
    ///
    /// Delegate the logic to [`crate::config::channel_accepts_event`], passing
    /// the channel's own state (`enabled`, `events`) and the global config.
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool;

    /// Sends the notification through this channel.
    ///
    /// On error, return a [`NotifError`]; the [`NotificationEngine`] logs the
    /// error and continues with the other channels without panicking.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError>;
}

/// Throttling state for a `(channel, event)` pair.
///
/// Set by [`apply_throttle`] before each dispatch and flushed by
/// [`flush_recaps`] at the end of the window. Everything is local to
/// [`run_engine_loop`]: the map is never shared across tasks, which eliminates
/// the Arc<Mutex>.
#[derive(Debug, Default)]
struct ThrottleState {
    /// Last actual emission (used to compute the end of the window).
    /// `None` = no emission yet; the first one always goes through.
    last_sent_at: Option<Instant>,
    /// Number of notifications dropped since the last emission.
    dropped_count: u32,
    /// Sample of the last dropped notification, used for the recap (we keep the
    /// timestamp and metadata of the most recent drop).
    recap_sample: Option<Notification>,
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

/// Internal command sent to the [`NotificationEngine`] via its handle.
enum NotifEngineCommand {
    /// Replaces the active configuration and channels (hot-reload).
    Reload {
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
    },
    /// Publishes a notification directly without going through the EventBus.
    Publish { notification: Notification },
    /// Requests a clean engine shutdown.
    Shutdown,
}

/// Handle returned by [`NotificationEngine::spawn`] to control the engine.
///
/// Cloneable: storable in `AppState` (REST routes) and `SupervisorHandles`
/// (graceful shutdown) at the same time.
///
/// Call [`NotificationEngineHandle::shutdown`] to signal the engine to stop.
/// Call [`NotificationEngineHandle::reload`] to hot-reload configuration.
pub struct NotificationEngineHandle {
    tx: mpsc::Sender<NotifEngineCommand>,
}

impl Clone for NotificationEngineHandle {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl NotificationEngineHandle {
    /// Hot-reloads the engine's configuration and channels.
    ///
    /// The engine immediately replaces its internal channels. Events received
    /// after the reload use the new configuration.
    pub async fn reload(
        &self,
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
    ) {
        let _ = self
            .tx
            .send(NotifEngineCommand::Reload { config, channels })
            .await;
    }

    /// Publishes a notification directly, without going through the EventBus.
    ///
    /// The notification is dispatched to every channel whose
    /// [`NotificationChannel::accepts`] returns `true`. Used by
    /// [`crate::inactivity_watcher::InactivityWatcher`].
    pub async fn publish(&self, notification: Notification) {
        let _ = self
            .tx
            .send(NotifEngineCommand::Publish { notification })
            .await;
    }

    /// Signal the engine to stop gracefully.
    ///
    /// Fire-and-forget: the engine stops its loop on receipt.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(NotifEngineCommand::Shutdown).await;
    }
}

/// Notification engine: subscribes to the EventBus and dispatches events to the
/// configured channels.
///
/// Started by the Supervisor via [`NotificationEngine::spawn`].
/// Each received event is turned into a [`Notification`] via [`map_event`], then
/// dispatched to every channel whose [`NotificationChannel::accepts`] returns
/// `true`.
///
/// Channel errors are logged at `warn!` and do not interrupt dispatch.
pub struct NotificationEngine {
    config: NotificationConfig,
    channels: Vec<Box<dyn NotificationChannel>>,
    event_bus: EventBusSender,
    /// Base URL of the local REST API (e.g. `http://127.0.0.1:7771`).
    ///
    /// Built from `ApiConfig` at startup. Used to produce the HITL resume URLs
    /// in the notification metadata.
    api_base_url: String,
    /// Path to the `hitl.db` SQLite database for writing to `notification_logs`.
    ///
    /// `None` = logging disabled (tests, dev without data_dir). In production,
    /// the Supervisor passes `Some(data_dir.join("hitl.db"))`.
    log_db_path: Option<PathBuf>,
    /// `true` if the LLM cost threshold has already triggered a notification this session.
    ///
    /// Edge trigger: reset to `false` when `threshold_exceeded` returns to
    /// `false` (new session or cost dropped below the threshold).
    cost_threshold_already_notified: bool,
}

impl NotificationEngine {
    /// Creates a new notification engine.
    ///
    /// `api_base_url`: base URL of the local REST API (e.g. `http://127.0.0.1:7771`),
    /// built from `ApiConfig` at Supervisor startup.
    ///
    /// `log_db_path`: path to `hitl.db` to write the `notification_logs` table.
    /// Pass `None` to disable SQLite logging (tests).
    ///
    /// Emits a `tracing::warn!` for each event name in `config.events` or in the
    /// channel lists that is not recognized by the engine. Startup is not
    /// blocked by these warnings.
    pub fn new(
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
        event_bus: EventBusSender,
        api_base_url: String,
        log_db_path: Option<PathBuf>,
    ) -> Self {
        // Validate global event names and per-channel event names at startup.
        event_filter::warn_unknown_events(&config.events);
        for channel_cfg in &config.channels {
            if let Some(events) = &channel_cfg.events {
                event_filter::warn_unknown_events(events);
            }
        }

        Self {
            config,
            channels,
            event_bus,
            api_base_url,
            log_db_path,
            cost_threshold_already_notified: false,
        }
    }

    /// Spawns the engine as a Tokio task and returns a [`NotificationEngineHandle`].
    ///
    /// The handle allows the Supervisor to stop the engine gracefully before the
    /// EventBus closes, and REST routes to hot-reload configuration after CRUD
    /// mutations.
    pub fn spawn(self) -> NotificationEngineHandle {
        let (cmd_tx, cmd_rx) = mpsc::channel(8);
        let NotificationEngine {
            config,
            channels,
            event_bus,
            api_base_url,
            log_db_path,
            cost_threshold_already_notified,
        } = self;
        tokio::spawn(run_engine_loop(EngineLoopState {
            config,
            channels,
            event_bus,
            api_base_url,
            log_db_path,
            cost_threshold_already_notified,
            cmd_rx,
        }));
        NotificationEngineHandle { tx: cmd_tx }
    }

    /// Reaction to [`RuntimeEvent::TokenBudgetUpdated`].
    ///
    /// Edge trigger: emits an OS notification only on the `false -> true`
    /// transition of `threshold_exceeded`. The flag is rearmed when
    /// `threshold_exceeded` returns to `false` (new session or cost dropped
    /// below the threshold).
    ///
    /// Emits nothing if the threshold is not yet exceeded or if the notification
    /// has already been sent for this excess.
    pub async fn handle_budget_update(&mut self, event: &RuntimeEvent) {
        process_budget_alert(
            &mut self.cost_threshold_already_notified,
            event,
            DispatchCtx {
                config: &self.config,
                channels: &self.channels,
                api_base_url: &self.api_base_url,
                log_db_path: self.log_db_path.as_deref(),
            },
        )
        .await;
    }

    /// Turns a [`RuntimeEvent`] into a [`Notification`].
    ///
    /// Pure function: delegates to [`event_filter::map_event`].
    /// Testable without infrastructure.
    pub fn map_event(&self, event: &RuntimeEvent) -> Option<Notification> {
        event_filter::map_event(&self.api_base_url, event)
    }
}

/// Builds the LLM cost-threshold alert notification.
fn build_cost_alert_notification(
    session_cost_usd: f64,
    threshold_usd: f64,
    api_base_url: &str,
) -> Notification {
    let mut metadata = HashMap::new();
    metadata.insert("dashboard_url".into(), format!("{api_base_url}/dashboard"));
    Notification {
        event: "llm.cost_alert".into(),
        timestamp: chrono::Utc::now(),
        task_id: None,
        agent: None,
        message: format!("Coût session : ${session_cost_usd:.3} (seuil : ${threshold_usd:.3})"),
        metadata,
        severity: crate::config::Severity::Warning,
    }
}

/// Dispatch dependencies shared by the notification paths, grouped to avoid
/// passing a long list of parameters around.
struct DispatchCtx<'a> {
    config: &'a NotificationConfig,
    channels: &'a [Box<dyn NotificationChannel>],
    api_base_url: &'a str,
    log_db_path: Option<&'a std::path::Path>,
}

/// Evaluates a [`RuntimeEvent::TokenBudgetUpdated`] against the edge-trigger state.
///
/// If `threshold_exceeded` goes from `false` to `true`, dispatches a
/// notification via the configured channels and updates `already_notified`.
/// Rearms `already_notified` to `false` when `threshold_exceeded` returns to `false`.
async fn process_budget_alert(
    already_notified: &mut bool,
    event: &RuntimeEvent,
    ctx: DispatchCtx<'_>,
) {
    let RuntimeEvent::TokenBudgetUpdated {
        session_cost_usd,
        threshold_usd,
        threshold_exceeded,
        ..
    } = event
    else {
        return;
    };

    if *threshold_exceeded && !*already_notified {
        *already_notified = true;
        let notif =
            build_cost_alert_notification(*session_cost_usd, *threshold_usd, ctx.api_base_url);
        let results = dispatch_notif(ctx.config, ctx.channels, &notif).await;
        if let Some(db_path) = ctx.log_db_path {
            let db_path = db_path.to_path_buf();
            tokio::task::spawn_blocking(move || {
                write_notification_log(&db_path, &notif, &results);
            });
        }
    } else if !threshold_exceeded {
        *already_notified = false;
    }
}

/// Owned state moved into the notification engine's loop task.
///
/// Groups the config, channels, bus, and edge-trigger state to pass a single
/// bundle to [`run_engine_loop`] instead of a long list of parameters.
struct EngineLoopState {
    config: NotificationConfig,
    channels: Vec<Box<dyn NotificationChannel>>,
    event_bus: EventBusSender,
    api_base_url: String,
    log_db_path: Option<PathBuf>,
    cost_threshold_already_notified: bool,
    cmd_rx: mpsc::Receiver<NotifEngineCommand>,
}

/// Main loop of the notification engine (free function).
///
/// Listens to both the EventBus (runtime events) and the command channel
/// (reload / shutdown). The reload replaces the config and channels live,
/// without interrupting the EventBus listening.
async fn run_engine_loop(state: EngineLoopState) {
    let EngineLoopState {
        mut config,
        mut channels,
        event_bus,
        api_base_url,
        log_db_path,
        mut cost_threshold_already_notified,
        mut cmd_rx,
    } = state;

    let mut rx = event_bus.subscribe();
    drop(event_bus);

    // Per-(channel_id, event_name) throttle state: local to this task,
    // never shared. Reset on engine restart by design (UX anti-spam, not
    // a security rate-limit).
    let mut throttle: HashMap<(String, String), ThrottleState> = HashMap::new();

    // Tick every second to flush pending recaps. 1s is a fine resolution
    // given throttle windows are in the 60s-3600s range.
    let mut recap_tick = interval(Duration::from_secs(1));
    recap_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(NotifEngineCommand::Reload { config: new_config, channels: new_channels }) => {
                        let count = new_channels.len();
                        config = new_config;
                        channels = new_channels;
                        // Reload drops obsolete throttle entries for channels that
                        // no longer exist; remaining entries keep their counters.
                        throttle.retain(|(channel_id, _), _| {
                            config.channels.iter().any(|c| &c.id == channel_id)
                        });
                        tracing::info!(channels = count, "NotificationEngine : configuration rechargée");
                    }
                    Some(NotifEngineCommand::Publish { notification }) => {
                        let channel_results = dispatch_with_throttle(
                            &config,
                            &channels,
                            &notification,
                            &mut throttle,
                            Instant::now(),
                        )
                        .await;
                        if let Some(ref db_path) = log_db_path {
                            let db_path = db_path.clone();
                            let notif_clone = notification.clone();
                            tokio::task::spawn_blocking(move || {
                                write_notification_log(&db_path, &notif_clone, &channel_results);
                            });
                        }
                    }
                    Some(NotifEngineCommand::Shutdown) | None => {
                        tracing::info!("NotificationEngine : signal d'arrêt reçu - arrêt propre");
                        break;
                    }
                }
            }
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if let Some(notif) = map_event_with(&config, &channels, &api_base_url, &event) {
                            let channel_results = dispatch_with_throttle(
                                &config,
                                &channels,
                                &notif,
                                &mut throttle,
                                Instant::now(),
                            )
                            .await;
                            if let Some(ref db_path) = log_db_path {
                                let db_path = db_path.clone();
                                let notif_clone = notif.clone();
                                tokio::task::spawn_blocking(move || {
                                    write_notification_log(
                                        &db_path,
                                        &notif_clone,
                                        &channel_results,
                                    );
                                });
                            }
                        }
                        process_budget_alert(
                            &mut cost_threshold_already_notified,
                            &event,
                            DispatchCtx {
                                config: &config,
                                channels: &channels,
                                api_base_url: &api_base_url,
                                log_db_path: log_db_path.as_deref(),
                            },
                        )
                        .await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "NotificationEngine a raté des événements (bus saturé)"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("NotificationEngine : bus fermé - arrêt propre");
                        break;
                    }
                }
            }
            _ = recap_tick.tick() => {
                flush_recaps(&config, &channels, &mut throttle, log_db_path.as_deref(), Instant::now()).await;
            }
        }
    }
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
async fn dispatch_with_throttle(
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
                    "Notification throttled - sera incluse dans le prochain récap"
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
                "Canal de notification en erreur - dispatch continue"
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
async fn flush_recaps(
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
        "{} événements « {} » au cours des {} dernières secondes",
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

/// Turns a [`RuntimeEvent`] into a [`Notification`] in the context of `run()`.
///
/// Free function used after destructuring `self` in [`NotificationEngine::run`].
fn map_event_with(
    _config: &NotificationConfig,
    _channels: &[Box<dyn NotificationChannel>],
    base_url: &str,
    event: &RuntimeEvent,
) -> Option<Notification> {
    event_filter::map_event(base_url, event)
}

/// Dispatches a notification to every channel that accepts it.
///
/// For each channel, calls [`NotificationChannel::accepts`] then
/// [`NotificationChannel::send`]. Errors are logged at `warn!` without
/// interrupting dispatch to the remaining channels.
///
/// Returns a map `channel_id -> Option<error_message>` for the channels that
/// accepted the notification (`None` = success, `Some(msg)` = error).
async fn dispatch_notif(
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
                        "Canal de notification en erreur - dispatch continue"
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
fn write_notification_log(
    db_path: &std::path::Path,
    notif: &Notification,
    channel_results: &HashMap<String, Option<String>>,
) {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "notification_logs : impossible d'ouvrir la base");
            return;
        }
    };

    if let Err(e) = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notification_logs (
            id          TEXT    PRIMARY KEY,
            event_name  TEXT    NOT NULL,
            task_id     TEXT,
            agent_id    TEXT,
            sent_at     TEXT    NOT NULL DEFAULT (datetime('now')),
            channels    TEXT    NOT NULL DEFAULT '{}',
            error       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_notif_logs_sent_at ON notification_logs(sent_at);",
    ) {
        tracing::warn!(error = %e, "notification_logs : migration échouée");
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
        tracing::warn!(error = %e, "notification_logs : INSERT échoué");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{channel_accepts_event, ChannelConfig, ChannelKind};
    use apollia_core::{AgentId, TaskId};
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    /// Mock channel for unit tests.
    struct MockChannel {
        name: String,
        enabled: bool,
        events: Option<Vec<String>>,
        should_fail: bool,
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl NotificationChannel for MockChannel {
        fn id(&self) -> &str {
            &self.name
        }

        fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
            channel_accepts_event(self.enabled, &self.events, event, &config.events)
        }

        async fn send(&self, _notif: &Notification) -> Result<(), NotifError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                Err(NotifError::Internal("erreur simulée".into()))
            } else {
                Ok(())
            }
        }
    }

    fn make_config(global_events: Vec<String>) -> NotificationConfig {
        NotificationConfig {
            events: global_events,
            channels: vec![],
            inactivity_timeout_secs: 30,
        }
    }

    #[test]
    fn test_ac3_accepts_subset_events_filters_correctly() {
        // GIVEN a channel configured with a subset of events
        let channel = MockChannel {
            name: "slack".into(),
            enabled: true,
            events: Some(vec!["task.input_required".into(), "task.failed".into()]),
            should_fail: false,
            call_count: Arc::new(AtomicU32::new(0)),
        };
        let config = make_config(vec![
            "task.input_required".into(),
            "task.failed".into(),
            "agent.degraded".into(),
        ]);

        // WHEN / THEN agent.degraded is rejected: absent from the channel list
        assert!(!channel.accepts("agent.degraded", &config));
        // AND the listed events are accepted
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
    }

    #[test]
    fn test_ac4_accepts_wildcard_all_events() {
        // GIVEN a channel configured with the "*" wildcard
        let channel = MockChannel {
            name: "monitoring".into(),
            enabled: true,
            events: Some(vec!["*".into()]),
            should_fail: false,
            call_count: Arc::new(AtomicU32::new(0)),
        };
        let config = make_config(vec![
            "task.input_required".into(),
            "task.failed".into(),
            "agent.degraded".into(),
        ]);

        // WHEN / THEN every event in the global list is accepted
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        assert!(channel.accepts("agent.degraded", &config));
    }

    #[tokio::test]
    async fn test_ac5_channel_error_does_not_stop_other_channels() {
        // GIVEN two channels: "desktop" OK, "slack" returns an error
        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);

        let desktop_count = Arc::new(AtomicU32::new(0));
        let desktop_count_clone = desktop_count.clone();

        let config = make_config(vec!["task.input_required".into()]);

        let channels: Vec<Box<dyn NotificationChannel>> = vec![
            Box::new(MockChannel {
                name: "desktop".into(),
                enabled: true,
                events: None,
                should_fail: false,
                call_count: desktop_count_clone,
            }),
            Box::new(MockChannel {
                name: "slack".into(),
                enabled: true,
                events: None,
                should_fail: true,
                call_count: Arc::new(AtomicU32::new(0)),
            }),
        ];

        let engine = NotificationEngine::new(
            config,
            channels,
            tx.clone(),
            "http://127.0.0.1:7771".to_owned(),
            None,
        );
        let handle = engine.spawn();

        // Let the engine subscribe to the bus.
        tokio::task::yield_now().await;

        // WHEN we send an event
        tx.send(RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-001"),
            prompt: "Confirmer ?".into(),
            step_id: None,
        })
        .expect("envoi échoue");

        // Let the dispatch run.
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Shut down gracefully via the handle.
        handle.shutdown().await;

        // THEN desktop received the notification despite the slack error
        assert_eq!(desktop_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_map_event_delegates_to_event_filter() {
        // GIVEN NotificationEngine delegates map_event to event_filter::map_event
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let engine = NotificationEngine::new(
            make_config(vec![]),
            vec![],
            tx,
            "http://127.0.0.1:7771".to_owned(),
            None,
        );

        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-test"),
            prompt: "Test ?".into(),
            step_id: None,
        };

        // WHEN
        let notif = engine.map_event(&event);

        // THEN
        assert!(notif.is_some());
        assert_eq!(notif.unwrap().event, "task.input_required");
    }

    #[test]
    fn test_map_event_returns_none_for_unknown_event() {
        // GIVEN
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let engine = NotificationEngine::new(
            make_config(vec![]),
            vec![],
            tx,
            "http://127.0.0.1:7771".to_owned(),
            None,
        );

        let event = RuntimeEvent::AgentRegistered(AgentId::from("agent-1"));

        // WHEN / THEN
        assert!(engine.map_event(&event).is_none());
    }

    #[test]
    fn test_unknown_notification_event_emits_warning() {
        // GIVEN a set of event names where one is unknown and one is the wildcard
        let events = vec![
            "task.failed".to_string(),
            "agent.exploded".to_string(),
            "*".to_string(),
        ];

        // WHEN finding unknown events from the known set
        let unknown: Vec<&str> = events
            .iter()
            .filter(|e| *e != "*" && !crate::event_filter::KNOWN_EVENT_NAMES.contains(&e.as_str()))
            .map(String::as_str)
            .collect();

        // THEN "agent.exploded" is identified as unknown; "*" and "task.failed" are not
        assert_eq!(unknown, vec!["agent.exploded"]);

        // AND calling warn_unknown_events does not panic or block
        crate::event_filter::warn_unknown_events(&events);
    }

    /// Verifies that ChannelConfig deserializes correctly from TOML-compatible JSON.
    #[test]
    fn test_channel_config_deserialize() {
        let json = r#"{
            "id": "desktop",
            "type": "desktop",
            "enabled": true,
            "events": ["task.input_required", "task.failed"]
        }"#;
        let config: ChannelConfig = serde_json::from_str(json).expect("désérialisation échoue");
        assert_eq!(config.id, "desktop");
        assert!(matches!(config.kind, ChannelKind::Desktop));
        assert!(config.enabled);
        assert_eq!(
            config.events.as_deref(),
            Some(&["task.input_required".to_string(), "task.failed".to_string()][..])
        );
    }

    fn make_budget_event(threshold_exceeded: bool) -> RuntimeEvent {
        RuntimeEvent::TokenBudgetUpdated {
            session_cost_usd: 0.75,
            total_input_tokens: 1000,
            total_output_tokens: 500,
            total_cache_read_tokens: 200,
            threshold_usd: 0.50,
            threshold_exceeded,
        }
    }

    #[tokio::test]
    async fn handle_budget_update_notifies_on_threshold_exceeded() {
        // GIVEN engine with a mock channel that accepts llm.cost_alert
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let call_count = Arc::new(AtomicU32::new(0));
        let config = make_config(vec!["llm.cost_alert".into()]);
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: call_count.clone(),
        })];
        let mut engine =
            NotificationEngine::new(config, channels, tx, "http://127.0.0.1:7771".into(), None);

        // WHEN threshold is exceeded and not yet notified
        let event = make_budget_event(true);
        engine.handle_budget_update(&event).await;

        // THEN notification dispatched once
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert!(engine.cost_threshold_already_notified);
    }

    #[tokio::test]
    async fn handle_budget_update_no_duplicate_notification() {
        // GIVEN engine already notified (flag set by a prior exceeded event)
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let call_count = Arc::new(AtomicU32::new(0));
        let config = make_config(vec!["llm.cost_alert".into()]);
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: call_count.clone(),
        })];
        let mut engine =
            NotificationEngine::new(config, channels, tx, "http://127.0.0.1:7771".into(), None);

        // Set the flag via the first exceeded event
        engine.handle_budget_update(&make_budget_event(true)).await;
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // WHEN threshold exceeded again (still above threshold)
        engine.handle_budget_update(&make_budget_event(true)).await;

        // THEN no second notification dispatched
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn handle_budget_update_rearms_on_false() {
        // GIVEN engine already notified
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let mut engine = NotificationEngine::new(
            make_config(vec!["llm.cost_alert".into()]),
            vec![],
            tx,
            "http://127.0.0.1:7771".into(),
            None,
        );
        engine.handle_budget_update(&make_budget_event(true)).await;
        assert!(engine.cost_threshold_already_notified);

        // WHEN threshold no longer exceeded (new session or cost dropped)
        engine.handle_budget_update(&make_budget_event(false)).await;

        // THEN flag is reset
        assert!(!engine.cost_threshold_already_notified);
    }

    // Throttle tests

    fn throttle_config(channel_id: &str, event: &str, min_interval: u32) -> NotificationConfig {
        NotificationConfig {
            events: vec![event.into()],
            channels: vec![ChannelConfig {
                id: channel_id.into(),
                kind: ChannelKind::Desktop,
                enabled: true,
                events: None,
                url: None,
                signing_secret: None,
                min_severity: None,
                min_interval_seconds: min_interval,
            }],
            inactivity_timeout_secs: 30,
        }
    }

    fn make_notif(event: &str) -> Notification {
        Notification {
            event: event.into(),
            timestamp: chrono::Utc::now(),
            task_id: Some("t-001".into()),
            agent: None,
            message: format!("test {event}"),
            metadata: HashMap::new(),
            severity: Severity::Info,
        }
    }

    #[tokio::test]
    async fn test_throttle_blocks_within_window() {
        // GIVEN a channel with 60s throttle on task.completed
        let config = throttle_config("desktop", "task.completed", 60);
        let count = Arc::new(AtomicU32::new(0));
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: count.clone(),
        })];
        let mut throttle = HashMap::new();
        let t0 = Instant::now();

        // WHEN we dispatch 3 events in rapid succession
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0,
        )
        .await;
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0 + Duration::from_secs(5),
        )
        .await;
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0 + Duration::from_secs(30),
        )
        .await;

        // THEN only the first one reached the channel; the other 2 were dropped
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // AND the throttle entry tracks the 2 drops
        let state = throttle
            .get(&("desktop".into(), "task.completed".into()))
            .expect("throttle entry");
        assert_eq!(state.dropped_count, 2);
        assert!(state.recap_sample.is_some());
    }

    #[tokio::test]
    async fn test_throttle_releases_after_window() {
        // GIVEN a channel with 60s throttle that already sent once
        let config = throttle_config("desktop", "task.completed", 60);
        let count = Arc::new(AtomicU32::new(0));
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: count.clone(),
        })];
        let mut throttle = HashMap::new();
        let t0 = Instant::now();

        // WHEN one notif passes, then we wait past the window
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0,
        )
        .await;
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0 + Duration::from_secs(61),
        )
        .await;

        // THEN both reached the channel - the second one re-armed the window
        assert_eq!(count.load(Ordering::SeqCst), 2);
        let state = throttle
            .get(&("desktop".into(), "task.completed".into()))
            .expect("throttle entry");
        assert_eq!(state.dropped_count, 0);
    }

    #[tokio::test]
    async fn test_throttle_is_per_event_not_global() {
        // GIVEN a channel with 60s throttle (applies to *every* event independently)
        let mut config = throttle_config("desktop", "task.completed", 60);
        // Add another event to the global list so the channel accepts it too
        config.events.push("task.input_required".into());

        let count = Arc::new(AtomicU32::new(0));
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: count.clone(),
        })];
        let mut throttle = HashMap::new();
        let t0 = Instant::now();

        // WHEN we send task.completed (consumed) then task.input_required immediately
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0,
        )
        .await;
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.input_required"),
            &mut throttle,
            t0 + Duration::from_millis(10),
        )
        .await;

        // THEN both pass - throttle is per (channel, event) not per channel
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_throttle_zero_disables() {
        // GIVEN a channel with min_interval = 0 (no throttling)
        let config = throttle_config("desktop", "task.completed", 0);
        let count = Arc::new(AtomicU32::new(0));
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: count.clone(),
        })];
        let mut throttle = HashMap::new();
        let t0 = Instant::now();

        // WHEN we send 5 events back-to-back
        for i in 0..5 {
            dispatch_with_throttle(
                &config,
                &channels,
                &make_notif("task.completed"),
                &mut throttle,
                t0 + Duration::from_millis(i),
            )
            .await;
        }

        // THEN all 5 reached the channel and no throttle entry was created
        assert_eq!(count.load(Ordering::SeqCst), 5);
        assert!(throttle.is_empty());
    }

    #[tokio::test]
    async fn test_flush_recaps_emits_summary() {
        // GIVEN a 60s throttle with 3 drops accumulated, window now elapsed
        let config = throttle_config("desktop", "task.completed", 60);
        let count = Arc::new(AtomicU32::new(0));
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: count.clone(),
        })];
        let mut throttle = HashMap::new();
        let t0 = Instant::now();

        // 1 initial send + 3 drops
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0,
        )
        .await;
        for i in 1..=3 {
            dispatch_with_throttle(
                &config,
                &channels,
                &make_notif("task.completed"),
                &mut throttle,
                t0 + Duration::from_secs(i * 5),
            )
            .await;
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // WHEN we flush after the window elapsed
        flush_recaps(
            &config,
            &channels,
            &mut throttle,
            None,
            t0 + Duration::from_secs(61),
        )
        .await;

        // THEN the recap was emitted (+1 send)
        assert_eq!(count.load(Ordering::SeqCst), 2);

        // AND the throttle state is rearmed
        let state = throttle
            .get(&("desktop".into(), "task.completed".into()))
            .expect("entry persists");
        assert_eq!(state.dropped_count, 0);
        assert!(state.recap_sample.is_none());
    }

    #[tokio::test]
    async fn test_flush_recaps_skips_within_window() {
        // GIVEN drops accumulated but window not yet elapsed
        let config = throttle_config("desktop", "task.completed", 60);
        let count = Arc::new(AtomicU32::new(0));
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: count.clone(),
        })];
        let mut throttle = HashMap::new();
        let t0 = Instant::now();

        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0,
        )
        .await;
        dispatch_with_throttle(
            &config,
            &channels,
            &make_notif("task.completed"),
            &mut throttle,
            t0 + Duration::from_secs(10),
        )
        .await;
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // WHEN we flush only 30s after - still within window
        flush_recaps(
            &config,
            &channels,
            &mut throttle,
            None,
            t0 + Duration::from_secs(30),
        )
        .await;

        // THEN no recap was emitted (still 1 total send)
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
