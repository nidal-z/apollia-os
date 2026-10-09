---
title: An action was refused
slug: /operator-help/troubleshooting/an-action-was-refused
sidebar_position: 3
---

# An action was refused

> For operators whose agent reports a refused action, or seems to have abandoned a task: understand why and unblock what comes next.

## Quick checks (in order of likelihood)

### 1. You refused the action yourself recently

Every refusal is passed to the agent, with the reason when you typed one, and the agent adapts or stops. That is the normal behaviour.

**Solution:**
1. In the sidebar, click **Activity log**.
2. Use the **Agent** filter to keep the agent concerned, and the **Result** filter to keep the **Refused** entries.
3. Find the line that reads *You refused an action of* followed by the name of the agent. Click it to read its detail: the action, its target, its family and when it happened.
4. If the refusal was a mistake, ask the agent again in the chat. The request for your consent comes back, and you answer **Once** this time.

### 2. A rule refuses the action

A rule can refuse a kind of action without asking you. The Activity log then reads *A rule refused an action of* followed by the name of the agent.

**Solution:**
1. In the sidebar, open **Permissions**.
2. Choose the agent concerned, or **Rules for every agent**, and look under **Fine rules** for a rule whose effect is **Refuses**.
3. Click **Withdraw** on that rule if it is the cause. The withdrawal is recorded in the Activity log, and the action is again subject to the accesses of its family.

   > **Note:** no answer on a consent card refuses for good: **Refuse** is always one-off. A rule that refuses is written by hand on the **Permissions** screen, which offers it with the developer mode on.

### 3. The agent does not have this access, or the organization forbids it

What an agent may do is granted by action family: read, write or delete, send outside, run code. An action outside its accesses is refused without asking you. When the Activity log reads *The organization policy blocked an action of*, the refusal comes from the policy of your organization instead.

**Solution:**
1. On the **Permissions** screen, choose the agent and read **Accesses granted, by family**.
2. If the access is legitimate, click **Open the sheet**, then **Adjust** on the sheet of the agent, and grant it in the **Accesses** section. See [Approve or refuse an agent action](../controle/approve-or-reject-an-action.md).
3. Ask again from the chat. For a refusal by the **Organization policy**, ask the person who manages Apollia for you: the policy cannot be lifted from your computer.

### 4. The same type of action is refused over and over

If an agent hits several refusals in a row, it may stop on its own. This often points to a badly phrased instruction rather than a permission blockage.

**Solution:**
1. In the **Activity log**, count the consecutive **Refused** entries of that agent on the same action.
2. Rephrase your request in the chat and state the allowed perimeter (for example: *"work only in `~/Reports`"*).
3. To stop being asked for this kind of action, answer **Always for this agent** on the next consent card.

## If nothing works

1. **Overview**: the **Activity log** lists every action of the agents and every access change, newest first. Filtered on one agent, it shows a run of unexpected refusals and what refused each one: you, a rule or the policy.
2. **Withdraw everything**: if the behaviour has become inconsistent, open **Permissions** and click **Revoke all**. Choose what is withdrawn (the fine rules, the "always" consents or both) and the **Scope** (every agent or one agent), then confirm. The accesses granted by family do not change, and the consents will be asked again.
3. **Last resort:** uninstall the agent from the **Agents** screen, then install it again to start from the accesses it declares.

> **Technical reference:** [Apollia reference](/reference).
