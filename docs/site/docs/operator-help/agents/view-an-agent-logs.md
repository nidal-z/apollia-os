---
title: View an agent's logs
slug: /operator-help/agents/view-an-agent-logs
sidebar_position: 4
---

# View an agent's logs

> To understand what an agent did, or why it failed: open its Logs panel and browse the history of its tasks.

## Prerequisites

- The agent is installed and has been started at least once.
- Ideally, the agent has already run a mission (at least a few tasks available).

## Steps

1. In the sidebar, click **My Assistants**.

2. Find the card of the agent whose activity you want to review.

3. Click **Logs** on its card. A panel opens on the right, titled **Agent Logs**, with a task counter at the top.
   ![Logs panel open with counter, search bar, status filters and sorting](/img/operator-help/agents-consulter-les-logs-d-un-agent-1.png)

   > **Note:** this panel shows the history of the tasks run by the agent. It is not a text journal with Info/Warning/Error levels.

4. Each row in the list already carries the essentials - no need to click to open a detail view:
   - **Status** (on the left) - see the list below.
   - Execution **duration** (e.g. `850ms`, `2.4s`, `1m 30s` past the minute).
   - **Relative timestamp** (e.g. `5min ago`) - hover it to see the exact date and time.
   - **Input received** - the request that triggered the task.
   - **Result** or **Error** - the output produced, or the error message if the task failed.

5. Identify tasks by their **status**:
   - **Completed** - task executed successfully.
   - **Failed** - task in error, to be examined.
   - **Working** - task still running.
   - **Approval** - the agent is waiting for a human decision (to be handled from the Inbox).
   - **Submitted** - task recorded, not yet picked up.
   - **Canceled** - task interrupted before the end.

6. **Filter the list** when there are many tasks:
   - Type in the **search bar** to keep only the tasks whose input or result contains that word.
   - Click a **status chip** (All / Completed / Failed / Working / Approval / Submitted / Canceled) to see only that status.
   - Use the **sort menu** at the top right of the filters to order by: Most recent (default), Oldest, Longest, Shortest.
   - The counter at the top shows the number of tasks displayed vs total (e.g. `4 / 27 tasks`).
   - If no task matches, a **Reset filters** button appears.

7. **Refresh** the list without closing the panel: click the `↻` icon at the top right of the panel.

8. Close the panel to go back to the agent list.

## Verification

You see the list of the agent's tasks with their status, their duration and a preview of the request and the result. A recently failed task is spotted at a glance thanks to its red **Failed** status and its error message shown right below.

## If it does not work

- **No task displayed:** the agent was never started or received no mission. Start it and send it an instruction.
- **Empty panel after a run:** check that the agent is indeed started (ACTIVE status on its card), then click `↻` to refresh.
- **Unintelligible error:** copy the message and see [An agent is stuck](../troubleshooting/an-agent-is-stuck.md).

> **Technical reference:** [Apollia reference](/reference) - interpreting task statuses, troubleshooting a stuck agent or an agent in timeout.
