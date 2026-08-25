CREATE TABLE audit_journal_entries (
    seq         INTEGER NOT NULL,
    run_id      TEXT    NOT NULL,
    ts          TEXT    NOT NULL,
    kind        TEXT    NOT NULL,
    payload     TEXT    NOT NULL,
    prev_hash   TEXT    NOT NULL,
    hash        TEXT    NOT NULL, signature      TEXT, signing_key_id TEXT, global_seq            INTEGER, global_prev_hash      TEXT, global_hash           TEXT, global_signature      TEXT, global_signing_key_id TEXT,
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
CREATE UNIQUE INDEX idx_aje_global_seq ON audit_journal_entries(global_seq) WHERE global_seq IS NOT NULL;
