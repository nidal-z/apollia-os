use std::{collections::HashMap, sync::Arc, time::Duration, time::Instant};

use tokio::sync::watch;

use crate::{
    config::Severity,
    engine::{Notification, NotificationEngineHandle},
};

/// Watches runtime inactivity and sends a notification if the threshold is exceeded.
///
/// The actor is started via [`InactivityWatcher::start`], which launches a
/// `tokio::spawn`. The timer is rearmed by [`InactivityWatcher::reset_timer`] on
/// every significant runtime event (to be called from the EventBus). After each
/// notification is sent, the timer rearms automatically to avoid spam.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tokio::sync::watch;
/// use apollia_notifications::{InactivityWatcher, NotificationEngineHandle};
///
/// async fn example(handle: NotificationEngineHandle) {
///     let watcher = Arc::new(InactivityWatcher::new(30, handle));
///     let (_active_tx, active_rx) = watch::channel(true);
///     watcher.clone().start(active_rx);
///     // reset_timer() is called from the EventBus on each significant RuntimeEvent
///     watcher.reset_timer();
/// }
/// ```
pub struct InactivityWatcher {
    timeout: Duration,
    engine_handle: NotificationEngineHandle,
    last_activity_tx: watch::Sender<Instant>,
}

impl InactivityWatcher {
    /// Creates a new watcher with the timeout and the notification engine handle.
    ///
    /// The timer is initialized to `Instant::now()` at construction.
    pub fn new(timeout_secs: u64, engine_handle: NotificationEngineHandle) -> Self {
        let (tx, _) = watch::channel(Instant::now());
        Self {
            timeout: Duration::from_secs(timeout_secs),
            engine_handle,
            last_activity_tx: tx,
        }
    }

    /// Rearms the inactivity timer to the current instant.
    ///
    /// To be called from the EventBus consumer on each
    /// `RuntimeEvent::is_significant_for_inactivity()`.
    pub fn reset_timer(&self) {
        let _ = self.last_activity_tx.send(Instant::now());
    }

    /// Runs the watch loop in an isolated `tokio::spawn`.
    ///
    /// - When `task_active_rx` is `false`, the loop waits without sending a notification.
    /// - When `task_active_rx` becomes `true`, the timer starts.
    /// - After [`Self::timeout`] with no activity, a notification is published and
    ///   the timer rearms automatically (anti-spam).
    pub fn start(self: Arc<Self>, mut task_active_rx: watch::Receiver<bool>) {
        tokio::spawn(async move {
            let mut activity_rx = self.last_activity_tx.subscribe();

            loop {
                // Wait for the task to be active.
                while !*task_active_rx.borrow() {
                    if task_active_rx.changed().await.is_err() {
                        return;
                    }
                }

                // Compute the time remaining before timeout.
                let last = *activity_rx.borrow();
                let remaining = self.timeout.saturating_sub(last.elapsed());

                tokio::select! {
                    _ = tokio::time::sleep(remaining) => {
                        // Check the task is still active before notifying.
                        if *task_active_rx.borrow() {
                            self.engine_handle
                                .publish(build_inactivity_notification())
                                .await;
                            // Rearm the timer after notification (anti-spam).
                            let _ = self.last_activity_tx.send(Instant::now());
                        }
                    }
                    result = activity_rx.changed() => {
                        if result.is_err() {
                            return;
                        }
                        // Activity detected: loop back to recompute the remaining time.
                    }
                    result = task_active_rx.changed() => {
                        if result.is_err() {
                            return;
                        }
                        // task_active changed: loop back to re-evaluate the status.
                    }
                }
            }
        });
    }
}

/// Builds the standard inactivity notification.
fn build_inactivity_notification() -> Notification {
    Notification {
        event: "agent.inactivity".into(),
        timestamp: chrono::Utc::now(),
        task_id: None,
        agent: None,
        message: "Apollia est en attente - une action de votre part est peut-être requise".into(),
        metadata: HashMap::new(),
        severity: Severity::Warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{channel_accepts_event, NotificationConfig},
        engine::{NotifError, NotificationChannel, NotificationEngine},
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Mock channel that counts received notifications.
    struct CountingChannel {
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl NotificationChannel for CountingChannel {
        fn id(&self) -> &str {
            "mock"
        }

        fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
            channel_accepts_event(true, &None, event, &config.events)
        }

        async fn send(&self, _notif: &Notification) -> Result<(), NotifError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// Returns `(handle, call_count, _bus_keep_alive)`.
    ///
    /// The `_bus_keep_alive` MUST be kept in the test's local variable so the
    /// broadcast channel stays open for the duration of the test. Without it,
    /// `run_engine_loop` gets `RecvError::Closed` on its first poll and stops
    /// immediately.
    fn make_engine_with_counter() -> (
        NotificationEngineHandle,
        Arc<AtomicU32>,
        apollia_core::EventBusSender,
    ) {
        let call_count = Arc::new(AtomicU32::new(0));
        let config = NotificationConfig {
            events: vec!["agent.inactivity".into()],
            channels: vec![],
            inactivity_timeout_secs: 1,
        };
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(CountingChannel {
            call_count: call_count.clone(),
        })];
        // Pass a clone so `run_engine_loop` can drop its copy without closing the channel.
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let engine = NotificationEngine::new(
            config,
            channels,
            tx.clone(),
            "http://127.0.0.1:7771".to_owned(),
            None,
        );
        let handle = engine.spawn();
        (handle, call_count, tx)
    }

    #[tokio::test]
    async fn inactivity_watcher_triggers_after_timeout() {
        // GIVEN InactivityWatcher with a 1s timeout, active task
        let (handle, call_count, _bus) = make_engine_with_counter();
        let watcher = Arc::new(InactivityWatcher::new(1, handle));
        let (_active_tx, active_rx) = watch::channel(true);
        watcher.clone().start(active_rx);

        // WHEN no reset_timer() for 1.1s
        tokio::time::sleep(Duration::from_millis(1100)).await;
        // Let the engine consume the Publish command from its cmd_rx.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // THEN notification published
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn inactivity_watcher_resets_on_event() {
        // GIVEN InactivityWatcher with a 1s timeout
        let (handle, call_count, _bus) = make_engine_with_counter();
        let watcher = Arc::new(InactivityWatcher::new(1, handle));
        let (_active_tx, active_rx) = watch::channel(true);
        watcher.clone().start(active_rx);

        // WHEN reset_timer() at 800ms, waiting 1.5s total
        tokio::time::sleep(Duration::from_millis(800)).await;
        watcher.reset_timer();
        tokio::time::sleep(Duration::from_millis(700)).await;

        // THEN no notification (timer rearmed at 800ms, new timeout at 1800ms total)
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn inactivity_watcher_no_notification_when_task_inactive() {
        // GIVEN InactivityWatcher with an inactive task
        let (handle, call_count, _bus) = make_engine_with_counter();
        let watcher = Arc::new(InactivityWatcher::new(1, handle));
        let (_active_tx, active_rx) = watch::channel(false);
        watcher.clone().start(active_rx);

        // WHEN waiting 1.5s without the task being active
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // THEN no notification
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn inactivity_notification_has_correct_fields() {
        // GIVEN
        // WHEN
        let notif = build_inactivity_notification();

        // THEN
        assert_eq!(notif.event, "agent.inactivity");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(notif.task_id.is_none());
        assert!(!notif.message.is_empty());
    }
}
