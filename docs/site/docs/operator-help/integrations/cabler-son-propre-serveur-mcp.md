# Wire your own MCP server

> For any operator or builder who wants to connect an MCP server that is not in the catalogue, locally (stdio) or remotely (HTTP, SSE).

## Prerequisites

- Apollia running.
- An MCP server that conforms to the spec, over **stdio** transport (local subprocess), **streamable-http** or **sse**.
- Access to the server: local command + arguments, or remote URL + authentication headers.

## Steps

1. In the **Connections** sidebar, click **+ Add custom** at the top. The panel opens on the **Custom** tab.

   ![Custom tab of the catalogue: the blank form](/img/operator-help/en/integration-cabler-son-propre-serveur-mcp-1.png)

2. Fill in the form according to the transport you chose (see the subsections below).

3. Click **Test**. Apollia attempts a real connection and counts the tools declared by the server.

4. If the test passes, click **Install**. The server appears in the sidebar.

### stdio case (local command)

- **Name**: unique identifier, lowercase letters, digits and hyphens only (example: `test-fs`).
- **Transport**: `stdio`.
- **Command**: executable to launch (for example `npx`, `uvx`, or an absolute path).
- **Arguments**: separated by spaces (for example `-y @modelcontextprotocol/server-filesystem ~/Documents`).
- **Require approval**: tick this if you want an HITL approval on every tool call.

![Custom form on stdio transport, with the command and the arguments filled in](/img/operator-help/en/integration-cabler-son-propre-serveur-mcp-2.png)

### streamable-http case (remote server)

- **Name**: unique identifier.
- **Transport**: `streamable-http`.
- **URL**: HTTP endpoint of the server (`https://...`).
- **Headers** (optional): one per line, in the `Header-Name=value` format. Example: `Authorization=Bearer sk-...` or `X-API-Key=...`.

![Custom form on streamable-http transport, with the URL and the authentication headers](/img/operator-help/en/integration-cabler-son-propre-serveur-mcp-3.png)

### SSE case

Identical to the streamable-http case but with **Transport**: `sse`. Used for servers that keep a persistent SSE connection open.

## OAuth 2.1, automatic

If your MCP server advertises an OAuth endpoint that conforms to the MCP authorization spec (RFC 9728 Protected Resource Metadata + RFC 8414 Authorization Server Metadata), Apollia handles it all on its own:

1. Metadata discovery (PRM, then OIDC fallback).
2. Client identification through the Apollia CIMD, or Dynamic Client Registration (RFC 7591) as a fallback.
3. Code exchange with PKCE S256 and Resource Indicators (RFC 8707).
4. Token storage in the local keychain and proactive refresh with singleflight.

You have nothing to configure on the Apollia side. The server triggers everything on the first 401.

## Local mDNS discovery

Apollia can discover MCP servers on your local network through mDNS (service type `_apollia-mcp._tcp.local.`). Enable the option in **Connections, Preferences** if your server advertises it.

## Verification

- Green dot next to the server in the sidebar.
- The detail view shows the `tools`, `resources` and `prompts` sections filled in with what the server advertises.
- A ping test confirms the latency (see [Test an MCP connection](tester-une-connexion-mcp.md)).

## Security, what Apollia applies by default

- **Trust level**: any manually added server is marked `custom`. No automatic `verified_official` level.
- **HITL approval**: by default the tool is in *requires_approval* mode, every call asks for your validation. You can loosen this per tool on the [Understand MCP permissions](comprendre-les-permissions-mcp.md) page.
- **Roots**: Apollia declares the accessible directories to the server (the agent workspace + the current project). The server sees nothing else.
- **Sampling and elicitation**: not implemented. Apollia does not advertise these two capabilities during the handshake, so a server that supports them will not try to call back through them.

## Deferred loading mode

By default, Apollia loads the tools of an MCP server in `deferred` mode: they are not injected into the context at startup. The agent uses `tool_search` to fetch them on demand. This is the right setting for most servers.

If your server exposes few tools (fewer than ten) or if your agents use them systematically on every run, you can switch to `eager` mode in your configuration:

```toml
[mcp]
tool_loading = "eager"
```

In `eager` mode, every tool of the server is loaded into the context on every call. This simplifies the agent behaviour but increases token consumption.

The `tool_search_limit` parameter bounds the number of tools returned by `tool_search` in `deferred` mode. Default value: `20`. Valid range: `1` to `500`.

```toml
[mcp]
tool_loading = "deferred"
tool_search_limit = 20
```

## If it does not work

- **"Command not found" on stdio**: your binary is not in Apollia's PATH. Give the absolute path or adjust your PATH before launching Apollia.
- **"Connection refused" on HTTP or SSE**: wrong URL or port, or a firewall blocking outbound traffic. Check that the server is reachable from your machine with `curl <url>`.
- **OAuth loop**: your server advertises its metadata endpoint incorrectly, or a scope was refused. Apollia rejects non-conformant authorization servers fail-fast (PKCE S256 is mandatory). Check the server-side logs.
- **"No tool detected"**: the handshake succeeds but the server does not declare the `tools` capability in its `InitializeResult`. Check the server-side implementation.

## Getting your server to show up in the UI catalogue

Your internal server appears in the list once connected, but without the logo, description and trust-level badge that the official entries carry. Customizing the catalogue is not possible in `v0.1.0-preview`: the override file is read by nothing.

> **Technical reference:** [Apollia reference](/reference) , full protocol schema, capabilities, transports, security.
