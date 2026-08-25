-- mailbox.db as written before the lease-owner fence (schema v1,
-- user_version 0): no lease_owner column.
CREATE TABLE mailbox_messages (
    message_id       TEXT PRIMARY KEY,
    to_agent         TEXT    NOT NULL,
    from_agent       TEXT    NOT NULL,
    payload          TEXT    NOT NULL,
    sent_at          TEXT    NOT NULL,
    created_unix     INTEGER NOT NULL,
    state            TEXT    NOT NULL,
    lease_until_unix INTEGER,
    seq              INTEGER NOT NULL
);
CREATE INDEX idx_mailbox_to_seq
    ON mailbox_messages(to_agent, seq ASC);

INSERT INTO mailbox_messages VALUES
    ('legacy-mail-1', 'agent-b', 'agent-a', '{"k":"v"}', '2025-06-01T08:00:00Z', 1748764800, 'pending', NULL, 1);
