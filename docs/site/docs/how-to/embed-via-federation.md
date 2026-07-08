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
front of your data, and that your product has an HTTP API Apollia can call back.

## The pattern

In the federation model the two systems stay peers:

- **Apollia runs as a sovereign sidecar.** It performs the agent work: reasoning,
  planning, tool calls, all under its own governance (permissions, audit,
  budgets).
- **Your product exposes its data through an MCP server.** Apollia connects to it
  as an MCP client and calls your tools to read what it needs. Your data stays on
  your side; Apollia reads it through the tools you choose to expose.
- **Apollia writes back through your HTTP API.** When the agent has a result to
  persist, it calls your product's REST endpoints, so your product remains the
  system of record and keeps control of every write.

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
tool interface (tool names are namespaced with an `mcp:` prefix). Those calls go
through the same governed path as native tools, so they are subject to
permissions and land in the audit trail.

See the operator help on
[connecting an MCP server](/operator-help/integrations/connecter-un-serveur-mcp)
and [wiring your own MCP server](/operator-help/integrations/cabler-son-propre-serveur-mcp)
for the setup details.

## Step 3: gate writes with human approval

Federation usually means the agent can trigger changes in your product. Keep a
human in the loop on those.

Apollia's permission engine classifies each tool call and can require explicit
approval before an action runs. Route your write-capable tools (whether MCP
tools or the callbacks to your REST API) through a rule that raises an approval
request, so an operator confirms before anything is written back. Approvals are
recorded, so the decision is part of the trail alongside the action.

For how approvals and autonomy levels shape this, see
[Configure permissions, autonomy tiers and budgets](/how-to)
and the explanation of [the accountability model](/explanation/accountability-model).

## Step 4: let Apollia write back through REST

When the agent produces a result, it calls your product's HTTP API to persist
it. Your product validates and stores the change, remaining the system of
record. If you also drive Apollia from your product (submitting tasks, streaming
results), that side uses the same stable contract described in
[Integrate Apollia via the driving contract](/how-to/integrate-via-driving-contract).

## Why federation

This keeps sovereignty on your side of the line. Your data is read through tools
you expose and written through an API you own, while Apollia contributes the
agent runtime with its governance. It is the integration model for products that
cannot hand their data to a cloud sandbox but still want autonomous, auditable
agents.

## Related

- [Integrate Apollia via the driving contract](/how-to/integrate-via-driving-contract)
  for driving the runtime from your product.
- [Audit, verify and roll back a run](/how-to/audit-verify-rollback) for the
  trail every federated action leaves.
- [The accountability model](/explanation/accountability-model) for the
  governance that backs this pattern.
