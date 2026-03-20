-- Chat tables — Sprint 18 (STORY-198)
-- Persistance locale des sessions de chat et messages.

CREATE TABLE IF NOT EXISTS chat_sessions (
    id         TEXT PRIMARY KEY,
    mode       TEXT NOT NULL CHECK (mode IN ('libre', 'agent')),
    agent_name TEXT,
    system_prompt TEXT NOT NULL DEFAULT '',
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'processing', 'closed')),
    available_tools TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    closed_at  TEXT
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES chat_sessions(id),
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content         TEXT NOT NULL DEFAULT '',
    tool_calls_json TEXT,
    tool_name       TEXT,
    created_at      TEXT NOT NULL,
    seq             INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_tool_authorizations (
    session_id    TEXT NOT NULL REFERENCES chat_sessions(id),
    tool_name     TEXT NOT NULL,
    authorized_at TEXT NOT NULL,
    PRIMARY KEY (session_id, tool_name)
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id, seq);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_status ON chat_sessions(status);
