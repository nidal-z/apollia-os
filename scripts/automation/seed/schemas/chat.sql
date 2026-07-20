CREATE TABLE chat_messages (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES chat_sessions(id),
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content         TEXT NOT NULL DEFAULT '',
    tool_calls_json TEXT,
    tool_name       TEXT,
    created_at      TEXT NOT NULL,
    seq             INTEGER NOT NULL
, metadata TEXT);
CREATE TABLE chat_tool_authorizations (
    session_id    TEXT NOT NULL REFERENCES chat_sessions(id),
    tool_name     TEXT NOT NULL,
    authorized_at TEXT NOT NULL,
    PRIMARY KEY (session_id, tool_name)
);
CREATE INDEX idx_chat_messages_session ON chat_messages(session_id, seq);
CREATE VIRTUAL TABLE chat_sessions_fts USING fts5(
                session_id UNINDEXED,
                created_at UNINDEXED,
                summary
            )
/* chat_sessions_fts(session_id,created_at,summary) */;
CREATE TABLE IF NOT EXISTS 'chat_sessions_fts_data'(id INTEGER PRIMARY KEY, block BLOB);
CREATE TABLE IF NOT EXISTS 'chat_sessions_fts_idx'(segid, term, pgno, PRIMARY KEY(segid, term)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS 'chat_sessions_fts_content'(id INTEGER PRIMARY KEY, c0, c1, c2);
CREATE TABLE IF NOT EXISTS 'chat_sessions_fts_docsize'(id INTEGER PRIMARY KEY, sz BLOB);
CREATE TABLE IF NOT EXISTS 'chat_sessions_fts_config'(k PRIMARY KEY, v) WITHOUT ROWID;
CREATE TABLE chat_approval_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL,
                message_id  TEXT NOT NULL,
                tool_name   TEXT NOT NULL,
                decision    TEXT NOT NULL CHECK (decision IN ('accept', 'refuse', 'always_accept')),
                resolved_at TEXT NOT NULL
            , reason TEXT);
CREATE TABLE sqlite_sequence(name,seq);
CREATE INDEX idx_chat_approval_log_resolved
                ON chat_approval_log(resolved_at DESC);
CREATE TABLE IF NOT EXISTS "chat_sessions" (
                    id              TEXT PRIMARY KEY,
                    mode            TEXT NOT NULL CHECK (mode IN ('libre', 'agent', 'companion')),
                    agent_name      TEXT,
                    system_prompt   TEXT NOT NULL DEFAULT '',
                    status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'processing', 'closed')),
                    available_tools TEXT NOT NULL DEFAULT '[]',
                    created_at      TEXT NOT NULL,
                    closed_at       TEXT,
                    llm_backend     TEXT,
                    summary         TEXT,
                    title           TEXT,
                    parent_session_id TEXT REFERENCES "chat_sessions"(id),
                    fork_depth      INTEGER NOT NULL DEFAULT 0,
                    project_id      TEXT
                , plan_mode INTEGER NOT NULL DEFAULT 0, plan_phase TEXT NOT NULL DEFAULT 'done');
CREATE INDEX idx_chat_sessions_status ON chat_sessions(status);
CREATE INDEX idx_sessions_parent ON chat_sessions(parent_session_id);
CREATE INDEX idx_chat_sessions_project ON chat_sessions(project_id);
CREATE TABLE session_todos (
    id          TEXT NOT NULL,
    session_id  TEXT NOT NULL,
    content     TEXT NOT NULL,
    status      TEXT NOT NULL CHECK(status IN ('pending','in_progress','completed')),
    depends_on  TEXT NOT NULL DEFAULT '[]',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (id, session_id)
);
CREATE INDEX idx_session_todos_session ON session_todos(session_id);
CREATE TABLE session_plans (
    session_id  TEXT PRIMARY KEY,
    plan_id     TEXT NOT NULL,
    revision    INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL,
    summary     TEXT,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE session_plan_steps (
    session_id  TEXT NOT NULL,
    step_id     TEXT NOT NULL,
    ordinal     INTEGER NOT NULL,
    payload     TEXT NOT NULL,
    PRIMARY KEY (session_id, step_id)
);
CREATE TABLE session_plan_mutations (
    session_id  TEXT NOT NULL,
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    payload     TEXT NOT NULL
);
CREATE INDEX idx_session_plan_mut ON session_plan_mutations(session_id);
