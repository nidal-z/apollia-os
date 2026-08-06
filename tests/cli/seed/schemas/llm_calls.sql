CREATE TABLE llm_calls (
    id                TEXT PRIMARY KEY,
    task_id           TEXT,
    step_id           TEXT,
    backend           TEXT NOT NULL,
    model             TEXT NOT NULL,
    prompt_tokens     INTEGER,
    completion_tokens INTEGER,
    cost_usd          REAL,
    latency_ms        INTEGER,
    prompt_text       TEXT,
    completion_text   TEXT,
    created_at        TEXT NOT NULL
                      DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_llm_calls_task    ON llm_calls(task_id);
CREATE INDEX idx_llm_calls_created ON llm_calls(created_at);
