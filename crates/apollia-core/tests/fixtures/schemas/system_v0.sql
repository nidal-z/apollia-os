-- system.db as a pre-versioning binary wrote it (user_version = 0):
-- llm_backends at its initial shape, stt_config before the additive
-- input_device column. Frozen fixture: do not update it when the live
-- schema evolves, it is the "old format" the migration tests open.
CREATE TABLE IF NOT EXISTS llm_backends (
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

CREATE TABLE IF NOT EXISTS stt_config (
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
