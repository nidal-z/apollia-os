-- audit_journal.db as first shipped (schema v1, user_version 0): no
-- signature columns, no global-chain columns, no global index.
CREATE TABLE audit_journal_entries (
    seq         INTEGER NOT NULL,
    run_id      TEXT    NOT NULL,
    ts          TEXT    NOT NULL,
    kind        TEXT    NOT NULL,
    payload     TEXT    NOT NULL,
    prev_hash   TEXT    NOT NULL,
    hash        TEXT    NOT NULL,
    PRIMARY KEY (run_id, seq)
);
CREATE INDEX idx_aje_run_id_seq
    ON audit_journal_entries(run_id, seq ASC);
CREATE TRIGGER aje_no_update
    BEFORE UPDATE ON audit_journal_entries
    BEGIN SELECT RAISE(ABORT, 'audit journal is append-only'); END;
CREATE TRIGGER aje_no_delete
    BEFORE DELETE ON audit_journal_entries
    BEGIN SELECT RAISE(ABORT, 'audit journal is append-only'); END;
CREATE TABLE audit_journal_state (
    id          INTEGER PRIMARY KEY CHECK (id = 0),
    global_seq  INTEGER NOT NULL,
    global_hash TEXT    NOT NULL,
    updated_ts  TEXT    NOT NULL
);

INSERT INTO audit_journal_entries VALUES
    (1, 'legacy-run', '2025-06-01T08:00:00Z', 'tool_call_started', '{}', 'GENESIS', 'hash-1'),
    (2, 'legacy-run', '2025-06-01T08:00:01Z', 'tool_call_completed', '{}', 'hash-1', 'hash-2');
