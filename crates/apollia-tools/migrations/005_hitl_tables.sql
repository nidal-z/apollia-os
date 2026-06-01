-- Migration 005 - table tasks + table task_approvals (HITL)
-- Idempotente : CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS

-- Table tasks : créée pour la première fois dans cette migration.
-- Stocke l'état HITL (input_required_* + input_response_*) de chaque tâche.
CREATE TABLE IF NOT EXISTS tasks (
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
);

-- Historique complet des suspensions (multi-approval pour Mode Orchestré).
CREATE TABLE IF NOT EXISTS task_approvals (
    id           TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(8)))),
    task_id      TEXT NOT NULL,
    step_id      TEXT,               -- NULL si Mode Direct
    prompt       TEXT NOT NULL,
    context_json TEXT NOT NULL,
    approved     BOOLEAN,            -- NULL tant qu'en attente
    reason       TEXT,
    requested_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    responded_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_task_approvals_task_id ON task_approvals(task_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status           ON tasks(status);
