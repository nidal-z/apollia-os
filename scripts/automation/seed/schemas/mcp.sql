CREATE TABLE mcp_servers (
                name               TEXT PRIMARY KEY,
                command            TEXT NOT NULL DEFAULT '',
                args_json          TEXT NOT NULL DEFAULT '[]',
                env_json           TEXT NOT NULL DEFAULT '{}',
                transport          TEXT NOT NULL DEFAULT 'stdio',
                url                TEXT,
                requires_approval  INTEGER NOT NULL DEFAULT 0,
                init_timeout_secs  INTEGER NOT NULL DEFAULT 30,
                call_timeout_secs  INTEGER NOT NULL DEFAULT 60,
                tags_json          TEXT NOT NULL DEFAULT '[]',
                enabled            INTEGER NOT NULL DEFAULT 1,
                created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
