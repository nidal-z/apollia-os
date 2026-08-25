-- sidechains.db as first shipped (schema v1, user_version 0).
CREATE TABLE task_sidechains (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_task_id  TEXT NOT NULL,
    sidechain_n     INTEGER NOT NULL,
    child_task_id   TEXT,
    agent_name      TEXT NOT NULL,
    input_summary   TEXT,
    output_summary  TEXT,
    status          TEXT NOT NULL DEFAULT 'running'
                    CHECK(status IN ('running', 'completed', 'failed')),
    started_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
    completed_at    DATETIME
);
CREATE INDEX idx_task_sidechains_parent
ON task_sidechains(parent_task_id, sidechain_n ASC);

INSERT INTO task_sidechains
    (parent_task_id, sidechain_n, child_task_id, agent_name, input_summary, output_summary, status)
VALUES
    ('parent-1', 1, 'child-1', 'agent-b', 'do the thing', 'done', 'completed');
