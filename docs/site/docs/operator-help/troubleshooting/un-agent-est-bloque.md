---
title: An agent is stuck
sidebar_position: 4
---

# An agent is stuck

> For operators who see an agent in "Running" status with no progress for several minutes: identify the cause and get the work moving again.

## Quick checks (in order of likelihood)

### 1. The agent is waiting for your approval

By far the most frequent case. The agent tried a sensitive action (writing a file, sending an email, calling an external tool) and is waiting for your validation.

**Solution:**
1. In the sidebar, click **Inbox**.
2. The **To do** tab is selected by default. Filter on the **Approvals** chip to see only pending approvals, and use the **Agent** selector on the right to isolate the agent concerned.
   ![Inbox on the To do tab, with one approval card expanded to show what the agent is waiting for](/img/operator-help/troubleshooting-un-agent-est-bloque-1.png)
3. Click the item to expand it into a HITL card, then **Allow** / **Refuse**. The agent resumes its work immediately. See [Approve or refuse an agent action](../controle/approuver-ou-refuser-une-action.md) for the details.

### 2. The agent is waiting for an answer from an external tool

A call to an MCP server (Notion, GitHub, Slack) or to a remote API can take time if the service is slow or unreachable.

**Solution:**
1. In the sidebar, open **My Assistants**.
2. Find the card of the agent concerned, hover it to reveal the actions on the right.
3. Click the **⋯** menu → **View logs**. A sliding panel opens from the right with the task history.
4. Use the **search bar** at the top of the panel to type a tool name (`notion`, `github`, `fetch`, and so on) - only the tasks whose input or output mentions that tool stay visible.
5. If the last running task has been frozen on that tool for several minutes, open **Connections** in the sidebar and test the matching server.

### 3. A credential or an authorization expired

OAuth tokens (Google, Notion, GitHub) expire regularly. The agent then sits in a silent error loop.

**Solution:**
1. Open the agent's **Logs** panel (see step 2 above).
2. Filter on the **Failed** status in the chip bar. Type `401`, `403`, `expired` or `unauthorized` in the **search** to target authorization errors.
3. If you find some, go to **Connections** and reconnect the service concerned.

### 4. The agent loops on the same action

Some agents can get stuck on a step they retry indefinitely. Apollia applies a limit (StepBudget) whose ceiling depends on the autonomy tier: it is lower in `assisted` and higher in `bounded_autonomous` or `long_autonomous`.

**Solution:**
1. In the agent's **Logs** panel, look for several consecutive tasks with the same input or output.
2. On the agent card in **My Assistants**, click the **Stop icon** (inline action visible on hover). A confirmation appears.
3. Confirm the stop. The agent moves to **STOPPED** status. Restart it once you have fixed the cause (instructions too vague, missing tool).

   > **Note:** there is no separate *"Force stop"* button - the Stop icon sends a normal stop signal. If the agent does not react after a few seconds, restart the application.

### 5a. The agent takes longer than expected in the supervised or bounded_autonomous tier

In the `supervised` or `bounded_autonomous` tier, the agent checks its own work after execution and attempts a self-correction if needed. This lengthens the apparent duration before the agent declares itself done. It is normal behaviour, not a blockage.

If the duration looks excessive to you, open the **Logs** panel (see [View an agent's logs](../agents/consulter-les-logs-d-un-agent.md)). Verification has no status of its own: the task stays **Working** while the agent checks itself. What tells you the loop is failing to converge is a task that stays working far past the point where its result should have landed. Stop the agent, then rephrase the instructions or adjust the manifest.

### 5b. A dependency is missing (tool, file, model)

The agent may require an MCP tool that is not installed, a file that cannot be found or a local model that has not been downloaded.

**Solution:**
1. In the **Logs** panel, filter on **Failed** and type `not found`, `missing` or `unavailable` in the search.
2. Install the missing tool through **Connections**, or download the model from **Settings → Model Hub**.
3. Restart the agent from its card.

## If nothing works

1. Click the **Stop icon** on the agent card, then restart it.
2. If the blockage comes back immediately, disable the agent from the **My Assistants** list (on/off toggle on the card) to avoid any resource consumption.
3. **Collect the logs for support**: open the agent's **Logs** panel, click the **Copy icon** (to the left of the Refresh button in the header). The currently displayed tasks (filters + search taken into account) are copied to the clipboard as text, ready to paste into a ticket. A toast confirms the number of tasks copied.

> **Technical reference:** [Apollia reference](/reference) - understand the supervisor that watches agent progress and triggers timeouts.
