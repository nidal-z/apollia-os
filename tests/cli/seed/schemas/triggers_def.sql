CREATE TABLE trigger_definitions (
    id              TEXT PRIMARY KEY,
    agent           TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    on_busy         TEXT NOT NULL DEFAULT 'queue',
    source_type     TEXT NOT NULL,
    source_config   TEXT NOT NULL,
    input_template  TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (on_busy IN ('queue', 'drop')),
    CHECK (source_type IN ('cron', 'interval', 'oneshot', 'file_watch', 'webhook'))
);
