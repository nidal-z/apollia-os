-- chat.db as the first shipped binary wrote it (schema v1, user_version 0):
-- no llm_backend / summary / title / forking / project / plan columns, no
-- FTS index, no approval log, and a mode CHECK without 'companion'.
CREATE TABLE chat_sessions (
    id         TEXT PRIMARY KEY,
    mode       TEXT NOT NULL CHECK (mode IN ('libre', 'agent')),
    agent_name TEXT,
    system_prompt TEXT NOT NULL DEFAULT '',
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'processing', 'closed')),
    available_tools TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    closed_at  TEXT
);

CREATE TABLE chat_messages (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES chat_sessions(id),
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content         TEXT NOT NULL DEFAULT '',
    tool_calls_json TEXT,
    tool_name       TEXT,
    created_at      TEXT NOT NULL,
    seq             INTEGER NOT NULL
);

CREATE TABLE chat_tool_authorizations (
    session_id    TEXT NOT NULL REFERENCES chat_sessions(id),
    tool_name     TEXT NOT NULL,
    authorized_at TEXT NOT NULL,
    PRIMARY KEY (session_id, tool_name)
);

CREATE INDEX idx_chat_messages_session ON chat_messages(session_id, seq);
CREATE INDEX idx_chat_sessions_status ON chat_sessions(status);

INSERT INTO chat_sessions VALUES
    ('legacy-session', 'libre', NULL, 'you are helpful', 'closed', '[]', '2025-06-01T08:00:00Z', '2025-06-01T09:00:00Z');
INSERT INTO chat_messages VALUES
    ('legacy-msg-1', 'legacy-session', 'user', 'hello', NULL, NULL, '2025-06-01T08:00:01Z', 1),
    ('legacy-msg-2', 'legacy-session', 'assistant', 'hi there', NULL, NULL, '2025-06-01T08:00:02Z', 2);
INSERT INTO chat_tool_authorizations VALUES
    ('legacy-session', 'fs_read', '2025-06-01T08:30:00Z');
