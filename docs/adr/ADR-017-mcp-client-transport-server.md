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
