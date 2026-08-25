-- notifications.db as a pre-versioning binary wrote it (user_version = 0):
-- the three tables before the label and min_interval_seconds columns on
-- notification_channels. Frozen fixture: do not update it when the live
-- schema evolves, it is the "old format" the migration tests open.
CREATE TABLE IF NOT EXISTS notification_channels (
    id              TEXT PRIMARY KEY,
    channel_type    TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    config_json     TEXT NOT NULL DEFAULT '{}',
    events_json     TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (channel_type IN ('desktop', 'webhook'))
);

CREATE TABLE IF NOT EXISTS notification_global_events (
    event_name      TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS notification_logs (
    id              TEXT PRIMARY KEY,
    event_name      TEXT NOT NULL,
    task_id         TEXT,
    agent_id        TEXT,
    sent_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    channels        TEXT NOT NULL DEFAULT '{}',
    error           TEXT
);

CREATE INDEX IF NOT EXISTS idx_notif_logs_sent_at
    ON notification_logs(sent_at);
