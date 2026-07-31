# Configure deferred loading of MCP tools

> For any operator who wants to control when MCP tools are loaded into memory: at startup (eager) or on the agent's request (deferred).

## Prerequisites

- Apollia running with at least one MCP server connected.
- Access to `apollia.toml` to change the configuration.

## Eager vs deferred: when to pick which

| Mode | Loading | Memory at startup | Recommended when |
|---|---|---|---|
| `deferred` (default) | On demand, through `tool_search` | Low: only the metadata is indexed. | You have many MCP servers connected, or servers with a lot of tools. The agent looks for the tool it needs through `tool_search`. |
| `eager` | At daemon startup | Higher: every tool is loaded immediately. | The tool set is small and fixed. Your agents do not know how to use `tool_search` (for example: older agents or very specialized agents). |

In `deferred` mode, the daemon does not load the full tool schemas at startup. The agent issues `tool_search` calls to find and load the tools on the fly. This shortens the startup time and lowers the memory footprint on installations with many MCP servers.

## Steps - Configure the loading mode

Edit `apollia.toml`:

```toml
[mcp]
tool_loading      = "deferred"
tool_search_limit = 20
```

- `tool_loading`: `"deferred"` (default) or `"eager"`.
- `tool_search_limit`: maximum number of tools returned by a `tool_search` call (default: `20`, bounds: `1` to `500`). Raise this value if your agents need to browse a wide catalogue in a single search.

Restart the daemon after changing it.

## Verification

In `deferred` mode, watch the logs at daemon startup: no message of the form "loading tools from <server>" shows up. The loading messages appear only when an agent issues a `tool_search` call.

In `eager` mode, the startup logs list every tool loaded per server.

## If it does not work

- **"The agent cannot find a tool that is nonetheless connected" in `deferred` mode:** check that the agent declares `tool_search` in its manifest (`skills` or `tools` key). Without that declaration, the agent cannot search for tools on demand. Switch temporarily to `eager` to debug and confirm that the tool really is exposed by the MCP server.
- **Startup time stays long despite `deferred` mode:** another MCP server declares its tools automatically at startup, independently of this parameter. Check the configuration of each server on the **Connections** page.
- **`tool_search` returns too few results:** raise `tool_search_limit` in `apollia.toml`. The upper bound is `500`.

> **Technical reference:** [Apollia reference](/reference) - MCP client architecture, `tool_search` protocol, tool governance, scoping.
