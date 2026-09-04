---
title: Configure deferred loading of MCP tools
slug: /operator-help/integrations/configure-deferred-mcp-loading
sidebar_position: 8
---

# Configure deferred loading of MCP tools

> For any operator who wants to control when MCP tools are loaded into memory: at startup (eager) or on the agent's request (deferred).

## Prerequisites

- Apollia running with at least one MCP server connected.
- Access to `apollia.toml` to change the configuration.

## Eager vs deferred: when to pick which

| Mode | What the model is shown | Recommended when |
|---|---|---|
| `deferred` (default) | The `tool_search` tool, and the indexed tools only when the whole index fits inside `tool_search_limit`. | You have many MCP servers connected, or servers with a lot of tools. The agent looks for the tool it needs through `tool_search`. |
| `eager` | Every tool of every server, schema included. | The tool set is small and fixed, and you would rather not spend a turn on a search. |

In `deferred` mode, the daemon does not put every tool schema in front of the model at startup. It indexes what each server exposes, and the agent finds what it needs through `tool_search`.

**What the mode does not change.** Both modes send the same single `tools/list` to each server at connection time, and the deferred path keeps the schemas that answer brings back in its cache rather than dropping them. So the process holds the same thing either way: deferring is about what enters the prompt, not about what the machine loads. Do not pick it to save memory or to shorten startup, pick it to keep the prompt small.

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

Watch the daemon logs at startup. Each connected server emits one line carrying a tool count, and the event name tells you which mode ran: `mcp.tools.index.discovered` in `deferred` mode, `mcp.tools.discovered` in `eager` mode. There is no per-tool listing in either mode, and no message naming a server as it loads.

In `deferred` mode, one more line is written when the assistant's tool list is built: `mcp.deferred.index_advertised` if the whole index fitted inside `tool_search_limit` and was declared up front, `mcp.deferred.index_reachable_through_search_only` if it did not and `tool_search` is the only way in. Both carry the indexed count and the limit, which is what tells you which side of the bound you are on.

## If it does not work

- **"The agent cannot find a tool that is nonetheless connected" in `deferred` mode:** there is nothing to declare in the agent's manifest, and no manifest key named `tool_search` is read by anything. The tool is injected by the runtime, and only into the built-in conversational assistant. An installed Python agent has no `tool_search` at all, so in `deferred` mode it reaches MCP tools only when the whole index fits inside `tool_search_limit` and is therefore declared up front. Raise that bound, or switch to `eager`.
- **Startup time stays long despite `deferred` mode:** the mode is not what to look at. Both modes pay the same `tools/list` per server at connection time, so a slow start comes from a server that is slow to answer it. Test each one from the **Connections** page.
- **`tool_search` returns too few results:** raise `tool_search_limit` in `apollia.toml`. The upper bound is `500`.

> **Technical reference:** [Apollia reference](/reference) - MCP client architecture, `tool_search` protocol, tool governance, scoping.
