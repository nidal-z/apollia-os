# crates/apollia-mcp/AGENTS.md

> Local rules for the `apollia-mcp` crate (MCP client: config, JSON-RPC,
> transports, session, manager, executor). Read after `docs/agents/INDEX.md`
> and before editing this crate.

This crate connects Apollia to MCP servers over three transports (`stdio`,
`streamable-http`, `sse`). MCP servers are untrusted input: their responses
cross the trust boundary into the daemon, so every read from a server must be
bounded and every parse must fail into a typed error, never a panic. See
ADR-017 (transport) and ADR-018 (OAuth) for the design and its addenda.

---

## 1. Untrusted responses are bounded

Every transport read is capped by `max_response_bytes` on `McpServerConfig`
(default 8 MiB, bounds `[1024, 1073741824]`). A read that exceeds the cap
returns `TransportError::ResponseTooLarge { limit }` rather than growing memory:

- `stdio`: the stdout line reader accumulates against the cap and aborts before
  a never-terminated line exhausts memory.
- `streamable-http`: the body is read chunk by chunk against the cap, never via
  an unbounded `Response::text()`.
- `sse`: the receive buffer is bounded; overflow tears the connection down and
  surfaces to callers as `TransportError::Closed`.

The cap is per-server config: it is read by `create_transport`, forwarded to
each transport constructor, and persisted in the SQLite `mcp_servers` table
(`max_response_bytes` column). When adding a new transport or a new server-side
read, thread the cap through and enforce it at the read; do not add an
unbounded `text()`, `json()`, `bytes()`, or `next_line()` on server output.

OAuth discovery and token reads (crate `apollia-auth`) are bounded by a fixed
`MAX_OAUTH_RESPONSE_BYTES` constant, not this per-server key, because the OAuth
flow runs before a transport exists.

---

## 2. Errors and configuration

- Errors use `thiserror` (`TransportError`, `McpConfigError`, `McpRepoError`).
  No `anyhow`, no `unwrap`/`expect`/`panic!` outside tests.
- New `McpServerConfig` fields need: a `#[serde(default = "...")]`, a
  `default_*` function, a bound check in `McpServerConfig::validate` with a
  dedicated `McpConfigError` variant, and, if they must survive the desktop
  path, a column plus additive migration in `server_repository.rs`
  (`CREATE TABLE`, the `ALTER TABLE ADD COLUMN` guard, `save`, the `SELECT`
  lists, and `row_to_config`).

---

## 3. Tests

Transport tests use a local axum server on an ephemeral TCP port (HTTP/SSE) or
a real subprocess (`cat`, `sh`) for stdio; config tests use `tempfile`. Write
them GIVEN / WHEN / THEN. Any new bounded read gets both a rejection test
(oversized input aborts with the typed error) and a legitimate-path test
(normal input still round-trips under a small cap).
