CREATE TABLE execution_plans (
    plan_id      TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL UNIQUE,
    agent_name   TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
                 -- running | completed | failed | replanning
    replan_count INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE plan_steps (
    step_id      TEXT NOT NULL,
    plan_id      TEXT NOT NULL REFERENCES execution_plans(plan_id),
    description  TEXT NOT NULL,
    tool_hint    TEXT,
    depends_on   TEXT NOT NULL DEFAULT '[]',  -- JSON array of step_ids
    status       TEXT NOT NULL DEFAULT 'pending',
                 -- pending | running | completed | failed | skipped
    output       TEXT,         -- NULL while not yet completed
    error        TEXT,         -- NULL on success
    started_at   TIMESTAMP,
    completed_at TIMESTAMP, input_rendered TEXT, input_truncated INTEGER NOT NULL DEFAULT 0, output_text TEXT, output_truncated INTEGER NOT NULL DEFAULT 0, tool_used TEXT, error_detail TEXT, duration_ms INTEGER, rationale TEXT, provenance TEXT,
    PRIMARY KEY (step_id, plan_id)
);
CREATE INDEX idx_plan_steps_plan_id
    ON plan_steps(plan_id);
CREATE INDEX idx_execution_plans_task_id
    ON execution_plans(task_id);
