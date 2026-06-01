-- Migration 004 - execution_plans et plan_steps
-- Idempotente : utilise CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS

CREATE TABLE IF NOT EXISTS execution_plans (
    plan_id      TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL UNIQUE,
    agent_name   TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
                 -- running | completed | failed | replanning
    replan_count INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS plan_steps (
    step_id      TEXT NOT NULL,
    plan_id      TEXT NOT NULL REFERENCES execution_plans(plan_id),
    description  TEXT NOT NULL,
    tool_hint    TEXT,
    depends_on   TEXT NOT NULL DEFAULT '[]',  -- JSON array de step_ids
    status       TEXT NOT NULL DEFAULT 'pending',
                 -- pending | running | completed | failed | skipped
    output       TEXT,         -- NULL si pas encore complété
    error        TEXT,         -- NULL si succès
    started_at   TIMESTAMP,
    completed_at TIMESTAMP,
    PRIMARY KEY (step_id, plan_id)
);

CREATE INDEX IF NOT EXISTS idx_plan_steps_plan_id
    ON plan_steps(plan_id);
CREATE INDEX IF NOT EXISTS idx_execution_plans_task_id
    ON execution_plans(task_id);
