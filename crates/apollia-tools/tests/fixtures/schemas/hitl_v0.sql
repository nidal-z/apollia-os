-- hitl.db as a pre-versioning binary wrote it (user_version = 0): the
-- 005_hitl_tables shape, before the tasks observability columns, the
-- task_approvals timing columns and the notification_logs table.
-- Frozen fixture: do not update it when the live schema evolves, it is
-- the "old format" the migration tests open.
CREATE TABLE IF NOT EXISTS tasks (
    task_id                  TEXT PRIMARY KEY,
    agent_name               TEXT NOT NULL DEFAULT '',
    status                   TEXT NOT NULL DEFAULT 'input_required',
    step_id                  TEXT,
    input_required_prompt    TEXT,
    input_required_context   TEXT,
    input_required_at        TIMESTAMP,
    input_response_approved  BOOLEAN,
    input_response_reason    TEXT,
    input_response_at        TIMESTAMP,
    created_at               TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at               TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS task_approvals (
    id           TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(8)))),
    task_id      TEXT NOT NULL,
    step_id      TEXT,
    prompt       TEXT NOT NULL,
    context_json TEXT NOT NULL,
    approved     BOOLEAN,
    reason       TEXT,
    requested_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    responded_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_task_approvals_task_id ON task_approvals(task_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status           ON tasks(status);
