CREATE TABLE trigger_history (
    id          TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(8)))),
    trigger_id  TEXT NOT NULL,
    agent_name  TEXT NOT NULL,
    fired_at    TIMESTAMP NOT NULL,
    task_id     TEXT,              -- NULL when skipped or on error
    status      TEXT NOT NULL,     -- 'fired' | 'skipped' | 'error'
    reason      TEXT,              -- reason when skipped or on error
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
, payload_json TEXT, dispatch_ms INTEGER);
CREATE TABLE trigger_state (
    trigger_id  TEXT PRIMARY KEY,
    last_fired  TIMESTAMP,
    fire_count  INTEGER NOT NULL DEFAULT 0,
    skip_count  INTEGER NOT NULL DEFAULT 0,
    enabled     BOOLEAN NOT NULL DEFAULT 1
);
CREATE INDEX idx_trigger_history_trigger_id ON trigger_history(trigger_id);
CREATE INDEX idx_trigger_history_fired_at   ON trigger_history(fired_at DESC);
