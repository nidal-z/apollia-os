CREATE TABLE tasks (
    task_id                  TEXT PRIMARY KEY,
    agent_name               TEXT NOT NULL DEFAULT '',
    status                   TEXT NOT NULL DEFAULT 'input_required',
    step_id                  TEXT,                 -- NULL si Mode Direct
    input_required_prompt    TEXT,
    input_required_context   TEXT,                 -- JSON sérialisé
    input_required_at        TIMESTAMP,
    input_response_approved  BOOLEAN,
    input_response_reason    TEXT,
    input_response_at        TIMESTAMP,
    created_at               TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at               TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
, input_text TEXT, input_truncated INTEGER NOT NULL DEFAULT 0, output_text TEXT, output_truncated INTEGER NOT NULL DEFAULT 0, duration_ms INTEGER, transitions_json TEXT, run_id TEXT);
CREATE TABLE task_approvals (
    id           TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(8)))),
    task_id      TEXT NOT NULL,
    step_id      TEXT,               -- NULL si Mode Direct
    prompt       TEXT NOT NULL,
    context_json TEXT NOT NULL,
    approved     BOOLEAN,            -- NULL tant qu'en attente
    reason       TEXT,
    requested_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    responded_at TIMESTAMP
, suspended_at TEXT, wait_duration_ms INTEGER);
CREATE INDEX idx_task_approvals_task_id ON task_approvals(task_id);
CREATE INDEX idx_tasks_status           ON tasks(status);
CREATE INDEX idx_task_approvals_pending ON task_approvals(task_id) WHERE approved IS NULL;
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
