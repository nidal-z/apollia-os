CREATE TABLE chat_artifacts (
    id                 TEXT PRIMARY KEY,
    session_id         TEXT NOT NULL,
    source_message_id  TEXT,
    kind               TEXT NOT NULL,
    language           TEXT,
    source_tool        TEXT,
    title              TEXT NOT NULL,
    content            TEXT NOT NULL,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);
CREATE INDEX idx_chat_artifacts_session
    ON chat_artifacts(session_id, created_at DESC);
CREATE INDEX idx_chat_artifacts_source_message
    ON chat_artifacts(source_message_id);
