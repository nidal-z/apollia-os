---
title: Understand MCP permissions
slug: /operator-help/integrations/understand-mcp-permissions
sidebar_position: 7
---

# Understand MCP permissions

> For any operator who wants to know why a tool asks for an approval, how to withdraw a consent, and what your organisation's policy forbids.

## Prerequisites

- At least one connector or MCP server connected.
- You know the difference between a native connector and an MCP server (see [Integrations overview](vue-d-ensemble-integrations.md)).

## Why this tool asks for an approval

Every tool exposed by a connector or an MCP belongs to an **action family**, and the family carries the rule:

| Family | Rule | Examples |
|---|---|---|
| **Read** | **Without asking**: the tool runs immediately. | A tool of a server with no account, which the server declares read-only. |
| **Read personal data**, **Write or delete**, **Send outside** | **Your consent is asked**: a card appears, you decide. | Reading a connected account, `gcal.list_events`, `outlook.search`. Writes and deletions, `gcal.create_event`, `gcal.delete_event`. Sends, `gmail.send`, `outlook.send`. |
| **Run code** | **Consent every time**. | A command or a script the agent wants to run. |

The card carries three answers, **Once**, **Always for this agent** and **Refuse**. There is no scope to pick and nothing to type. **Always for this agent** holds for the agent that asked and for the action family of the tool; you find it, and withdraw it, on the **Permissions** screen. A refusal accepts an optional reason, passed to the agent. For code, the lasting answer is **Always allow this command**: it is offered with developer mode on, for a single simple command, and every other run asks again. A tool its server does not describe is treated as sensitive and asked each time.

There is no MCP-specific approvals screen, and there is no queue of MCP requests to go through. A tool exposed by an MCP server goes through the same gate as a native tool, the one the execution loop opens: the card in the conversation while you are there, the **To review** screen for an agent task that is waiting. Everything the rest of this help centre says about approving an action applies to it unchanged.

## Viewing and changing the rules

Open **Permissions** in the sidebar. The screen lists the agents, and for each one:

- **Accesses granted, by family**: what the agent may use, with the rule of each family.
- **"Always" given**: the consents you answered **Always for this agent**, each with a **Withdraw** button.
- **Fine rules**: the rules on one precise command or tool, each with a **Withdraw** button. The **Rules for every agent** are listed apart.
- The decisions themselves are not listed on this screen: every consent answered and every refusal is an entry of the **Activity log**.

To withdraw several at once, **Revoke all** asks what is withdrawn (the fine rules, the "always" consents or both) and for which scope (every agent or one agent), then shows how many are concerned. It asks for no typed confirmation. Every withdrawal is recorded in the **Activity log**.

There is no approval level per server. In **Connections**, the **Settings** tab of a server holds its launch settings, its timeouts and **Disconnect**, and nothing about approval: a tool is decided by its action family, whichever server it comes from.

An "always" appears on its own, when you answer **Always for this agent** to a consent request. A fine rule can be written by hand from this screen, through **Add a fine rule**, with developer mode on.

## What the organisation policy forbids

Forbidding the cloud is a decision of your organisation, not a per-tool rule. It
is written in the signed policy IT may deploy on the computer, as
`connections.remote = "deny"`, and the runtime checks it.

- **No policy**: you are free. No connection is refused for being online.
- **A policy with `connections.remote = "deny"`**: adding a connection to an online service is refused, a Google or Microsoft account or a remote MCP server alike. The dialog says "Refused by your organisation's policy" and nothing is sent.

The policy is read when Apollia starts. A connection it forbids is not started: it is listed under **Forbidden by the policy** in **Connections**, and a call to one of its tools is refused. A policy can also forbid a kind of action or a tool by name, and that refusal holds whatever you answered before.

## What an MCP server can and cannot ask of you

The specification lets a server call back to its client three ways. Apollia
answers none of them today, and advertises one.

- **Roots**, announced but not answered: Apollia advertises the capability during the handshake, and nothing answers a `roots/list` request. No directory is declared, so this is not a filesystem boundary and must not be read as one. What actually bounds a local server is the command and the arguments you gave it.
- **Sampling**, not implemented: a server cannot ask Apollia to make an LLM call on its behalf.
- **Elicitation**, not implemented: a server cannot ask you for structured input.

The last two are not advertised during the handshake, so a server discovers
their absence at connection time rather than by sending a request that goes
unanswered. Roots is the one case where the announcement runs ahead of the
implementation.

## Verification

- Open **Permissions** in the sidebar, the list of agents is displayed.
- In the chat, trigger a write (sending a mail), the consent request appears.
- Answer **Always for this agent**, and check that a new row appears under **"Always" given** for that agent.

## If it does not work

- **A tool that only reads asks for your agreement**: it reads a connected account, which is personal data, or its server did not describe it and it is treated as sensitive. Answer **Always for this agent** to stop the question for that agent.
- **A sensitive tool runs without asking**: you answered **Always for this agent** on its family one day. Open **Permissions** and withdraw it.
- **A connection stops answering after a policy was deployed**: check whether it is listed under **Forbidden by the policy** in **Connections**. If it is not, look at the server itself, through [Test an MCP connection](test-an-mcp-connection.md).

> **Technical reference:** [Apollia reference](/reference) , full governance, audit trail, rule format in `governance.db`.
