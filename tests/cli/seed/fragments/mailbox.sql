-- Deterministic seed fragment for the inter-agent mailbox (mailbox.db).
-- Schema is applied separately by the builder (schemas/mailbox.sql).
--
-- Seeds three durable messages between two seed agents (apollia-guide and
-- seed-classifier, both present under seed/files/agents). A monotonic seq
-- (1, 2, 3) keeps the ordering deterministic: list_mailbox_messages returns
-- them newest first (seq DESC), so mailbox-det asserts a stable first row.
--
-- sent_at and created_unix are taken at build time, a few seconds apart in
-- seq order; see the note above the INSERT for why they cannot be fixed.
-- Timestamps are relative to the build: the mailbox actor evicts a message
-- past its TTL (86 400 s, crates/apollia-runtime/src/mailbox.rs) at the first
-- sweep, so rows dated at a fixed past instant were gone before any surface
-- read them. Measured on 2026-09-06: mailbox.db held 0 rows after boot.
INSERT INTO mailbox_messages
    (message_id, to_agent, from_agent, payload, sent_at, created_unix, state, lease_until_unix, lease_owner, seq)
VALUES
    ('seed-msg-1', 'seed-classifier', 'apollia-guide',
     '{"kind":"classify_request","document":"invoice-2026-07.pdf"}',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-30 seconds'), strftime('%s', 'now') - 30, 'pending', NULL, NULL, 1),
    ('seed-msg-2', 'apollia-guide', 'seed-classifier',
     '{"kind":"classify_result","label":"invoice","confidence":0.97}',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-25 seconds'), strftime('%s', 'now') - 25, 'pending', NULL, NULL, 2),
    ('seed-msg-3', 'seed-classifier', 'apollia-guide',
     '{"kind":"ack","note":"filed under accounting/2026-Q3"}',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-20 seconds'), strftime('%s', 'now') - 20, 'pending', NULL, NULL, 3);
