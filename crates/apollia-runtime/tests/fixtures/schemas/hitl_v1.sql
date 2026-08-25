-- hitl.db notification_logs as first shipped (schema v1, user_version 0).
CREATE TABLE notification_logs (
    id          TEXT    PRIMARY KEY,
    event_name  TEXT    NOT NULL,
    task_id     TEXT,
    agent_id    TEXT,
    sent_at     TEXT    NOT NULL DEFAULT (datetime('now')),
    channels    TEXT    NOT NULL DEFAULT '{}',
    error       TEXT
);
CREATE INDEX idx_notif_logs_sent_at ON notification_logs(sent_at);

INSERT INTO notification_logs VALUES
    ('legacy-notif-1', 'task.completed', 'task-1', 'agent-a', '2025-06-01T08:00:00Z', '{"desktop":"ok"}', NULL);
