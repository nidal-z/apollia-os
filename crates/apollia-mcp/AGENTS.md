# crates/apollia-mcp/AGENTS.md

> Local rules for the `apollia-mcp` crate (MCP client: config, JSON-RPC,
> transports, session, manager, executor). Read after the root `AGENTS.md`
> and before editing this crate.

This crate connects Apollia to MCP servers over three transports (`stdio`,
`streamable-http`, `sse`). MCP servers are untrusted input: their responses
cross the trust boundary into the daemon, so every read from a server must be
bounded and every parse must fail into a typed error, never a panic. See
the decisions chapter of the documentation site for the transport and OAuth
> design.

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

## 2. Tool fields are validated and bounded

The byte cap bounds the raw read; it does not bound the parsed tool metadata.
A server's `tools/list` names and descriptions, and the `initialize`
`instructions` string, are untrusted free text that reaches sensitive sinks: a
name becomes a tool registry key (`mcp:<server>/<tool>`) and a tracing field,
descriptions and instructions flow into the model's tool catalogue. They are
validated and bounded once, at the session ingestion boundary
(`discover_tools`, `discover_tools_index`, the `initialize` handler), via
`sanitize.rs`:

- Tool names: non-empty, at most 128 bytes, charset `[A-Za-z0-9_.-]`. A bad
  name is a bad registry key and a bad `tools/call` argument, so the tool is
  dropped, not rewritten; the server's other tools survive. Never log a raw
  tool name from an untrusted server: it is the tracing-forgery vector.
- Descriptions and instructions: control characters stripped, truncated on a
  UTF-8 boundary.
- Tool count: capped by `max_tools` on `McpServerConfig` (default 256, bounds
  `[1, 8192]`).

Sanitize at ingestion, never at each sink. Every downstream consumer (registry,
deferred tool-search index, `test_connection` API) reads the already-bounded
session state, so a new sink needs no extra guard.

---

## 3. Errors and configuration

- Errors use `thiserror` (`TransportError`, `McpConfigError`, `McpRepoError`).
  No `anyhow`, no `unwrap`/`expect`/`panic!` outside tests.
- New `McpServerConfig` fields need: a `#[serde(default = "...")]`, a
  `default_*` function, a bound check in `McpServerConfig::validate` with a
  dedicated `McpConfigError` variant, and, if they must survive the desktop
  path, a column plus additive migration in `server_repository.rs`
  (`CREATE TABLE`, the `ALTER TABLE ADD COLUMN` guard, `save`, the `SELECT`
  lists, and `row_to_config`).

---

## 4. Tests

Transport tests use a local axum server on an ephemeral TCP port (HTTP/SSE) or
a real subprocess (`cat`, `sh`) for stdio; config tests use `tempfile`. Write
them GIVEN / WHEN / THEN. Any new bounded read gets both a rejection test
(oversized input aborts with the typed error) and a legitimate-path test
(normal input still round-trips under a small cap). The same rule applies to a
new bounded or validated tool field (see section 2): a rejection test (malicious
name dropped, control characters stripped, list capped) and a legitimate-path
test (real names and descriptions pass through unchanged).
