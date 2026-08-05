# Understand MCP permissions

> For any operator who wants to know why a tool asks for an approval, how to change a rule, and what local-only mode does.

## Prerequisites

- At least one connector or MCP server connected.
- You know the difference between a native connector and an MCP server (see [Integrations overview](vue-d-ensemble-integrations.md)).

## Why this tool asks for an approval

Every tool exposed by a connector or an MCP has an **approval policy**:

| Policy | Behaviour | Use case |
|---|---|---|
| `auto_approve` | The tool runs immediately. | Reads with no side effect, `gmail.list_drafts`, `outlook.search`, `gcal.list_events`. |
| `always_require_approval` | An HITL popup appears, you decide. | Writes, `gmail.send`, `outlook.send`, `gcal.create_event`. |
| `confirm_phrase` | HITL popup + you have to type a confirmation phrase (the name of the thing to delete, for instance). | Irreversible deletions, `gcal.delete_event`, `outlook_cal.delete_event`. |

When you see the popup, you can tick *"Always allow for this project"*. That creates a **persistent rule** which you will find, and can revoke, under **Settings, Permissions**.

There is no MCP-specific approvals screen, and there is no queue of MCP requests to go through. A tool exposed by an MCP server goes through the same gate as a native tool, the one the execution loop opens: the card in the conversation while you are there, the **Inbox** for an agent task that is waiting. Everything the rest of this help centre says about approving an action applies to it unchanged.

![Approval popup in the chat: the tool title, the exposed parameters, the Allow once and Deny buttons, and the Always allow menu](/img/operator-help/integration-comprendre-les-permissions-mcp-1.png)

## Viewing and changing the rules

Open **Settings, Permissions**. Four sections:

- **Rules**: the persistent rules, created through the approval prompts, by the onboarding agent, or by hand. Filterable by scope (All, This project, Chat agent, Everywhere) and by tool. The rule's author is shown on each row but is not a filter.
- **Active sessions**: permissions valid only for the current chat session.
- **Apollia Chat**: the rules that apply to the free chat specifically.
- **Recent audit**: the last twenty tool decisions, most recent first.

A **Revoke** button on each rule. To revoke every rule of a scope at once, the **Revoke all** button asks you to pick the scope and shows how many rules it would remove, then Cancel or Revoke. It asks for no typed confirmation.

The per-server approval level (`auto` / `ask` / `readonly`) is not on this page: it lives in **Connections**, on the server's own **Settings** tab.

![Settings, Permissions page: the permission rules stacked with a Revoke button on each row](/img/operator-help/integration-comprendre-les-permissions-mcp-2.png)

Rules are created two ways. Most of the time they appear on their own, when you answer an approval popup with "Always allow". You can also create one by hand from this page, through the **Add rule** form, which is the way to authorize something before an agent ever asks for it.

## What local-only mode does

The **sovereignty profile** is a global decision, independent of the per-tool rules.

- **`cloud_allowed`** (default): every cloud connector (Google, Microsoft) is active, every remote MCP server is active.
- **`local_only`**: Google and Microsoft connectors disabled, remote HTTP and SSE MCP servers disabled. Only purely local stdio MCP servers stay available (Filesystem, Memory, SQLite, Git, Time).

When an agent tries to use a tool blocked by the profile, it gets the `SovereigntyBlocked` error. It can either ask for a profile change or pick an alternative tool.

In v0.1.0, the profile is set on the backend configuration side, not yet through a toggle in the interface.

## What an MCP server can and cannot ask of you

The specification lets a server call back to its client three ways. Apollia
implements one of them.

- **Roots**, implemented: Apollia declares the accessible directories to the server (the agent workspace, the project folder). The server sees nothing else on the filesystem side.
- **Sampling**, not implemented: a server cannot ask Apollia to make an LLM call on its behalf.
- **Elicitation**, not implemented: a server cannot ask you for structured input.

The two unimplemented capabilities are not advertised during the handshake, so a
server discovers their absence at connection time rather than by sending a
request that goes unanswered. Both are planned, and both will be gated by your
approval when they arrive.

## Verification

- Open **Settings, Permissions**, the 3 sections are displayed.
- In the chat, trigger a write (sending a mail), the popup appears.
- Tick *"Always allow for this project"*, confirm, and check that a new row appears under **Permission rules**.

## If it does not work

- **A read-only tool asks for an approval when it should not**: the default policy has been hardened. Check under **Permissions** and restore the `auto` mode.
- **A sensitive tool runs without asking**: you created a persistent permission rule by ticking the box one day. Go revoke it.
- **`local_only` blocks my local MCP**: check that your MCP really is on `stdio` transport. An MCP on `http://localhost:...` is blocked all the same (the profile filters by transport, not by host).

> **Technical reference:** [Apollia reference](/reference) , full governance, audit trail, rule format in `governance.db`.
