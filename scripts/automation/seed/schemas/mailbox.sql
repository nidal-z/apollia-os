-- Schema for the durable inter-agent mailbox store (mailbox.db).
-- Mirrors the runtime schema owned by the mailbox actor
-- (crates/apollia-runtime/src/mailbox.rs). The builder applies this file, then
-- the matching fragment (fragments/mailbox.sql) inserts deterministic rows.
--
-- Unblocks the mailbox-det automation script: the desktop
-- list_mailbox_messages command reads mailbox_messages ORDER BY seq DESC, so the
-- Observability > Mailbox tab renders populated rows instead of the empty state.
CREATE TABLE IF NOT EXISTS mailbox_messages (
    message_id       TEXT PRIMARY KEY,
    to_agent         TEXT    NOT NULL,
    from_agent       TEXT    NOT NULL,
    payload          TEXT    NOT NULL,
    sent_at          TEXT    NOT NULL,
    created_unix     INTEGER NOT NULL,
    state            TEXT    NOT NULL,
    lease_until_unix INTEGER,
    lease_owner      TEXT,
    seq              INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mailbox_to_seq
    ON mailbox_messages(to_agent, seq ASC);
