-- Schema for audit.db, the append-only tool-invocation trail.
--
-- The runtime creates this table itself on first boot, so the seed did not ship
-- a schema and the database arrived empty. That left two published screens
-- showing their empty state: Observability > Audit Trail ("No tool invocations
-- recorded", every KPI at zero) and Observability > Timeline, which reads this
-- table through scan_audit_db and had nothing to show inside its window.
--
-- Mirrors what the runtime creates, including the append-only triggers: a seed
-- that diverges from the real schema would let a screen render here and fail on
-- a real machine.

CREATE TABLE tool_invocations (
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
    resources_used  TEXT,
    args_json       TEXT,
    stdout          TEXT,
    stderr          TEXT,
    run_id          TEXT
);

CREATE INDEX idx_tool_invocations_agent_id
    ON tool_invocations(agent_id);
CREATE INDEX idx_tool_invocations_started_at
    ON tool_invocations(started_at);

CREATE TRIGGER audit_no_update
    BEFORE UPDATE ON tool_invocations
    BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
CREATE TRIGGER audit_no_delete
    BEFORE DELETE ON tool_invocations
    BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
