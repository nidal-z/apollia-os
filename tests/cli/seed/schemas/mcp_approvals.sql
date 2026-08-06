CREATE TABLE mcp_approvals (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                server_name TEXT NOT NULL,
                tool_name   TEXT NOT NULL,
                approved_at TEXT NOT NULL,
                expires_at  TEXT,
                UNIQUE(server_name, tool_name)
            );
CREATE TABLE sqlite_sequence(name,seq);
CREATE TABLE mcp_pending_approvals (
                id           TEXT PRIMARY KEY,
                server_name  TEXT NOT NULL,
                tool_name    TEXT NOT NULL,
                arguments    TEXT NOT NULL,
                requested_at TEXT NOT NULL,
                status       TEXT NOT NULL DEFAULT 'pending'
            );
