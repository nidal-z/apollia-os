-- audit.db as a pre-versioning binary wrote it (user_version = 0):
-- tool_invocations before the args_json / stdout / stderr / run_id
-- observability columns. Frozen fixture: do not update it when the live
-- schema evolves, it is the "old format" the migration tests open.
CREATE TABLE IF NOT EXISTS tool_invocations (
    id              TEXT PRIMARY KEY,
    agent_id        TEXT NOT NULL,
    task_id         TEXT NOT NULL,
    tool_name       TEXT NOT NULL,
    input_hash      TEXT NOT NULL,
    sandbox_profile TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    duration_ms     INTEGER,
    exit_code       INTEGER,
    success         INTEGER NOT NULL,
    error_code      TEXT,
    resources_used  TEXT
);
CREATE INDEX IF NOT EXISTS idx_tool_invocations_agent_id
    ON tool_invocations(agent_id);
CREATE INDEX IF NOT EXISTS idx_tool_invocations_started_at
    ON tool_invocations(started_at);
CREATE TRIGGER IF NOT EXISTS audit_no_update
    BEFORE UPDATE ON tool_invocations
    BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
CREATE TRIGGER IF NOT EXISTS audit_no_delete
    BEFORE DELETE ON tool_invocations
    BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
