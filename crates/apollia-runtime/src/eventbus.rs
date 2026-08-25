use apollia_core::RuntimeEvent;
use tokio::sync::broadcast;

/// Write handle on the EventBus: clonable, shareable across actors.
///
/// Each actor receives a clone of this sender at initialization.
/// Publishing is non-blocking; if the buffer is full, the send returns
/// an error (slow receivers will get `RecvError::Lagged`).
pub type EventBusSender = broadcast::Sender<RuntimeEvent>;

/// Read handle on the EventBus, one per consumer actor.
///
/// Obtained either via [`EventBus::new`] (first receiver) or via
/// `sender.subscribe()` for subsequent receivers.
/// On `RecvError::Lagged`, log a warning and continue, never panic.
pub type EventBusReceiver = broadcast::Receiver<RuntimeEvent>;

/// Single creation point for the runtime EventBus.
///
/// Instantiated once by the Supervisor, and directly in tests.
pub struct EventBus;

impl EventBus {
    /// Creates a new bus with a buffer of `capacity` events.
    ///
    /// Returns the shareable [`EventBusSender`] and a first [`EventBusReceiver`].
    /// Additional receivers are obtained via `sender.subscribe()`.
    ///
    /// `capacity` must be validated beforehand via [`apollia_core::RuntimeConfig::validate`]
    /// (bounds: [64, 65536]). This function does not validate the value itself.
    ///
    /// Intentionally a factory (not a `Self` constructor): `EventBus` is a
    /// stateless namespace that delegates entirely to the broadcast channel.
    pub fn with_capacity(capacity: usize) -> (EventBusSender, EventBusReceiver) {
        broadcast::channel(capacity)
    }

    /// Creates a new bus with the default buffer of 1024 events.
    ///
    /// Shorthand for [`EventBus::with_capacity`]`(1024)`. Useful in tests and
    /// contexts where the configuration is not available.
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
    async fn test_publication_reception() {
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
    async fn test_multiple_consumers() {
        // GIVEN
        let (tx, mut rx1) = EventBus::new();
        let mut rx2 = tx.subscribe();
        let mut rx3 = tx.subscribe();

        // WHEN
        tx.send(RuntimeEvent::AllReady).unwrap();

        // THEN all 3 consumers receive the event
        assert!(matches!(rx1.recv().await.unwrap(), RuntimeEvent::AllReady));
        assert!(matches!(rx2.recv().await.unwrap(), RuntimeEvent::AllReady));
        assert!(matches!(rx3.recv().await.unwrap(), RuntimeEvent::AllReady));
    }

    #[tokio::test]
    async fn test_lagged_consumer_ne_panic_pas() {
        // GIVEN a buffer of 8 to speed up the test (same behavior as 1024)
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(8);

        // WHEN we send 9 messages without consuming (buffer saturated)
        for i in 0..9u32 {
            let _ = tx.send(RuntimeEvent::TaskCanceled {
                task_id: format!("task-{}", i).into(),
            });
        }

        // THEN RecvError::Lagged is returned, no panic
        let result = rx.recv().await;
        assert!(
            matches!(result, Err(broadcast::error::RecvError::Lagged(_))),
            "expected Lagged error, got: {:?}",
            result
        );

        // AND the consumer can keep receiving subsequent events
        // (drain the messages still in the buffer before sending a new one)
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
        // GIVEN two sender clones publishing without interference
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
