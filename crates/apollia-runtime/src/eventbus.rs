use apollia_core::RuntimeEvent;
use tokio::sync::broadcast;

/// Handle en écriture sur l'EventBus — clonable, partageable entre acteurs.
///
/// Chaque acteur reçoit un clone de ce sender à l'initialisation.
/// La publication est non-bloquante ; si le buffer est plein, l'envoi
/// retourne une erreur (les receivers lents recevront `RecvError::Lagged`).
pub type EventBusSender = broadcast::Sender<RuntimeEvent>;

/// Handle en lecture sur l'EventBus — un par acteur consommateur.
///
/// Obtenu soit via [`EventBus::new`] (premier receiver), soit via
/// `sender.subscribe()` pour les receivers suivants.
/// En cas de `RecvError::Lagged`, logger un warning et continuer —
/// jamais de panic.
pub type EventBusReceiver = broadcast::Receiver<RuntimeEvent>;

/// Point de création unique de l'EventBus du runtime.
///
/// Instancié une seule fois par le Supervisor.
/// Durant Sprint 1, instancié directement dans les tests.
pub struct EventBus;

impl EventBus {
    /// Crée un nouveau bus avec un buffer de `capacity` événements.
    ///
    /// Retourne le [`EventBusSender`] partageable et un premier [`EventBusReceiver`].
    /// Les receivers supplémentaires s'obtiennent via `sender.subscribe()`.
    ///
    /// La `capacity` doit être validée en amont via [`apollia_core::RuntimeConfig::validate`]
    /// (bornes : [64, 65536]). Cette fonction ne valide pas elle-même la valeur.
    ///
    /// Intentionnellement factory (pas un constructeur Self) : `EventBus` est
    /// un namespace sans état propre — il délègue entièrement au canal broadcast.
    pub fn with_capacity(capacity: usize) -> (EventBusSender, EventBusReceiver) {
        broadcast::channel(capacity)
    }

    /// Crée un nouveau bus avec le buffer par défaut de 1024 événements.
    ///
    /// Raccourci pour [`EventBus::with_capacity`]`(1024)`. Utile dans les tests
    /// et les contextes où la configuration n'est pas disponible.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> (EventBusSender, EventBusReceiver) {
        Self::with_capacity(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::RuntimeEvent;

    #[tokio::test]
    async fn test_ac1_publication_reception() {
        // GIVEN
        let (tx, mut rx) = EventBus::new();

        // WHEN
        tx.send(RuntimeEvent::AgentRegistered("agent-1".into()))
            .unwrap();

        // THEN
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentRegistered(id) if id == "agent-1"));
    }

    #[tokio::test]
    async fn test_ac2_multiple_consumers() {
        // GIVEN
        let (tx, mut rx1) = EventBus::new();
        let mut rx2 = tx.subscribe();
        let mut rx3 = tx.subscribe();

        // WHEN
        tx.send(RuntimeEvent::AllReady).unwrap();

        // THEN — les 3 consumers reçoivent
        assert!(matches!(rx1.recv().await.unwrap(), RuntimeEvent::AllReady));
        assert!(matches!(rx2.recv().await.unwrap(), RuntimeEvent::AllReady));
        assert!(matches!(rx3.recv().await.unwrap(), RuntimeEvent::AllReady));
    }

    #[tokio::test]
    async fn test_ac3_lagged_consumer_ne_panic_pas() {
        // GIVEN — buffer de 8 pour accélérer le test (comportement identique à 1024)
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(8);

        // WHEN — on envoie 9 messages sans consommer (buffer saturé)
        for i in 0..9u32 {
            let _ = tx.send(RuntimeEvent::StepExecuted {
                task_id: format!("task-{}", i).into(),
                step: i,
                tool: None,
            });
        }

        // THEN — RecvError::Lagged retourné, pas de panic
        let result = rx.recv().await;
        assert!(
            matches!(result, Err(broadcast::error::RecvError::Lagged(_))),
            "expected Lagged error, got: {:?}",
            result
        );

        // ET — le consumer peut continuer à recevoir les prochains événements
        // (vider d'abord les messages encore dans le buffer avant d'en envoyer un nouveau)
        loop {
            match rx.try_recv() {
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => panic!("unexpected error draining buffer: {:?}", e),
            }
        }
        tx.send(RuntimeEvent::AllReady).unwrap();
        let next = rx.recv().await.unwrap();
        assert!(matches!(next, RuntimeEvent::AllReady));
    }

    #[tokio::test]
    async fn test_sender_clone_is_independent() {
        // GIVEN — deux clones du sender publient sans interférence
        let (tx1, mut rx) = EventBus::new();
        let tx2 = tx1.clone();

        // WHEN
        tx1.send(RuntimeEvent::AgentRegistered("agent-a".into()))
            .unwrap();
        tx2.send(RuntimeEvent::AgentStopped("agent-b".into()))
            .unwrap();

        // THEN
        let e1 = rx.recv().await.unwrap();
        let e2 = rx.recv().await.unwrap();
        assert!(matches!(e1, RuntimeEvent::AgentRegistered(id) if id == "agent-a"));
        assert!(matches!(e2, RuntimeEvent::AgentStopped(id) if id == "agent-b"));
    }
}
