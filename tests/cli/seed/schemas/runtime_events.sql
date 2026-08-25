CREATE TABLE runtime_events (
    event_id         TEXT PRIMARY KEY,
    task_id          TEXT NOT NULL,
    agent_id         TEXT NOT NULL,
    parent_event_id  TEXT,
    correlation_id   TEXT,
    step_num         INTEGER,
    kind             TEXT NOT NULL,
    payload_json     TEXT NOT NULL,
    ts               TEXT NOT NULL,
    created_at_unix  INTEGER NOT NULL
);
CREATE INDEX idx_runtime_events_task_ts
    ON runtime_events(task_id, ts);
CREATE INDEX idx_runtime_events_parent
    ON runtime_events(parent_event_id);
CREATE INDEX idx_runtime_events_correlation
    ON runtime_events(correlation_id);
CREATE INDEX idx_runtime_events_created_at
    ON runtime_events(created_at_unix);
CREATE TRIGGER runtime_events_no_update
BEFORE UPDATE ON runtime_events
BEGIN
    SELECT RAISE(ABORT, 'runtime_events is append-only');
END;
