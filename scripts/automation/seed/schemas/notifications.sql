CREATE TABLE notification_channels (
            id              TEXT PRIMARY KEY,
            channel_type    TEXT NOT NULL,
            enabled         BOOLEAN NOT NULL DEFAULT 1,
            config_json     TEXT NOT NULL DEFAULT '{}',
            events_json     TEXT,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), label TEXT, min_interval_seconds INTEGER NOT NULL DEFAULT 0,
            CHECK (channel_type IN ('desktop', 'webhook'))
        );
CREATE TABLE notification_global_events (
            event_name      TEXT PRIMARY KEY
        );
CREATE TABLE notification_logs (
            id              TEXT PRIMARY KEY,
            event_name      TEXT NOT NULL,
            task_id         TEXT,
            agent_id        TEXT,
            sent_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            channels        TEXT NOT NULL DEFAULT '{}',
            error           TEXT
        );
CREATE INDEX idx_notif_logs_sent_at
            ON notification_logs(sent_at);
