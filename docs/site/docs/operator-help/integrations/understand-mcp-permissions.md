---
title: Understand MCP permissions
slug: /operator-help/integrations/understand-mcp-permissions
sidebar_position: 7
---

# Understand MCP permissions

> For any operator who wants to know why a tool asks for an approval, how to change a rule, and what local-only mode does.

## Prerequisites

- At least one connector or MCP server connected.
- You know the difference between a native connector and an MCP server (see [Integrations overview](vue-d-ensemble-integrations.md)).

## Why this tool asks for an approval

Every tool exposed by a connector or an MCP has an **approval policy**:

| Policy | Behaviour | Use case |
|---|---|---|
| `auto_approve` | The tool runs immediately. | Reads with no side effect, `gcal.list_events`, `gdrive.workspace_list`, `outlook.search`. |
| `always_require_approval` | An HITL popup appears, you decide. | Writes, `gmail.send`, `outlook.send`, `gcal.create_event`. |
| `confirm_phrase` | HITL popup + you have to type a confirmation phrase (the name of the thing to delete, for instance). | Irreversible deletions, `gcal.delete_event`, `outlook_cal.delete_event`. |

The card carries three buttons, **Allow once**, **Refuse** and **Always allow**. The scope is a separate choice, offered next to them: *For this session*, *Always for this assistant*, *Always for this project* (unavailable when the session is attached to no project) or *Always, everywhere*. Answering **Always allow** on any scope but the session creates a **persistent rule** which you will find, and can revoke, under **Settings, Permissions**.

There is no MCP-specific approvals screen, and there is no queue of MCP requests to go through. A tool exposed by an MCP server goes through the same gate as a native tool, the one the execution loop opens: the card in the conversation while you are there, the **Inbox** for an agent task that is waiting. Everything the rest of this help centre says about approving an action applies to it unchanged.

![Approval popup in the chat: the tool title, the exposed parameters, the Allow once, Refuse and Always allow buttons, and the scope picker](/img/operator-help/integration-comprendre-les-permissions-mcp-1.png)

## Viewing and changing the rules

Open **Settings, Permissions**. Four sections:

- **Rules**: the persistent rules, created through the approval prompts, by the onboarding agent, or by hand. Filterable by scope (All, This project, Chat agent, Everywhere) and by tool. The rule's author is shown on each row but is not a filter.
- **Active sessions**: permissions valid only for the current chat session.
- **Apollia Chat**: the rules that apply to the free chat specifically.
- **Recent audit**: a read-only tail of the `permission_audit` table. Nothing in the runtime writes to that table in `v0.1.0-preview`, so the section stays empty however many approvals you answer. The record of what actually ran is the **Audit Trail** tab of the **Observability** page.

A **Revoke** button on each rule. To revoke every rule of a scope at once, the **Revoke all** button asks you to pick the scope and shows how many rules it would remove, then Cancel or Revoke. It asks for no typed confirmation.

The per-server approval level is not on this page: it lives in **Connections**, on the server's own **Settings** tab. It offers two choices, *Allow automatically* and *Ask me every time*. A read-only level was removed on purpose: it persisted the same byte as *Allow automatically*, so the most restrictive label produced the least protective setting.

![Settings, Permissions page: the permission rules stacked with a Revoke button on each row](/img/operator-help/integration-comprendre-les-permissions-mcp-2.png)

Rules are created two ways. Most of the time they appear on their own, when you answer an approval popup with "Always allow". You can also create one by hand from this page, through the **Add rule** form, which is the way to authorize something before an agent ever asks for it.

## What the sovereignty profile does

The **sovereignty profile** is a global decision, independent of the per-tool rules.

It is set in **Settings, Profile**, under **Data sovereignty**, and takes one of three values.

- **Cloud allowed** and **Local preferred**: starting a cloud OAuth connection is permitted.
- **Strictly local**: starting a cloud OAuth connection is refused, and so it is when the setting has never been answered, since an unanswered sensitive setting must not read as consent.

That is the whole of what the profile enforces in `v0.1.0-preview`. It is checked at one place, when you click **Connect** on a native connector, and nowhere else. It does **not** filter MCP servers: a remote HTTP or SSE server already installed keeps being reachable under *Strictly local*, and no agent ever receives a sovereignty error, because none is raised on the tool path. Treat the profile as a gate on connecting a cloud account, not as a network boundary.

## What an MCP server can and cannot ask of you

The specification lets a server call back to its client three ways. Apollia
answers none of them today, and advertises one.

- **Roots**, announced but not answered: Apollia advertises the capability during the handshake, and nothing answers a `roots/list` request. No directory is declared, so this is not a filesystem boundary and must not be read as one. What actually bounds a local server is the command and the arguments you gave it.
- **Sampling**, not implemented: a server cannot ask Apollia to make an LLM call on its behalf.
- **Elicitation**, not implemented: a server cannot ask you for structured input.

The last two are not advertised during the handshake, so a server discovers
their absence at connection time rather than by sending a request that goes
unanswered. Roots is the one case where the announcement runs ahead of the
implementation; the three are planned, and all three will be gated by your
approval when they arrive.

## Verification

- Open **Settings, Permissions**, the four sections are displayed.
- In the chat, trigger a write (sending a mail), the popup appears.
- Answer **Always allow** with the *Always for this project* scope, confirm, and check that a new row appears under **Rules**.

## If it does not work

- **A read-only tool asks for an approval when it should not**: the default policy has been hardened. Check under **Permissions** and restore the `auto` mode.
- **A sensitive tool runs without asking**: you answered **Always allow** on it one day, on a scope wider than the session. Go revoke that rule.
- **An MCP server stops answering after a sovereignty change**: the profile is not the cause. It filters no MCP transport at all, local or remote. Look at the server itself, through [Test an MCP connection](test-an-mcp-connection.md).

> **Technical reference:** [Apollia reference](/reference) , full governance, audit trail, rule format in `governance.db`.
