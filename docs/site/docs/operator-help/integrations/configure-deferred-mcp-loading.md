---
title: Configure deferred loading of MCP tools
slug: /operator-help/integrations/configure-deferred-mcp-loading
sidebar_position: 8
---

# Configure deferred loading of MCP tools

> For any operator who wants to control when MCP tools are loaded into memory: at startup (eager) or on the agent's request (deferred).

## Prerequisites

- Apollia running with at least one service connected from **Connections**.
- Access to `apollia.toml` to change the configuration. With developer mode on, the file opens in **Settings**, section **Configuration**.

## Eager vs deferred: when to pick which

| Mode | What the model is shown | Recommended when |
|---|---|---|
| `deferred` (default) | The `tool_search` tool, and the MCP tools granted to the agent only when they all fit inside `tool_search_limit`. | You have many MCP servers connected, or servers with a lot of tools. The agent looks for the tool it needs through `tool_search`. |
| `eager` | Every MCP tool granted to the agent, schema included. | The tool set is small and fixed, and you would rather not spend a turn on a search. |

In `deferred` mode, the daemon does not put every tool schema in front of the model at startup. It indexes what each server exposes, and the agent finds what it needs through `tool_search`.

**What the mode does not change.** Both modes send the same single `tools/list` to each server at connection time, and the deferred path keeps the schemas that answer brings back in its cache rather than dropping them. So the process holds the same thing either way: deferring is about what enters the prompt, not about what the machine loads. Do not pick it to save memory or to shorten startup, pick it to keep the prompt small.

One nuance decides how the agent reaches a tool once it has found it. The index an agent searches holds the MCP tools granted to it, and nothing else. When that index fits inside `tool_search_limit`, Apollia declares those tools directly, schemas included, and the agent calls them like any other tool. Above that bound, `tool_search` stays the only entry point and its results carry each tool's schema, which is what the agent reads before calling. Either way the tool is callable; the bound decides whether it is announced up front or discovered.

## Steps - Configure the loading mode

Edit `apollia.toml`, in a text editor or in **Settings**, section **Configuration**, with developer mode on:

```toml
[mcp]
tool_loading      = "deferred"
tool_search_limit = 20
```

- `tool_loading`: `"deferred"` (default) or `"eager"`.
- `tool_search_limit`: maximum number of tools returned by a `tool_search` call (default: `20`, bounds: `1` to `500`). It doubles as the bound below which the whole index is declared up front, so raising it declares more tools directly and costs more prompt; lowering it pushes more discovery through `tool_search`.

Restart Apollia after changing `tool_loading`: the mode is read once, when Apollia starts, and a change saved while it runs waits for the next start. A change of `tool_search_limit` applies without a restart.

## When a server runs

Apollia's launch waits for no server. At launch, it starts only the servers whose tools it does not know yet, in the background (see below). Every other server starts at the first call of one of its tools, and the call waits for it to answer. After a while without a call it stops, and starts again at the next call.

```toml
[mcp]
idle_stop_secs = 900
```

- `idle_stop_secs`: seconds without a call after which a server is stopped (default: `900`, that is 15 minutes, bounds: `60` to `86400`). Restart Apollia after changing it.

While a server sleeps, Apollia keeps what it last published, so nothing disappears from the screens:

- its tools stay offered to the agents that hold them, in either mode, and listed on the card of the connection with their family;
- its resources stay offered by the **@** picker of the Chat. Reading one starts the server, like a call of one of its tools. A server whose resources were never listed is started once in the background when the picker asks for them, without holding up the others, and its list joins the picker when it arrives.

A server whose tools Apollia does not know yet is started at launch, in the background, to learn them; the launch does not wait for it either. A paused server is never started, neither by a call, nor by the picker, nor at launch: a call is refused with the reason that its connection is paused, and its resources are not offered.

## Verification

Watch the daemon logs. Each server that starts emits one line carrying its name and its tool count, and the event name tells you which mode ran: `mcp.tools.index.discovered` in `deferred` mode, `mcp.tools.discovered` in `eager` mode. There is no per-tool listing in either mode.

In `deferred` mode, one more line is written when the assistant's tool list is built: `mcp.deferred.index_advertised` if the whole index fitted inside `tool_search_limit` and was declared up front, `mcp.deferred.index_reachable_through_search_only` if it did not and `tool_search` is the only way in. Both carry the indexed count and the limit, which is what tells you which side of the bound you are on.

## If it does not work

- **"The agent cannot find a tool that is nonetheless connected" in `deferred` mode:** `tool_search` searches the tools granted to the agent, not everything that is connected. Open the agent's sheet, click **Adjust**, and grant the connection under **Accesses**. Nothing has to be declared for `tool_search` itself: the runtime offers it on its own in this mode.
- **The first call of a tool takes longer than the next ones:** its server was asleep and started for that call. The mode is not what to look at: both modes pay the same `tools/list` when a server starts. Raise `idle_stop_secs` to keep servers running longer, or click **Check** on the card of a slow server to see how it answers.
- **A change of `tool_loading` does nothing:** Apollia reads it at start. Restart Apollia.
- **`tool_search` returns too few results:** raise `tool_search_limit` in `apollia.toml`. The upper bound is `500`.

> **Technical reference:** [Apollia reference](/reference) - MCP client architecture, `tool_search` protocol, tool governance, scoping.
