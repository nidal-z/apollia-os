CREATE TABLE permission_rules (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name    TEXT NOT NULL,
            arg_prefix   TEXT,
            action       TEXT NOT NULL,
            created_at   INTEGER NOT NULL,
            created_by   TEXT,
            scope        TEXT NOT NULL DEFAULT 'global',
            project_path TEXT,
            expires_at   INTEGER
        , "agent_id" TEXT);
CREATE TABLE sqlite_sequence(name,seq);
CREATE INDEX idx_rules_tool ON permission_rules(tool_name);
CREATE TABLE permission_audit (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name   TEXT NOT NULL,
            first_arg   TEXT,
            decision    TEXT NOT NULL,
            decided_at  INTEGER NOT NULL,
            scope       TEXT,
            rule_id     INTEGER,
            agent       TEXT
        );
CREATE INDEX idx_audit_tool ON permission_audit(tool_name, decided_at);
CREATE TABLE tools (
            name        TEXT PRIMARY KEY,
            enabled     BOOLEAN NOT NULL DEFAULT TRUE,
            config_json TEXT,
            updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
        );
CREATE TABLE tool_credentials (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name       TEXT NOT NULL,
            key_name        TEXT NOT NULL,
            value_encrypted BLOB NOT NULL,
            created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used_at    INTEGER,
            UNIQUE(tool_name, key_name)
        );
CREATE TABLE chat_libre_config (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            system_prompt   TEXT NOT NULL DEFAULT '',
            allowed_tools   TEXT NOT NULL DEFAULT '[]',
            llm_backend     TEXT,
            updated_at      TEXT NOT NULL
                            DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
        );
CREATE TRIGGER no_update_audit
         BEFORE UPDATE ON permission_audit BEGIN
             SELECT RAISE(ABORT, 'permission_audit is append-only');
         END;
CREATE TRIGGER no_delete_audit
         BEFORE DELETE ON permission_audit BEGIN
             SELECT RAISE(ABORT, 'permission_audit is append-only');
         END;
CREATE INDEX idx_rules_scope_project
                ON permission_rules(scope, project_path);
