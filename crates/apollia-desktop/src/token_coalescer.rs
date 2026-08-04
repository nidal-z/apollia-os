//! Frame-rate coalescing for streamed chat tokens.
//!
//! The runtime publishes one `RuntimeEvent::ChatToken` per token. Forwarding
//! each one to the webview costs a full IPC hop: on Windows every
//! `app.emit` becomes an `ICoreWebView2::ExecuteScript` dispatched onto the UI
//! thread, and the webview then re-derives the whole in-flight answer on each
//! one. Both costs scale with the answer length, so a long turn ends up paying
//! quadratic work for a stream the eye samples at screen refresh anyway.
//!
//! This buffer holds tokens per streamed message and hands them back in
//! arrival order when the bridge decides to flush. Consumers concatenate the
//! `token` field, so a chunk carrying several tokens is indistinguishable from
//! the tokens that composed it.
//!
//! Ordering is the invariant that matters. Everything the frontend derives from
//! the accumulated buffer at the moment a non-token event lands, the live
//! reasoning cursor attached to a starting tool call above all, is only correct
//! if every token that preceded that event has already been delivered. The
//! bridge therefore drains before forwarding anything else, and
//! [`TokenCoalescer::drain`] preserves the order messages first received a
//! token in.

use crate::events::ChatTokenPayload;

/// One streamed assistant message with tokens waiting to be delivered.
struct Pending {
    session_id: String,
    message_id: String,
    text: String,
}

/// Accumulates streamed tokens between two flushes.
///
/// Keyed by `(session_id, message_id)`: two sessions streaming at once keep
/// separate buffers, and a new assistant message never inherits the tail of the
/// previous one.
#[derive(Default)]
pub struct TokenCoalescer {
    /// Pending messages in the order they first received a token.
    pending: Vec<Pending>,
}

impl TokenCoalescer {
    /// Empty coalescer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Buffer one streamed token.
    pub fn push(&mut self, session_id: &str, message_id: &str, token: &str) {
        if let Some(entry) = self
            .pending
            .iter_mut()
            .find(|p| p.session_id == session_id && p.message_id == message_id)
        {
            entry.text.push_str(token);
            return;
        }
        self.pending.push(Pending {
            session_id: session_id.to_owned(),
            message_id: message_id.to_owned(),
            text: token.to_owned(),
        });
    }

    /// True when nothing is waiting, so a tick can skip the flush entirely.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Take everything buffered, one payload per streamed message, in the order
    /// each message first produced a token.
    pub fn drain(&mut self) -> Vec<ChatTokenPayload> {
        self.pending
            .drain(..)
            .map(|p| ChatTokenPayload {
                session_id: p.session_id,
                message_id: p.message_id,
                token: p.text,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drain_concatenates_tokens_in_arrival_order() {
        // GIVEN three tokens streamed for one message
        let mut coalescer = TokenCoalescer::new();
        coalescer.push("s1", "m1", "Bon");
        coalescer.push("s1", "m1", "jour");
        coalescer.push("s1", "m1", " !");

        // WHEN the coalescer is drained
        let payloads = coalescer.drain();

        // THEN a single payload carries the tokens concatenated in order
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].session_id, "s1");
        assert_eq!(payloads[0].message_id, "m1");
        assert_eq!(payloads[0].token, "Bonjour !");
    }

    #[test]
    fn test_two_sessions_never_cross_talk() {
        // GIVEN two sessions streaming interleaved tokens
        let mut coalescer = TokenCoalescer::new();
        coalescer.push("s1", "m1", "a");
        coalescer.push("s2", "m2", "x");
        coalescer.push("s1", "m1", "b");
        coalescer.push("s2", "m2", "y");

        // WHEN the coalescer is drained
        let payloads = coalescer.drain();

        // THEN each session gets its own payload, in first-token order
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0].session_id, "s1");
        assert_eq!(payloads[0].token, "ab");
        assert_eq!(payloads[1].session_id, "s2");
        assert_eq!(payloads[1].token, "xy");
    }

    #[test]
    fn test_two_messages_in_one_session_stay_separate() {
        // GIVEN one session streaming two successive assistant messages
        let mut coalescer = TokenCoalescer::new();
        coalescer.push("s1", "m1", "first");
        coalescer.push("s1", "m2", "second");

        // WHEN the coalescer is drained
        let payloads = coalescer.drain();

        // THEN the second message does not inherit the first one's tail
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0].message_id, "m1");
        assert_eq!(payloads[0].token, "first");
        assert_eq!(payloads[1].message_id, "m2");
        assert_eq!(payloads[1].token, "second");
    }

    #[test]
    fn test_drain_of_an_idle_coalescer_emits_nothing() {
        // GIVEN a coalescer that received no token
        let mut coalescer = TokenCoalescer::new();

        // WHEN it is drained
        let payloads = coalescer.drain();

        // THEN nothing is emitted, so an idle tick costs no IPC
        assert!(coalescer.is_empty());
        assert!(payloads.is_empty());
    }

    #[test]
    fn test_drain_leaves_the_buffer_empty() {
        // GIVEN a drained coalescer
        let mut coalescer = TokenCoalescer::new();
        coalescer.push("s1", "m1", "a");
        let _ = coalescer.drain();

        // WHEN it is drained again
        let payloads = coalescer.drain();

        // THEN the tokens are not delivered twice
        assert!(payloads.is_empty());
    }
}
