-- Deterministic seed fragment for the MCP / CONNECTIONS subsystem (mcp.db).
-- Table: mcp_servers.
-- Unblocks the desktop Connections page automation (connections-det.json):
--   * sidebar-mcp-*                 (custom MCP server rows in the sidebar)
--   * mcp-detail-tab-{overview,tools,settings}
--   * mcp-tools-count / mcp-tools-list / mcp-tool-*
-- Schema is applied separately by the builder (schemas/mcp.sql).
--
-- IMPORTANT read-path finding (verified in code, not assumed):
--   The sidebar list is served by `list_mcp_servers` -> runtime
--   GET /api/v1/mcp/servers -> McpClientManager::status(). The manager only
--   returns servers whose MCP `initialize` + `tools/list` handshake SUCCEEDED
--   at boot (apollia-mcp/src/manager.rs: a config that fails to connect is
--   dropped from the session map and never surfaces in status()). A static SQL
--   row alone therefore surfaces NOTHING in the sidebar, it only drives the
--   boot connection attempt (apollia-runtime reads every mcp_servers row via
--   McpServerRepository::list() and hands them to the manager).
--
--   To make the rows actually appear (sidebar + overview + settings + tools +
--   individual mcp-tool-* entries), each row spawns the bundled deterministic
--   stub server files/mcp-stub-server.py, which completes the handshake and
--   advertises a fixed 3-tool catalogue.
--
-- Dependency: the stub is launched with /usr/bin/python3, present on any macOS
--   machine with the Xcode command-line tools (the desktop build toolchain
--   already requires them). No third-party Python package is used.
--
-- Path wiring: args_json carries the placeholder token __APOLLIA_SEED_MCP_STUB__.
--   build-seed.sh copies the stub into the seed data dir and rewrites the token
--   to the absolute stub path (the seed HOME is dynamic, so the path cannot be
--   hardcoded here). If that rewrite is skipped, the rows stay in mcp.db but do
--   not connect and the sidebar stays empty, matching the finding above.
--
-- Determinism: fixed names (primary key), fixed timestamps, no random values.
-- Two rows so the sidebar renders more than one entry and nth-0 is stable
-- (mcp_servers has no explicit ORDER BY in status(); both rows expose the same
-- stub catalogue, so either ordering yields identical, deterministic content).

INSERT INTO mcp_servers
  (name, command, args_json, env_json, transport, url,
   requires_approval, init_timeout_secs, call_timeout_secs, tags_json,
   enabled, created_at, updated_at)
VALUES
  ('seed-mcp-fs',
   '/usr/bin/python3',
   '["__APOLLIA_SEED_MCP_STUB__"]',
   '{}',
   'stdio',
   NULL,
   0,
   30,
   60,
   '["seed","demo"]',
   1,
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),
  ('seed-mcp-notes',
   '/usr/bin/python3',
   '["__APOLLIA_SEED_MCP_STUB__"]',
   '{}',
   'stdio',
   NULL,
   0,
   30,
   60,
   '["seed","demo"]',
   1,
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z');
