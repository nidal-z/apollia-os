# Read the activity timeline

> For operators who want to see what happened in the application over a given time window: tasks launched, tools called, approvals, LLM calls, memory, delegations between agents, errors.

## Prerequisites

- At least one task or agent has run recently.

## Where to look, depending on what you need

| You are looking for… | Go to… |
|---|---|
| A **snapshot of right now**: what is waiting for your decision, what has just been delivered, what is running | The **Dashboard**, the application home screen (section below). |
| The history of **one specific agent** (statuses, durations, task input/output) | **My Assistants → Logs** - see [View an agent's logs](../agents/consulter-les-logs-d-un-agent.md). |
| A **specific event** (an LLM call, a tool run, an approval) over a time window | **Observability → Timeline** (this page). |
| A **tool invocation** with its inputs and outputs | **Observability → Audit Trail** - see [Read the audit trail](consulter-l-audit-trail.md). |

## The dashboard, for the present moment

This is the screen the application opens on. Where the timeline answers
"what happened", the dashboard answers "where do things stand now".

Three cards side by side, and an activity strip below:

- **Decisions waiting** *(the widest one, on the left)*: the actions waiting for your approval. Counter in the header, compact list of the first items, and a *"See all →"* link to the **Inbox**.
- **Ready deliverables**: recently completed tasks. Clicking a row opens the **Tasks** tab.
- **At work**: the agents currently active. Clicking one opens the agent detail.

![dashboard in operator mode, three cards in a grid, Decisions waiting on the left spanning two columns](/img/operator-help/observabilite-lire-le-digest-quotidien-1.png)

Below the cards, **Recent activity** lists the latest tasks across all statuses as mini-cards, and leads to the **Tasks** page.

The counters update on their own: start a task and *"At work"* increments without a manual refresh. If everything stays empty although an agent has just run, the real-time connection has probably dropped; quit and reopen the application.

## The timeline, for what happened

1. In the sidebar, click **Observability**, then the **Timeline** tab.

2. At the top, **four KPIs** summarise the current window: Events · Tools · LLM calls · Errors (counter in red if > 0). The KPIs react to the filters: if you hide tools, their counter stays but the **Events** total goes down.

3. Choose the **time window**: **30 min / 1 h / 6 h / 24 h / 7 d**. Default: 1 h. Events reload automatically about every 15 seconds.
   ![Timeline tab: the KPI strip, the filter bar, then the events grouped by day](/img/operator-help/observabilite-consulter-l-historique-des-taches-1.png)

4. **Filter the events**:
   - **Type** - 7 rounded chips (Task / Tool / LLM / Approval / Memory / Delegation / Error). Each chip enables or disables its category; greyed-out chips are disabled.
   - **Agent** - dropdown selector to see only the events of one specific assistant. *All agents* by default.

5. Events are **grouped by day** with a header ("Today", "Yesterday" or the full date) and a counter on the right. Each row shows:
   - A **coloured dot** + **lucide icon** matching the type (ClipboardList for Task, Wrench for Tool, Bot for LLM, Hand for Approval, Brain for Memory, Link2 for Delegation, AlertTriangle for Error).
   - The **readable title** *"Task → completed"*, *"Tool: bash (2.1 s)"*, *"LLM: claude-sonnet-4 · $0.42"*…
   - A type **badge**, the **agent**, the exact **timestamp** (HH:MM:SS) **and** the relative age (*"3 min ago"*).

6. Click a row to **expand the raw payload** of the event (JSON formatted in monospace, including the `source` field that tells which SQLite database the event comes from). Click again to collapse.

## Verification

You find your recent runs in the chosen window. Widening the window from `1h` to `24h` brings up more older events without a manual refresh.

## If it does not work

- **The timeline is empty**: the default window (1 h) may hold no activity. Widen it to `24 h` or `7 d`. The timeline now scans each SQLite source directly by timestamp: tasks, tools (audit), LLM calls, HITL, trigger firings, chat session openings, reasoning and runtime errors. If it stays empty over `7 d`, no activity was recorded in that window.
- **My LLM calls made from Chat do not appear**: only the **opening and closing of the chat session** and its **tool approvals** appear. LLM calls internal to the chat are not (yet) persisted in `llm_calls.db` - known limitation (issue tracker). Chat sessions with an agent that starts a task do surface every event normally.
- **An expected event does not appear**: check the type chips and the agent selector - an active filter can hide the row. The type chips work additively: if all of them are greyed out, nothing shows.
- **I want the detail of a whole task, not a granular event**: go through **My Assistants → Logs** on the agent concerned. The timeline is deliberately granular and factual.

> **Concept:** [Apollia explanation](/explanation)
