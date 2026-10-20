---
title: Hand a task to an agent
slug: /operator-help/agents/start-an-agent
sidebar_position: 2
---

# Hand a task to an agent

> For anyone who has an agent installed and wants it to work: talk to it, give it a series of items, or let an automation call it.

## Prerequisites

- The agent is listed in the **Agents** screen.
- A model answers. Models are set up in the **AI** screen.

## An agent is never started nor stopped

There is no start button and no stop button. An agent is loaded by its first task and released once it is idle. What the **Agents** screen shows is whether a task can reach the agent:

- **available**: the agent takes a task.
- **degraded**: the agent still takes a task, with a warning.
- **unavailable**: the agent was switched off from the command line. `apollia-os agent start <agent-id>` makes it available again.

The built-in agent **Apollia Chat**, at the top of the **Agents** column, is always available.

## Steps - Talk to an agent

1. In the sidebar, open **Agents**.

2. On the row of the agent, click **Chat**. A conversation with this agent opens in the **Chat** screen.

3. Type your request and send it. The agent answers in the conversation, and the steps of its work are shown as it goes.

You can also click the row to open the sheet of the agent, then **New chat** at the top of the sheet. From the **Chat** screen, the selector of the conversation lists your agents: choosing one opens a new conversation with it.

## Steps - Give an agent a series of items

1. Open **Agents** and click the row of the agent to open its sheet.

2. Click **Process a batch** at the top of the sheet. The window that opens hands the items of a source to the agent one at a time, each item a request of its own.

3. Under **Source**, choose where the items come from:
   - **Excel or CSV file**: a row of the workbook becomes an item. **Choose a file…** picks it on this computer. A preview of the workbook lets you choose the sheet, the header row and the columns sent to the agent.
   - **Folder**: a file of the folder becomes an item. Pick a connected folder, or **Choose a folder…** on this computer. The agent receives the text of each file, read from PDF, Word, PowerPoint, workbooks, CSV and plain text files. Tick **Read the sub-folders too** to take the files of its sub-folders as well. The folder is only read: nothing is written into it.
   - **Mailbox**: a received message becomes an item. The list holds the IMAP mailboxes and the Microsoft account (read as Outlook) of your **Connections**. **Add a mailbox…** opens **Connections**. Tick **Unread messages only** to leave the read messages out.

4. Under **What the agent does for each item**, write the instruction sent with every item.

5. Under **Result columns**, give each column a name and say what the field holds. **Add a column** adds one. The agent answers each item with exactly these fields, and each field becomes a column. For a file source, the columns are written into a copy of the workbook. With an .xlsx or .xlsm workbook, you can choose instead **In the original file, after a backup**. For a folder or a mailbox, the results are kept with the batch, and you export them from its view.

6. Under **At the end of the batch (optional)**, say what the agent writes once every item has run, for example a ranking or a recommendation. The agent writes this synthesis from the table of results. It is shown on the view of the batch and, for a file or a folder source, written as a Markdown file beside the source, named in the language of the interface: `<name> (synthesis YYYY-MM-DD).md` in English.

7. Under **Accesses to grant for the duration of the batch**, tick the sensitive accesses the agent may use without asking you, until the batch ends. Without a grant, the item concerned waits for your decision in **To review**, and the batch goes on.

8. Under **Failure threshold**, choose after how many failures the batch interrupts itself, or **Never interrupt**.

9. Click **Launch the batch**. The view of the batch opens and follows it item by item. See [The view of a batch](../screens/batch.md).

Every batch stays in **Batch history**, at the top of the **Agents** screen. An Excel or CSV file dropped in the Chat also offers **Process each row**, which opens the same window.

## Steps - Let an automation call the agent

1. Open the sheet of the agent and click **Automate**. The window that opens is prefilled with this agent as its target.

2. Say when the automation must run, then create it. It is listed in the **Automations** screen. See [Schedule a trigger](../automatisations/schedule-a-trigger.md).

## Steps - Launch a workflow

1. In the sidebar, open **Agents**. Workflows are listed in the **Workflows** column.

2. On the row of the workflow, click **Launch**.

3. Fill in the inputs the workflow asks for, then launch it. Its sheet shows where each step of the run stands.

## Choose the autonomy level of a run from the command line

From the command line, a run takes an autonomy level with the `--autonomy` flag:

```
apollia-os run <agent-id> "<your request>" --autonomy <tier>
```

The agent identifier is required. The request text is optional and defaults to
empty, which is rarely what you want.

The four available levels:

| Level | Behavior |
|---|---|
| `assisted` | The plan gate is armed: you approve the plan once before it runs. No verification pass. |
| `supervised` | Plan gate armed, plus one verification pass after the run finishes. |
| `bounded_autonomous` | **The plan gate is bypassed**: the plan runs without your approval. Wider step budget, one verification pass at the end. Pass `--plan` to re-arm the gate for a run. |
| `long_autonomous` | Same bypass as above, maximum budget. Reserved for tasks that tolerate running unattended end to end. |

At every level, a filesystem write the runtime judges risky raises its own
approval prompt, independently of the plan gate.

If you omit the flag, the run takes the level of the agent, and `assisted` when the agent names none.

> For the detail of the levels and their guarantees, see [Autonomy levels](choose-an-autonomy-level.md).

## Verification

- **Conversation**: your message gets an answer that arrives progressively.
- **Sheet of the agent**: the **Activity** tab lists the run under **Runs**, with its input, its output and its steps.

## If it does not work

- **The row has no Chat button, and New chat is greyed out:** the agent is **unavailable**. Make it available again with `apollia-os agent start <agent-id>`.
- **The agent reads "degraded":** it still takes a task. Open its logs to read what went wrong, see [View an agent's logs](view-an-agent-logs.md).
- **No answer comes:** check in the **AI** screen that a model is installed or that your server answers.
- **The agent asks for your agreement and waits:** answer in the conversation, or in the **To review** screen.
- **The agent stops too soon:** the step budget of the level has been reached. See [Autonomy levels](choose-an-autonomy-level.md).
- **The agent takes the task but never finishes:** see [An agent is stuck](../troubleshooting/an-agent-is-stuck.md).

> **Concept:** [Apollia explanation](/explanation) - what an agent, a workflow and a package are.
