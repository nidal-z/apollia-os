---
title: Configure deferred loading of MCP tools
sidebar_position: 8
---

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

In `deferred` mode, the daemon does not put the tool schemas in front of the model at startup. It indexes what each server exposes, and the agent finds what it needs through `tool_search`. This shortens the startup time and keeps the prompt small on installations with many MCP servers.

One nuance decides how the agent reaches a tool once it has found it. When the whole index fits inside `tool_search_limit`, Apollia declares those tools directly, schemas included, and the agent calls them like any other tool. Above that bound, `tool_search` stays the only entry point and its results carry each tool's schema, which is what the agent reads before calling. Either way the tool is callable; the bound decides whether it is announced up front or discovered.

## Steps - Configure the loading mode

Edit `apollia.toml`:

```toml
[mcp]
tool_loading      = "deferred"
tool_search_limit = 20
```

- `tool_loading`: `"deferred"` (default) or `"eager"`.
- `tool_search_limit`: maximum number of tools returned by a `tool_search` call (default: `20`, bounds: `1` to `500`). It doubles as the bound below which the whole index is declared up front, so raising it declares more tools directly and costs more prompt; lowering it pushes more discovery through `tool_search`.

Restart the daemon after changing it.

## Verification

In `deferred` mode, watch the logs at daemon startup: no message of the form "loading tools from <server>" shows up. The loading messages appear only when an agent issues a `tool_search` call.

In `eager` mode, the startup logs list every tool loaded per server.

## If it does not work

- **"The agent cannot find a tool that is nonetheless connected" in `deferred` mode:** check that the agent declares `tool_search` in its manifest (`skills` or `tools` key). Without that declaration, the agent cannot search for tools on demand. Switching temporarily to `eager` confirms whether the server really exposes the tool.
- **Startup time stays long despite `deferred` mode:** another MCP server declares its tools automatically at startup, independently of this parameter. Check the configuration of each server on the **Connections** page.
- **`tool_search` returns too few results:** raise `tool_search_limit` in `apollia.toml`. The upper bound is `500`.

> **Technical reference:** [Apollia reference](/reference) - MCP client architecture, `tool_search` protocol, tool governance, scoping.
