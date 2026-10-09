---
title: An agent is stuck
slug: /operator-help/troubleshooting/an-agent-is-stuck
sidebar_position: 4
---

# An agent is stuck

> For operators who see an agent at work with no progress for several minutes: identify the cause and get the work moving again.

## Quick checks (in order of likelihood)

### 1. The agent is waiting for your answer

By far the most frequent case. The agent tried a sensitive action (writing a file, sending an email, calling an external tool) and is waiting for your consent, or it asked you a question.

**Solution:**
1. In the sidebar, click **To review**.
2. Everything that waits for you is in one list: consents, questions, paused tasks and batch items. The **Decisions** filter keeps the consents only, and each item says where it comes from.
3. Answer on the card: **Once**, **Always for this agent** or **Refuse**. The conversation, the task or the batch concerned resumes at once. See [Approve or refuse an agent action](../controle/approve-or-reject-an-action.md) for the details.

### 2. The agent is waiting for an answer from an external tool

A call to an MCP server (Notion, GitHub, Slack) or to a remote API can take time if the service is slow or unreachable.

**Solution:**
1. In the sidebar, open **Agents**.
2. Find the agent concerned in the list and right-click it.
3. Choose **Logs**. A sliding panel opens from the right with the history of its tasks.
4. Use the search field at the top of the panel to type a tool name (`notion`, `github`, `fetch`, and so on): only the tasks whose input or result mentions that tool stay visible.
5. If the last running task has been frozen on that tool for several minutes, open **Connections** in the sidebar and test the matching connection.

### 3. A credential or an authorization expired

OAuth tokens (Google, Notion, GitHub) expire regularly. The agent then fails on every call to that service.

**Solution:**
1. Open the **Logs** panel of the agent (see check 2 above).
2. Filter on the **FAILED** status. Type `401`, `403`, `expired` or `unauthorized` in the search field to target authorization errors.
3. If you find some, go to **Connections** and reconnect the service concerned.

### 4. The agent loops on the same action

Some agents can get stuck on a step they retry again and again. Apollia applies a limit on the number of steps, whose ceiling depends on the autonomy tier: it is lower in `assisted` and higher in `bounded_autonomous` or `long_autonomous`.

**Solution:**
1. In the **Logs** panel of the agent, look for several consecutive tasks with the same input or result.
2. If the work runs in a conversation, open that conversation and click **Stop** in the message box.
3. Ask again once you have fixed the cause (instructions too vague, missing tool).

   > **Note:** an agent has no start, stop or on/off control of its own: it is loaded by its first task and released once idle. If a task that does not run in a conversation never ends, quit Apollia and open it again.

### 5a. The agent takes longer than expected in the supervised or bounded_autonomous tier

In the `supervised` or `bounded_autonomous` tier, the agent checks its own work after execution and attempts a self-correction if needed. This lengthens the apparent duration before the agent declares itself done. It is normal behaviour, not a blockage.

If the duration looks excessive to you, open the **Logs** panel (see [View an agent's logs](../agents/view-an-agent-logs.md)). Verification has no status of its own: the task stays **WORKING** while the agent checks itself. What tells you the loop is failing to converge is a task that stays working far past the point where its result should have landed. Stop the work, then rephrase the instructions.

### 5b. A dependency is missing (tool, file, model)

The agent may require a connection that is not set up, a file that cannot be found or a local model that has not been downloaded.

**Solution:**
1. In the **Logs** panel, filter on **FAILED** and type `not found`, `missing` or `unavailable` in the search field.
2. Add the missing connection from **Connections**, or download the model from the **AI** screen with **Discover models**.
3. Ask the agent again.

## If nothing works

1. Stop the work in its conversation, then ask again with a shorter, more precise request.
2. If the blockage comes back immediately, open the agent in **Agents** and read its sheet again: its mission, its model, its accesses and its sources.
3. **Collect the logs for support**: open the **Logs** panel of the agent and click **Copy logs** in its header, next to **Refresh**. The tasks currently displayed (filters and search taken into account) are copied to the clipboard as text, ready to paste into a report. A message confirms the number of tasks copied.

> **Technical reference:** [Apollia reference](/reference).
