CREATE TABLE _schema_version (
        version INTEGER NOT NULL
    );
CREATE TABLE stt_transcriptions (
        id                  TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
        full_text           TEXT NOT NULL,
        language            TEXT,
        source              TEXT NOT NULL DEFAULT 'hotkey',
        audio_duration_ms   INTEGER NOT NULL DEFAULT 0,
        processing_time_ms  INTEGER NOT NULL DEFAULT 0,
        model_name          TEXT,
        created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        CHECK (source IN ('hotkey', 'file', 'api'))
    );
CREATE INDEX idx_stt_transcriptions_created
        ON stt_transcriptions(created_at DESC);
