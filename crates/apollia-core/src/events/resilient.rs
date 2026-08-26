//! Lag-resilient subscription to the `EventBus`.
//!
//! `broadcast::Receiver::recv` answers `Lagged(n)` when a subscriber falls
//! behind the ring buffer. The contract of this runtime, stated in
//! `crates/apollia-runtime/AGENTS.md` section 2 and in
//! `docs/agents/OBSERVABILITY.md`, is: log at `WARN`, `resubscribe()`, and
//! continue, never panic.
//!
//! Thirteen subscriber loops implemented the first and the third of those, and
//! not one called `resubscribe()`. The difference is not cosmetic. After a lag
//! the receiver resumes on the oldest message the ring still holds, which a
//! subscriber that already fell behind will not catch up with while the
//! producer keeps its pace, so it lags again, and again, one `WARN` per round.
//! `resubscribe()` moves it to the tail once: the backlog it was never going to
//! catch is dropped deliberately, named in the log line, and reception resumes
//! at the current pace.
//!
//! The rule lives here rather than at each call site, because a rule restated
//! thirteen times is a rule thirteen files can drift from, and this one had.

use tokio::sync::broadcast::error::RecvError;

use crate::events::{EventBusSender, RuntimeEvent};

/// What one reception produced, for a subscriber that reacts to lag itself.
#[derive(Debug)]
pub enum Received {
    /// An event, in bus order.
    Event(RuntimeEvent),
    /// The reader fell behind: `skipped` events were dropped, the `WARN` has
    /// been logged and the receiver has resubscribed to the tail.
    ///
    /// Most loops never see this: [`ResilientReceiver::recv`] absorbs it. It
    /// exists for the subscriber that waits on one specific event and has
    /// another way of finding out, such as the A2A invoker asking the router
    /// for the task output directly.
    Lagged {
        /// Number of events the ring dropped before this reader reached them.
        skipped: u64,
    },
}

/// Receiver that applies the lag rule of the `EventBus` contract.
///
/// Obtained through [`subscribe_resilient`]. [`ResilientReceiver::recv`] hides
/// lag entirely and answers `None` only when the bus closes, so a subscriber
/// loop reads `while let Some(event) = rx.recv().await`.
pub struct ResilientReceiver {
    receiver: tokio::sync::broadcast::Receiver<RuntimeEvent>,
    subscriber: &'static str,
}

impl ResilientReceiver {
    /// Next event, or `None` once the bus is closed. Lag is absorbed.
    ///
    /// Cancel-safe: the only await is `broadcast::Receiver::recv`, which is
    /// itself cancel-safe, and no state is held across it. The call is
    /// therefore usable as a `tokio::select!` branch.
    pub async fn recv(&mut self) -> Option<RuntimeEvent> {
        loop {
            match self.recv_reporting_lag().await? {
                Received::Event(event) => return Some(event),
                Received::Lagged { .. } => continue,
            }
        }
    }

    /// Next event or a lag report, or `None` once the bus is closed.
    ///
    /// The `WARN` and the `resubscribe()` have already happened when
    /// [`Received::Lagged`] comes back: the caller decides what else to do,
    /// never whether the rule applies.
    ///
    /// Cancel-safe, for the same reason as [`ResilientReceiver::recv`].
    pub async fn recv_reporting_lag(&mut self) -> Option<Received> {
        match self.receiver.recv().await {
            Ok(event) => Some(Received::Event(event)),
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(subscriber = self.subscriber, skipped, "eventbus.lagged");
                self.receiver = self.receiver.resubscribe();
                Some(Received::Lagged { skipped })
            }
            Err(RecvError::Closed) => {
                tracing::info!(subscriber = self.subscriber, "eventbus.closed");
                None
            }
        }
    }
}

/// Subscribe to the bus under the lag rule of the `EventBus` contract.
///
/// `subscriber` names the loop in the logs; it is a static string so a lag line
/// always says which reader fell behind.
pub fn subscribe_resilient(bus: &EventBusSender, subscriber: &'static str) -> ResilientReceiver {
    ResilientReceiver {
        receiver: bus.subscribe(),
        subscriber,
    }
}

/// Wrap a receiver somebody else already obtained, under the same rule.
///
/// Used where the receiver is handed in rather than subscribed for, such as the
/// audit journal subscriber, which is spawned with one.
pub fn resilient(
    receiver: tokio::sync::broadcast::Receiver<RuntimeEvent>,
    subscriber: &'static str,
) -> ResilientReceiver {
    ResilientReceiver {
        receiver,
        subscriber,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn test_lag_resubscribes_to_the_tail_instead_of_the_backlog() {
        // GIVEN a bus of capacity 2 whose reader has fallen three behind
        let (tx, _rx0) = broadcast::channel::<RuntimeEvent>(2);
        let mut rx = resilient(tx.subscribe(), "test");
        for _ in 0..5 {
            let _ = tx.send(RuntimeEvent::AllReady);
        }

        // WHEN the reader receives
        let first = rx.recv_reporting_lag().await.expect("open bus");

        // THEN the lag is reported, and the retained backlog was dropped with
        // it: the next event is the one sent after the resubscribe, not one of
        // the two the ring still held.
        assert!(
            matches!(first, Received::Lagged { skipped: 3 }),
            "{first:?}"
        );
        let _ = tx.send(RuntimeEvent::ShutdownRequested);
        let next = rx.recv().await.expect("open bus");
        assert!(matches!(next, RuntimeEvent::ShutdownRequested), "{next:?}");
    }

    #[tokio::test]
    async fn test_recv_absorbs_lag_and_none_means_closed() {
        // GIVEN a bus of capacity 2 whose reader has fallen three behind
        let (tx, _rx0) = broadcast::channel::<RuntimeEvent>(2);
        let mut rx = resilient(tx.subscribe(), "test");
        for _ in 0..5 {
            let _ = tx.send(RuntimeEvent::AllReady);
        }

        // WHEN the plain loop reads, while one more event is published. The
        // join is what makes this deterministic without a sleep: `recv` is
        // polled first, absorbs the lag, registers its waker on the
        // resubscribed receiver, and the send that follows wakes it.
        let sender = tx.clone();
        let (event, ()) = tokio::join!(rx.recv(), async move {
            let _ = sender.send(RuntimeEvent::ShutdownRequested);
        });

        // THEN the lag never surfaced, and closing the bus ends the loop
        assert!(
            matches!(event, Some(RuntimeEvent::ShutdownRequested)),
            "{event:?}"
        );
        drop(tx);
        assert!(rx.recv().await.is_none());
    }
}
