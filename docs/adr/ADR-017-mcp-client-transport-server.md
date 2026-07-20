# ADR-017: MCP client, transport and server

- Status: Accepted
- Date: 2026-06-04

## Context

MCP (Model Context Protocol) is the interoperability standard for AI agents:
thousands of community servers expose tools (GitHub, Notion, Slack, databases,
search). Without an MCP client, Apollia agents are cut off from that ecosystem.
Apollia must consume MCP servers locally and remotely, expose its own native tools
to other MCP clients, and surface MCP resources to agents without breaking the
local-first and memory-at-agent-initiative principles.

Transport is heterogeneous. Roughly 90% of community servers ship as stdio
subprocesses (`uvx`, `npx`, native binaries), while official SaaS servers (Notion,
Brave, Linear) are increasingly remote over streamable HTTP or SSE. A single
transport mode does not cover the catalogue.

## Decision

We build a native `apollia-mcp` client in its own crate, with a `McpTransport`
trait abstracting stdio, streamable HTTP, and SSE, and we expose Apollia itself as
an MCP server over stdio.

### Native client and transport

The protocol (JSON-RPC 2.0 plus the MCP handshake) is implemented natively with
serde types, no third-party MCP SDK in the binary (no maintained Rust SDK exists
and the spec is still evolving). The client lives in `apollia-mcp`, isolated from
`apollia-tools` and `apollia-runtime`, driven by a dedicated Tokio actor. Transport
is selected dynamically from the server configuration:

- `StdioTransport`: the runtime spawns the server as a subprocess and talks over
  stdin and stdout, with separate reader and writer tasks. Servers start lazily on
  first tool invocation.
- `StreamableHttpTransport` and `SseTransport`: remote servers over HTTP, reusing
  the existing `reqwest` dependency, with no intermediary proxy binary.

MCP tools register in the tool registry under `mcp:{server}/{tool}`, avoiding
collisions with native tools and staying readable in manifests and the API.

### Two-level HITL

Human-in-the-loop applies at two granularities: per server (`requires_approval` in
the server config suspends every call to that server) and per agent
(`tools_requiring_approval` in the manifest lists specific tools such as
`mcp:notion/create_page`). No data leaves the machine without an explicit approval.

### Apollia as an MCP server

The runtime can also expose an MCP server over stdio (`StdioServerTransport`,
JSON-RPC 2.0) driven by a dedicated `McpServerActor`. An IDE or another agent
(VS Code, Cursor, Claude Desktop) drives Apollia as one tool among others. The
server exposes nine native tools (`bash_executor`, `file_read`, `file_write`,
`file_edit`, `file_list`, `file_glob`, `file_grep`, `mcp_client`,
`agent_install`) plus `submit_task`, which submits a task to an agent and returns
the result synchronously with a configurable timeout. The stdio server is
local-only by construction.

### MCP resources surface

Resources are exposed through two complementary paths, never auto-injected. On the
agent side, the read-only tools `mcp_resources_list` and `mcp_resources_read` let a
ReAct agent discover and read resource content on its own initiative, exactly like
`file_read`. On the user side, the desktop prompt bar offers an @-mention picker:
when the user pins a resource, it is added as an explicit system prefix on the next
turn, under explicit user control. Notification-driven cache invalidation is not
yet wired: `resources/updated` notifications are currently dropped rather than used
to invalidate the per-session resource caches, and there is no auto-injection
either way.

## Alternatives considered

### Third-party MCP SDK (rejected)
- Pros: less protocol code to maintain.
- Cons: no maintained Rust SDK; experimental crates add unmaintained transitive
  dependencies on an evolving spec (violates principle #2).

### Single transport (stdio-only or HTTP-only) (rejected)
- Pros: simpler client.
- Cons: stdio-only excludes official remote SaaS servers; HTTP-only excludes the
  ~90% of community servers shipped as stdio subprocesses.

### Auto-injecting active resources into the LLM context (rejected)
- Pros: "everything is there" UX.
- Cons: violates principle #6, pollutes context with undemanded content, hurts
  token cost and ReAct performance.

### Resources as tools only, or @-mention only (rejected)
- Pros: each is simple in isolation.
- Cons: tools-only loses the explicit user pin path; @-mention-only stops the agent
  from finding relevant resources itself.

### Chosen: native client with a transport trait, stdio server, dual resource surface
- Pros: covers the full catalogue, native and under our control, integrates Apollia
  into any MCP host, gives both agent initiative and explicit user pinning.
- Trade-offs: a new crate to maintain and a transport refactor; the dual resource
  surface needs clear documentation.

## Consequences

- Positive: the MCP ecosystem becomes reachable from Apollia agents, local and
  remote; Apollia plugs into any MCP host with no client change; resources are
  available both to the agent and to the user without auto-injection.
- Negative / trade-off: a synchronous `submit_task` can exceed a client timeout on
  long tasks (mitigated by the configurable timeout); remote transports add network
  latency and reconnection handling.
- Watch: token cost if an agent lists resources every turn (local TTL cache);
  resource update notifications should be debounced; lifecycle of remote servers
  that cannot be killed like a subprocess.

## Architectural principles

- Principle #1 (Local-first): stdio transport is local; the stdio server is
  local-only; remote servers are connected only by explicit user action.
- Principle #2 (Zero external dependency): native protocol implementation, no
  third-party MCP SDK, transports reuse existing dependencies.
- Principle #5 (One actor, one responsibility): the client manager and the server
  actor are dedicated Tokio actors with no shared state.
- Principle #6 (Memory at agent initiative): resources are never auto-injected; the
  agent reads on its own initiative, the user pins explicitly.

## Related

- [ADR-018](ADR-018-mcp-oauth.md) adds the OAuth flow for remote HTTP MCP servers.
- [ADR-019](ADR-019-connectors-integrations.md) wires MCP servers into the catalogue and the connector wizard.

## Addendum: response byte cap on untrusted transports (2026-07-20)

A pre-launch security review confirmed that no byte ceiling existed on any read
of an MCP server's output: the stdio line reader, the streamable-HTTP body read,
and the SSE receive buffer all grew without bound. A connected server that never
emitted a newline, streamed without end, or returned a giant body could exhaust
daemon memory mid-session.

Each transport now enforces a per-server byte cap, `max_response_bytes` on the
server configuration (default 8 MiB, bounds [1024, 1073741824], persisted with
the server record). Exceeding it aborts the read with a typed `ResponseTooLarge`
error instead of buffering without limit: the stdio reader bounds the accumulated
line, the HTTP transport streams the body chunk by chunk against the cap, and the
SSE listener bounds its receive buffer (tearing the connection down, which
surfaces to callers as a closed transport). Legitimate traffic and the JSON-RPC
id-correlation are unchanged; operators can raise the cap per server for large
tool payloads.

The subprocess stderr drainer keeps its rolling-window semantics and is not
bounded per line; that lower-severity path is tracked separately.

## Addendum: tool field validation on untrusted servers (2026-07-20)

The same review found that the byte cap bounded only the raw read: the parsed
tool fields were still used verbatim. A `tools/list` response carries a tool
`name`, an optional `description`, and the `initialize` handshake carries a
server `instructions` string, all free text chosen by the server. The name
became a tool registry key (`mcp:<server>/<tool>`) and a structured tracing
field; the description and instructions flowed into the tool catalogue exposed
to the model. A malicious server could inject control characters to forge
tracing lines, break the registry key separator, plant instructions in the
model context, or advertise thousands of tools to exhaust context and memory.

Every such field is now validated and bounded at the ingestion boundary, before
it reaches any consumer:

- Tool names must be non-empty, at most 128 bytes, and drawn from
  `[A-Za-z0-9_.-]`. A name is a registry key and a `tools/call` argument, so a
  malformed one cannot be silently rewritten: the offending tool is dropped
  (the server's well-formed tools are kept). The raw name is never logged, since
  it is the forgery vector the guard exists to contain.
- Descriptions and the server instructions string are stripped of control
  characters and truncated on a UTF-8 boundary (8 KiB and 16 KiB respectively).
- The number of tools retained from a server is capped by `max_tools` on the
  server configuration (default 256, bounds [1, 8192], persisted with the server
  record, same shape as `max_response_bytes`). Tools beyond the cap are dropped.

Legitimate servers are unaffected: real tool names (`GetWeather`,
`notion.search`) pass the charset, and normal descriptions round-trip unchanged.
The bounding is applied once, where the session stores the discovered tools and
instructions, so every downstream sink (the tool registry, the deferred tool
search index, the connection-test API) receives already-bounded data.
