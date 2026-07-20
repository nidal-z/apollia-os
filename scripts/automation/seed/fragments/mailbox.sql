-- Deterministic seed fragment for the inter-agent mailbox (mailbox.db).
-- Schema is applied separately by the builder (schemas/mailbox.sql).
--
-- Seeds three durable messages between two seed agents (apollia-guide and
-- seed-classifier, both present under seed/files/agents). Fixed sent_at on
-- 2026-07-01 and a monotonic seq (1, 2, 3) keep the ordering deterministic:
-- list_mailbox_messages returns them newest first (seq DESC), so mailbox-det
-- asserts a stable first row.
--
-- created_unix is the Unix epoch for 2026-07-01T09:00:00Z plus each offset;
-- the exact value is irrelevant to the desktop projection (it selects sent_at),
-- it only satisfies the NOT NULL column.
INSERT INTO mailbox_messages
    (message_id, to_agent, from_agent, payload, sent_at, created_unix, state, lease_until_unix, lease_owner, seq)
VALUES
    ('seed-msg-1', 'seed-classifier', 'apollia-guide',
     '{"kind":"classify_request","document":"invoice-2026-07.pdf"}',
     '2026-07-01T09:00:00Z', 1782032400, 'pending', NULL, NULL, 1),
    ('seed-msg-2', 'apollia-guide', 'seed-classifier',
     '{"kind":"classify_result","label":"invoice","confidence":0.97}',
     '2026-07-01T09:00:05Z', 1782032405, 'pending', NULL, NULL, 2),
    ('seed-msg-3', 'seed-classifier', 'apollia-guide',
     '{"kind":"ack","note":"filed under accounting/2026-Q3"}',
     '2026-07-01T09:00:10Z', 1782032410, 'pending', NULL, NULL, 3);
