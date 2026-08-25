-- runtime_events.db as first shipped (schema v1, user_version 0).
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
CREATE INDEX idx_runtime_events_task_ts ON runtime_events(task_id, ts);

INSERT INTO runtime_events VALUES
    ('legacy-evt-1', 'task-1', 'agent-a', NULL, NULL, 1, 'step.completed', '{}', '2025-06-01T08:00:00Z', 1748764800);
