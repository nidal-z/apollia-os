CREATE TABLE llm_backends (
    name         TEXT PRIMARY KEY,
    provider     TEXT NOT NULL,
    model        TEXT NOT NULL,
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 1,
    is_default   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (provider IN ('llama-cpp', 'openai', 'mistral', 'anthropic', 'ollama'))
);
CREATE TABLE stt_config (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    enabled              INTEGER NOT NULL DEFAULT 0,
    model_path           TEXT    NOT NULL DEFAULT '',
    hotkey               TEXT    NOT NULL DEFAULT 'ctrl+shift+space',
    clipboard_mode       TEXT    NOT NULL DEFAULT 'paste',
    clipboard_restore    INTEGER NOT NULL DEFAULT 1,
    silence_threshold_db REAL    NOT NULL DEFAULT -40.0,
    max_recording_sec    INTEGER NOT NULL DEFAULT 60,
    language             TEXT,
    trigger_mode         TEXT    NOT NULL DEFAULT 'toggle',
    updated_at           TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
-- Folder connections (migration v9), created here so the seed can name the
-- checkout as a folder the Apollia agent is granted; the runtime's own
-- migration creates it IF NOT EXISTS and keeps the row.
CREATE TABLE folder_connections (
    id       TEXT PRIMARY KEY CHECK (length(trim(id)) > 0),
    path     TEXT NOT NULL UNIQUE CHECK (length(trim(path)) > 0),
    label    TEXT NOT NULL CHECK (length(trim(label)) > 0),
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
