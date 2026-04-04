use std::{collections::HashMap, sync::Arc, time::Duration, time::Instant};

use tokio::sync::watch;

use crate::{
    config::Severity,
    engine::{Notification, NotificationEngineHandle},
};

/// Surveille l'inactivité du runtime et envoie une notification si le seuil est dépassé.
///
/// L'acteur est démarré via [`InactivityWatcher::start`] qui lance un `tokio::spawn`.
/// Le timer est réarmé par [`InactivityWatcher::reset_timer`] à chaque événement significatif
/// du runtime (à appeler depuis l'EventBus). Après chaque notification envoyée, le timer est
/// réarmé automatiquement pour éviter le spam.
///
/// # Exemple
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
///     // reset_timer() est appelé depuis l'EventBus à chaque RuntimeEvent significatif
///     watcher.reset_timer();
/// }
/// ```
pub struct InactivityWatcher {
    timeout: Duration,
    engine_handle: NotificationEngineHandle,
    last_activity_tx: watch::Sender<Instant>,
}

impl InactivityWatcher {
    /// Crée un nouveau watcher avec le timeout et le handle du moteur de notification.
    ///
    /// Le timer est initialisé à `Instant::now()` à la construction.
    pub fn new(timeout_secs: u64, engine_handle: NotificationEngineHandle) -> Self {
        let (tx, _) = watch::channel(Instant::now());
        Self {
            timeout: Duration::from_secs(timeout_secs),
            engine_handle,
            last_activity_tx: tx,
        }
    }

    /// Réarme le timer d'inactivité à l'instant courant.
    ///
    /// À appeler depuis le consommateur de l'EventBus à chaque
    /// `RuntimeEvent::is_significant_for_inactivity()`.
    pub fn reset_timer(&self) {
        let _ = self.last_activity_tx.send(Instant::now());
    }

    /// Lance la boucle de surveillance dans un `tokio::spawn` isolé.
    ///
    /// - Quand `task_active_rx` est `false`, la boucle attend sans envoyer de notification.
    /// - Quand `task_active_rx` passe à `true`, le timer démarre.
    /// - Après [`Self::timeout`] sans activité, une notification est publiée et le timer
    ///   se réarme automatiquement (anti-spam).
    pub fn start(self: Arc<Self>, mut task_active_rx: watch::Receiver<bool>) {
        tokio::spawn(async move {
            let mut activity_rx = self.last_activity_tx.subscribe();

            loop {
                // Attendre que la tâche soit active
                while !*task_active_rx.borrow() {
                    if task_active_rx.changed().await.is_err() {
                        return;
                    }
                }

                // Calculer le temps restant avant timeout
                let last = *activity_rx.borrow();
                let remaining = self.timeout.saturating_sub(last.elapsed());

                tokio::select! {
                    _ = tokio::time::sleep(remaining) => {
                        // Vérifier que la tâche est toujours active avant de notifier
                        if *task_active_rx.borrow() {
                            self.engine_handle
                                .publish(build_inactivity_notification())
                                .await;
                            // Réarmer le timer après notification (anti-spam)
                            let _ = self.last_activity_tx.send(Instant::now());
                        }
                    }
                    result = activity_rx.changed() => {
                        if result.is_err() {
                            return;
                        }
                        // Activité détectée — reboucler pour recalculer le temps restant
                    }
                    result = task_active_rx.changed() => {
                        if result.is_err() {
                            return;
                        }
                        // task_active a changé — reboucler pour réévaluer le statut
                    }
                }
            }
        });
    }
}

/// Construit la notification d'inactivité standard.
fn build_inactivity_notification() -> Notification {
    Notification {
        event: "agent.inactivity".into(),
        timestamp: chrono::Utc::now(),
        task_id: None,
        agent: None,
        message: "Apollia est en attente — une action de votre part est peut-être requise".into(),
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

    /// Canal mock qui compte les notifications reçues.
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

    /// Retourne `(handle, call_count, _bus_keep_alive)`.
    ///
    /// Le `_bus_keep_alive` DOIT être conservé dans la variable locale du test pour que
    /// le broadcast channel reste ouvert le temps du test. Sans lui, `run_engine_loop`
    /// reçoit `RecvError::Closed` dès son premier poll et s'arrête immédiatement.
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
        // On passe un clone pour que `run_engine_loop` puisse drop sa copie sans fermer le canal.
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
        // GIVEN InactivityWatcher avec timeout 1s, tâche active
        let (handle, call_count, _bus) = make_engine_with_counter();
        let watcher = Arc::new(InactivityWatcher::new(1, handle));
        let (_active_tx, active_rx) = watch::channel(true);
        watcher.clone().start(active_rx);

        // WHEN aucun reset_timer() pendant 1.1s
        tokio::time::sleep(Duration::from_millis(1100)).await;
        // Laisser l'engine consommer la commande Publish depuis son cmd_rx
        tokio::time::sleep(Duration::from_millis(50)).await;

        // THEN notification publiée
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn inactivity_watcher_resets_on_event() {
        // GIVEN InactivityWatcher avec timeout 1s
        let (handle, call_count, _bus) = make_engine_with_counter();
        let watcher = Arc::new(InactivityWatcher::new(1, handle));
        let (_active_tx, active_rx) = watch::channel(true);
        watcher.clone().start(active_rx);

        // WHEN reset_timer() à 800ms → attente 1.5s total
        tokio::time::sleep(Duration::from_millis(800)).await;
        watcher.reset_timer();
        tokio::time::sleep(Duration::from_millis(700)).await;

        // THEN aucune notification (timer réarmé à 800ms, nouveau timeout à 1800ms total)
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn inactivity_watcher_no_notification_when_task_inactive() {
        // GIVEN InactivityWatcher avec tâche inactive
        let (handle, call_count, _bus) = make_engine_with_counter();
        let watcher = Arc::new(InactivityWatcher::new(1, handle));
        let (_active_tx, active_rx) = watch::channel(false);
        watcher.clone().start(active_rx);

        // WHEN attente 1.5s sans que la tâche soit active
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // THEN aucune notification
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
