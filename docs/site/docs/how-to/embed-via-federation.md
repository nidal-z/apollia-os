---
sidebar_position: 2
title: Embed Apollia via federation (MCP + REST)
---

# Embed Apollia via federation (MCP + REST)

This guide shows how to integrate Apollia into a host product as a sovereign
sidecar, without moving your data into the runtime. It suits products whose data
cannot leave their trust boundary but that still want autonomous agents acting
on it.

It assumes you can run an Apollia daemon, that you can stand up an MCP server in
front of your data, and that your product takes writes back through that server
or through a public HTTP API.

## The pattern

In the federation model the two systems stay peers:

- **Apollia runs as a sovereign sidecar.** It performs the agent work: reasoning,
  planning, tool calls, all under its own governance (permissions, audit,
  budgets).
- **Your product exposes its data through an MCP server.** Apollia connects to it
  as an MCP client and calls your tools to read what it needs. Your data stays on
  your side; Apollia reads it through the tools you choose to expose.
- **Apollia writes back through your product.** When the agent has a result to
  persist, it calls a write tool of your MCP server, or your product's REST
  endpoints when they are public, so your product remains the system of record
  and keeps control of every write.

Apollia is often the client of the host, not the other way around. Nothing is
copied into the runtime that you did not deliberately expose.

## Step 1: expose your data over MCP

Stand up an MCP server that fronts the data and actions you want the agent to
use. Apollia's MCP client speaks the standard protocol (`initialize` plus
`tools/list`) over three transports: stdio, streamable HTTP, and SSE. Pick the
transport that fits how your server is deployed relative to the runtime.

Expose read tools for the context the agent needs, and keep write tools narrow
and explicit. The agent can only use what your server advertises.

## Step 2: connect Apollia to your MCP server

Register your server with the runtime and confirm its tools are discovered. Once
connected, an agent running inside Apollia invokes your MCP tools through its
tool interface (tool names take the form `mcp:<server>/<tool>`). A tool is
offered to an agent only when its access list grants it, on the sheet of the
agent or in its package. Those calls go through the same governed path as
native tools, so they are subject to permissions and land in the audit trail.

For the desktop setup details, see the operator help on
[connecting an MCP server](/operator-help/integrations/connect-an-mcp-server)
and [wiring your own MCP server](/operator-help/integrations/wire-your-own-mcp-server).

## Step 3: gate writes with human approval

Federation usually means the agent can trigger changes in your product. Keep a
human in the loop on those.

An MCP server you register carries no approval setting of its own. Each tool
it publishes is decided on its action family: a tool your server declares as
writing or destructive, or one it does not describe at all, is treated as
sensitive, and an agent that calls it asks first.

```sh
apollia-os mcp add my-product --url https://example.internal/mcp
```

The person in front of the run answers once, always for this agent, or refuses.
"Always" is a standing consent of that agent for that family of actions; it
holds until it is withdrawn. Declare `readOnlyHint` and `destructiveHint`
honestly on your tools: they are what sorts a read from a write here.

For how approvals and autonomy levels shape this, see
[Autonomy tiers](/explanation/autonomy-tiers) and the explanation of
[the accountability model](/explanation/accountability-model).

## Step 4: let Apollia write back through your product

The agent has two ways to persist a result in your product, and they do not
reach the same places.

The first is a write tool of your MCP server, from step 1. The call goes through
the governed path of step 3, and your server reaches your API wherever it runs,
inside your network included, with whatever credential it needs: nothing of
that credential passes through the agent. This is the way to choose when your
API is internal or needs a secret.

The second is `http_fetch`, the native tool that calls a web address. It is
offered only to an agent whose sheet or package grants it, and its family is
`send_external`, so a call is asked unless an "always" of the person or a grant
of the run covers it, and an unattended run waits for the answer. It refuses
any destination that is not public: loopback, private and link-local addresses,
`localhost`, names ending in `.local`, `.internal` or `.localdomain`, and a
redirect toward one of them. The model writes its method, headers and
body, so an API key it sends has to sit in the agent's context, and it reads
at most 1 MB of the answer. Use it for a public endpoint that needs no secret.

Either way, your product validates and stores the change, remaining the system
of record. If you also drive Apollia from your product (submitting tasks,
streaming results), that side uses the same stable contract described in
[Integrate Apollia via the driving contract](/how-to/integrate-via-driving-contract).

## Why federation

This keeps sovereignty on your side of the line. Your data is read through tools
you expose and written through a server or an API you own, while Apollia contributes the
agent runtime with its governance. It is the integration model for products that
cannot hand their data to a cloud sandbox but still want autonomous, auditable
agents.

## Related

- [Integrate Apollia via the driving contract](/how-to/integrate-via-driving-contract)
  for driving the runtime from your product.
- [Audit and verify a run](/how-to/audit-and-verify) for the
  trail every federated action leaves.
- [The accountability model](/explanation/accountability-model) for the
  governance that backs this pattern.
