-- Migration 006 — pipeline_runs et pipeline_step_runs
-- Idempotente : CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS

-- Table principale des runs de pipeline.
-- Un run correspond à une exécution déclenchée manuellement (CLI/API)
-- ou par un trigger (Sprint 9).
CREATE TABLE IF NOT EXISTS pipeline_runs (
    run_id          TEXT PRIMARY KEY,
    pipeline_id     TEXT NOT NULL,
    trigger_id      TEXT,                    -- NULL si déclenché manuellement
    status          TEXT NOT NULL DEFAULT 'running',
                    -- running | waiting_approval | completed | failed
    failed_step     TEXT,                    -- step_id si status=failed
    trigger_payload TEXT,                    -- payload du trigger ou input CLI
    started_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ended_at        TIMESTAMP
);

-- État d'exécution de chaque step au sein d'un run.
-- La PRIMARY KEY composite (run_id, step_id) garantit l'unicité des entrées.
CREATE TABLE IF NOT EXISTS pipeline_step_runs (
    run_id               TEXT NOT NULL REFERENCES pipeline_runs(run_id),
    step_id              TEXT NOT NULL,
    task_id              TEXT,                        -- NULL si pending/skipped/fallback_active
    agent_name           TEXT NOT NULL,
    status               TEXT NOT NULL DEFAULT 'pending',
                         -- pending | running | waiting_approval | completed
                         -- failed | skipped | fallback_active | cancelled
    output               TEXT,
    error                TEXT,
    started_at           TIMESTAMP,
    ended_at             TIMESTAMP,
    approved_by          TEXT,                        -- NULL pour les steps non-HITL
    approval_duration_ms INTEGER,                     -- durée d'attente HITL en ms
    PRIMARY KEY (run_id, step_id)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_runs_pipeline_id
    ON pipeline_runs(pipeline_id);
CREATE INDEX IF NOT EXISTS idx_pipeline_runs_status
    ON pipeline_runs(status);
CREATE INDEX IF NOT EXISTS idx_pipeline_step_runs_run_id
    ON pipeline_step_runs(run_id);
CREATE INDEX IF NOT EXISTS idx_pipeline_step_runs_task_id
    ON pipeline_step_runs(task_id);
