---
title: Understand the scope of an integration
slug: /operator-help/integrations/understand-integration-scope
sidebar_position: 2
---

# Understand the scope of an integration

> For any operator who wonders why one agent can call a tool while another cannot, or how to scope an integration to a specific project.

## Prerequisites

- At least one connected integration (native connector or MCP server).
- At least one installed agent or one active project.

## The three filters that control a tool call

When an agent tries to use a tool, Apollia applies three filters in order:

1. **The agent manifest** declares the list of required and optional tools. A tool absent from the manifest is not reachable by that agent.
2. **The permission rules** (see [Understand MCP permissions](understand-mcp-permissions.md)) decide whether the tool can run automatically or requires an approval.
<!-- claim:sovereignty-profile-gates-connecting-not-calling -->
3. **The sovereignty profile**, which is not a third filter on a tool call. Set to `local_only`, it refuses to open a new cloud connection: the OAuth flow will not start. It does not inspect calls made through a connection that is already established, so treat it as a gate on connecting, not on using.

If either of the first two refuses, the tool does not run.

## On the agent side

Open an agent detail page, **Tools** tab. The list shows, read-only:

- The tools required by the manifest, with their identifier (for example `outlook.send`).
- The optional tools.
- A badge indicates whether the tool requires an HITL approval by default.

![Agent detail page, Tools tab: the list of required and optional tools with their approval badges](/img/operator-help/integration-comprendre-la-portee-d-une-integration-1.png)

This list **cannot be edited from the interface in v0.1.0**. To add or remove a tool for an agent, you have to edit its manifest and reinstall it. See the Help page [Install an agent](../agents/install-an-agent.md).

## On the project side

Open a project, **Context** tab. There you find **Context Providers** (local folders, Git repositories, and so on), which feed the context of the project chats. **These are not MCP tools**, and it is not possible in v0.1.0 to scope an MCP or a connector to a specific project.

Every installed MCP and every native connector is visible to every agent that declares them in its manifest, regardless of the active project.

## Checking what an agent can do

- **Tools** tab of the agent, full list.
- In the chat, ask the agent *"List the tools you can use"*. It answers with its toolbelt if its system prompt allows it.
- Test a concrete call. If the tool is refused, the agent returns a clear message (`tool not allowed`, `SovereigntyBlocked`, and so on).

## If it does not work

- **The agent says "tool not allowed"**: the tool is not in its manifest. Update the agent (reinstall with an extended manifest) or use another agent that declares it.
- **The agent sees the tool but the call fails**: this is most likely a permission issue (expired token, missing scope) or a sovereignty issue. See [Understand MCP permissions](understand-mcp-permissions.md) and [Manage OAuth tokens](manage-oauth-tokens.md).
- **Two agents should see the same tool and only one does**: check both manifests, the tool has to be declared in each of them.

> **Technical reference:** [Apollia reference](/reference) , per-agent tool resolution, project scoping, ContextProvider.
