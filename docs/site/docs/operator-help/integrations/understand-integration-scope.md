---
title: Understand the scope of an integration
slug: /operator-help/integrations/understand-integration-scope
sidebar_position: 2
---

# Understand the scope of an integration

> For any operator who wonders why one agent can call a tool while another cannot, or how to scope an integration to a specific agent.

## Prerequisites

- At least one connected integration (native connector or MCP server).
- At least one installed agent.

## The four filters that control a tool call

When an agent tries to use a tool, Apollia applies four filters:

1. **The agent's accesses** list the tools granted to it, by action family. A tool that is not among them is not reachable by that agent.
2. **The rule of its action family** (see [Understand MCP permissions](understand-mcp-permissions.md)) decides whether the tool runs without asking or asks for your agreement first.
<!-- claim:organisation-policy-gates-connecting-not-calling -->
3. **The organisation policy**, when your organisation has deployed a signed one. It is read when Apollia starts and it comes before everything else: a kind of action, a tool or a connection it refuses stays refused, whatever the accesses and whatever you answer. A connection it forbids reads **Blocked by your organisation** in **Connections**, with the reason *forbidden by the policy*. When it denies remote connections, adding a connection to an online service is refused in the dialog, before the OAuth flow of a Google or Microsoft account starts. Without a policy, nothing is refused at this step.

4. **The pause of the connection.** A connection you paused keeps its agents' accesses, but none of its tools runs: the call is refused with the reason that its connection is paused. See [Pause or remove a connection](pause-or-remove-a-connection.md).

If any of the four refuses, the tool does not run.

## On the agent side

Open **Agents**, then the sheet of an agent. Its sections are **Mission and examples**, **Model**, **Accesses**, **Sources** and **Activity**. **Accesses** shows what the agent is allowed to do:

- The tools granted to the agent, grouped by action family (**Read**, **Write or delete**, **Send outside**, and so on).
- The rule of each family: **Without asking**, **Your consent is asked** or **Consent every time**.
- A **Remote** badge on an access that comes from a remote connection.

This list is edited on the sheet itself. Click **Adjust**: each access becomes a switch, **Grant a tool, an agent or a workflow** adds one, and **Grant a connection** grants the tools of a connection, family by family. Click **Save**; each change is noted in the **Activity log**.

An agent that follows the **Default** access, the one every new agent gets, keeps it when you give it a connection: what you give is marked **On top**, and the default is still read again at every run. An access whose connection is paused is marked **Paused**.

## On the connection side

The card of a connection, in **Connections**, answers the other question: which agents use it. **Agents that use it** lists them, with **By default** for those that hold it through the default access. **Give to an agent** gives its tools to several agents at once; the sensitive ones still ask for your agreement. Each gift is noted in the **Activity log**, agent by agent. The agents checked when the connection was added, under **Who can use it**, are given it the same way.

## On the sources side

The documents and folders an agent reads are its **sources**, checked in the **Sources** section of its sheet. **These are not MCP tools**: a source says what an agent may search, not which tool it may call.

An MCP server or a native connector is usable by an agent once its tools are granted to that agent, on its sheet.

## Checking what an agent can do

- **Accesses** section of the agent's sheet, full list.
- In the chat, ask the agent *"List the tools you can use"*.
- Test a concrete call. A tool the agent was not granted is not offered to it, so it answers that it cannot do it. A refusal that comes from the policy says that the organisation policy refused the action.

## If it does not work

- **The agent says it cannot use the tool**: the tool is not among its accesses. Open its sheet, click **Adjust** and grant it, or use another agent that already has it.
- **The agent sees the tool but the call fails**: this is most likely a permission issue, an expired token or a missing scope. See [Understand MCP permissions](understand-mcp-permissions.md) and [Manage OAuth tokens](manage-oauth-tokens.md).
- **Two agents should see the same tool and only one does**: check the **Accesses** of both, the tool has to be granted to each of them.

> **Technical reference:** [Apollia reference](/reference) , per-agent tool resolution.
