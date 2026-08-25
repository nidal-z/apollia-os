-- governance.db as a pre-versioning binary wrote it (user_version = 0):
-- the legacy permissions.db shape, before the scope / project_path /
-- agent_id / expires_at columns on permission_rules and the scope /
-- rule_id / agent columns on permission_audit, and before the
-- tools / tool_credentials / chat_libre_config tables joined the file.
-- Frozen fixture: do not update it when the live schema evolves, it is
-- the "old format" the migration tests open.
CREATE TABLE IF NOT EXISTS permission_rules (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name    TEXT NOT NULL,
    arg_prefix   TEXT,
    action       TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    created_by   TEXT
);
CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);

CREATE TABLE IF NOT EXISTS permission_audit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name   TEXT NOT NULL,
    first_arg   TEXT,
    decision    TEXT NOT NULL,
    decided_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_tool ON permission_audit(tool_name, decided_at);
