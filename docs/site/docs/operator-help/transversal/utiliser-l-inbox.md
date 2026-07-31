# Use the Inbox

> For operators who want to handle everything that needs their attention in one place: action approvals, agent questions, recent failures, and the history of notifications sent.

## Prerequisites

- At least one agent or trigger has run recently.

## Where to find it

In the sidebar, the **Inbox** entry (shield icon). The badge next to the label shows the number of **actions to handle** (pending approvals + questions). This counter stays strictly on concrete actions, it does not include informational events or past notifications.

## How the page is laid out

The page is organised into **three tabs**, reachable from the tab bar at the top:

| Tab | Content |
|---|---|
| **To do** | Pending HITL approvals + `ask_user` questions that agents are asking you. This is where blocking interactivity lives. |
| **Activity** | Recent events that deserve your attention without blocking an agent: task failures, trigger errors, agents running degraded, LLM provider unavailable. Rolling 14-day window. |
| **Notifications sent** | History of the notifications pushed to your Desktop / Webhook channels (last 50). |

The active tab is remembered between sessions (refreshing the page keeps your last view). You can also land there directly through a link (the **Settings → Notifications** page redirects to the *Notifications sent* tab, for instance).

## The "To do" tab

![To do tab - tab bar at the top with chip counters, filter chips (All / Approvals / Q...](/img/operator-help/transversal-utiliser-l-inbox-1.png)

### Filter the list

- **Three type chips** above the list: **All** / **Approvals** / **Questions**. Click to see only what interests you.
- **Agent selector** to the right of the chip row, isolates the work of a single agent.
- Items are **grouped automatically by day** (Today / Yesterday / Earlier), most recent at the top.

### Handle an approval

Click an approval row to expand it into a decision card. See the dedicated page [Approve or refuse an agent action](../controle/approuver-ou-refuser-une-action.md) for the detail of the 4 scopes of *Always allow* and of the refusal reason dialog (5 to 500 characters).

### Answer an agent question (`ask_user`)

When an agent uses the `ask_user` tool to question you, the item shows up in this tab under the **Question** label. Click to expand the dynamic form.

![ask_user form expanded - context in a blue callout at the top, a text question with a hint, a question wi...](/img/operator-help/transversal-utiliser-l-inbox-2.png)

The form displays, in order:

- A **blue callout** with the context supplied by the agent (if it supplied one, the *"why I am asking you these questions"*).
- One to ten **questions**, each rendered according to its type:
  - **Open question** - free text field, with a hint if the agent provided one.
  - **Single choice** - radio buttons, only one option selectable.
  - **Multiple choice** - checkboxes, several options possible.

Two actions at the bottom:

- **Reply** *(primary button)* - sends your answers to the agent, which resumes its execution. Fields left empty are marked *"not answered"* in the payload passed along, nothing blocks.
- **Decline to answer** *(red ghost)* - opens the reason dialog (5 to 500 characters). The agent receives your reasoned refusal and adapts what it does next.

> **Note:** a notification (desktop toast, or webhook if configured) goes out automatically when an agent calls `ask_user`. You can turn this notification off in **Settings → Notifications** by unchecking *Agent question*.

### Recent history

Below the list of pending items, a **Recent history (last 14 days)** section lists the last 50 resolved HITL decisions: ✅ Approved / 🛡 Always approved / ❌ Rejected (with the reason you typed). Read only.

## The "Activity" tab

![Activity tab - 4 filter chips All / Failures / Degradations / LLM, list of cards with a coloured icon...](/img/operator-help/transversal-utiliser-l-inbox-3.png)

This tab lists the events that did not call for immediate action but are worth a look. Four categories covered over the 14-day window:

- **Failures** ❌ - `task.failed` (agent task failed) and `trigger.error` (trigger in error).
- **Degradations** ⚠️ - `agent.degraded` (an optional tool is no longer available, the agent carries on with reduced capabilities).
- **LLM** 🔌 - `llm.backend_down` (AI provider unreachable).

Filter with the chips at the top. Each row offers a **View logs** button that opens **Observability → Timeline** pre-positioned on the task concerned when available.

> **Source:** this tab reads the same database as the Notifications page. An event only appears here if it triggered at least one delivery attempt towards a channel (global notification enabled). Otherwise, find it in **Observability → Timeline**.

## The "Notifications sent" tab

![Notifications sent tab - channel filter selector, 4-column table Timestamp / Channel / Even...](/img/operator-help/transversal-utiliser-l-inbox-4.png)

Table of the **last 50 notifications** pushed to your Desktop or Webhook channels, with:

- Relative **Timestamp** (with the absolute date in a tooltip).
- **Channel** - shown by its name (label) when set, otherwise by its identifier.
- **Event** - human label (*Task failed*, *Approval required*, and so on).
- **Status** - green *"sent"* badge or red *"failed"* badge.

A selector at the top filters by channel. To configure channels or global events, go to **Settings → Notifications** (see [Configure a channel](../notifications/configurer-un-canal.md)).

## Verification

- The "To do" tab empties out once you have resolved everything, and the sidebar badge drops back to zero.
- A new approval or agent question pushes the badge up **without a manual refresh** (real-time push).
- Refusing an item moves it immediately into the *Recent history* section.

## If it does not work

- **The sidebar badge stays at zero while an agent is waiting**: check the **state dot** next to the word *Apollia* in the top bar. If it is red, the runtime is disconnected; quit and reopen the application.
- **An `ask_user` item does not offer the form**: this is an edge case. Open the matching conversation (the session id is shown on the card); the agent is waiting there for an answer through the chat itself.
- **The Activity tab is empty while a task clearly failed**: the event was most likely not pushed into the notification chain (no channel subscribed). Open **Observability → Timeline** for the raw trace.
- **You want to avoid re-approving the same action over and over**: use the *Always allow* options on the card. See [Approve or refuse an agent action](../controle/approuver-ou-refuser-une-action.md).

## Requests coming from an MCP server

From v0.1.0 onwards, two kinds of events emitted by MCP servers can land in your inbox:

- **Structured input request** (MCP `elicitation/create`) - the server wants user input (dynamic form generated from a JSON Schema). Lands in **To do** in the same shape as a classic `ask_user`.
- **LLM sampling request** (MCP `sampling/createMessage`) - the server asks for an LLM call. The full prompt and the identifier of the source server are displayed before approval. Beyond 100 samplings per hour per server, requests are rejected automatically without reaching here.

Both kinds reuse the existing components (`AskUserForm` for elicitation, `HITLCard` for sampling). No new tab. More detail: [Understand MCP permissions](../integrations/comprendre-les-permissions-mcp.md).

> **Concept:** [Apollia explanation](/explanation)
