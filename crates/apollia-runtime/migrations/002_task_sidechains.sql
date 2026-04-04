-- Sidechain logging — A2A task delegation traceability.

CREATE TABLE IF NOT EXISTS task_sidechains (
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

CREATE INDEX IF NOT EXISTS idx_task_sidechains_parent
ON task_sidechains(parent_task_id, sidechain_n ASC);
