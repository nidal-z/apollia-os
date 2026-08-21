---
title: An action was refused
sidebar_position: 3
---

# An action was refused

> For operators who see a refused action in the Inbox, or whose agent seems to have abandoned a task: understand why and unblock what comes next.

## Quick checks (in order of likelihood)

### 1. You refused the action yourself recently

Every manual refusal sends a clear message to the agent, which adapts or stops. That is the normal behaviour.

**Solution:**
1. In the sidebar, click **Inbox**.
2. The **To do** tab. Scroll down to the **Recent history (last 14 days)** section below the list of pending items.
3. Find the line with the ❌ **Rejected** icon matching the action. The **reason entered at the time of the refusal** is displayed in red as sub-text just below it. It tells the agent what to change.
   ![Inbox on the To do tab, with the recent history at the bottom showing a rejected line and its reason](/img/operator-help/troubleshooting-une-action-est-refusee-1.png)
4. If the refusal was a mistake, restart the agent or ask it in the chat to retry with a new instruction.

### 2. A persisted permission rule blocks the action

A rule created earlier (through the onboarding flow or the initial configuration) can refuse this type of action automatically without showing an approval card.

**Solution:**
1. In the sidebar, open **Settings → Permissions**.
2. Scroll to the **Recent audit section** at the bottom of the page: it lists the last 20 permission decisions (allowed / denied) with the tool, the scope, the number of the rule applied and the agent concerned. This is where you will identify whether a refusal comes from a persisted rule - the **decision** column shows `deny` and the next column tells which rule applied the refusal.
3. Find the rule in the main **Active permissions** list above, then click **Revoke** (trash icon) to delete it if it is the cause.

   > **Note:** there is no *"Always deny"* button in HITL cards - a refusal is always one-off. `deny` rules only come from the initial configuration or from a direct edit.

### 3. The agent has no access to the folder or the tool

Apollia restricts some sensitive paths and tools by default. An action on a forbidden path is refused without even showing an approval card.

**Solution:**
1. If you see the refusal line in the Inbox history, read the technical reason - it mentions the path or the tool concerned.
2. If the access is legitimate, open **Settings → Permissions** and use the *Always for this assistant* / *Always for this project* scope on the **next** approval (instead of a deny rule). See [Approve or refuse an agent action](../controle/approuver-ou-refuser-une-action.md).
3. Restart the task from the chat.

### 4. The same type of action is refused over and over

If an agent hits several refusals in a row, it may stop on its own. This often points to a badly phrased instruction rather than a permission blockage.

**Solution:**
1. In the Inbox → Recent history, count the consecutive refusals on the same tool/path.
2. Rephrase your request in the chat and state the allowed perimeter (for example: *"work only in `~/Reports`"*).
3. To automate future approvals for this type of action, use **Always allow → For this assistant / For this project** on the next approval card.

## If nothing works

1. **Overview**: the **Recent audit** section of **Settings → Permissions** shows the last 20 permission decisions with their tool, their decision (`allow` / `deny`), their scope, the number of the rule applied and the agent. If a run of unexpected refusals appears, the rule responsible is immediately visible.
2. **Revoke everything**: if the behaviour has become inconsistent, open **Settings → Permissions**, click **Revoke all** (red button at the top right) and select the scope concerned (*This project* / *Chat / agent* / *Everywhere* / *All scopes*). Confirm. Approvals will start over from scratch.
3. **Last resort:** disable the agent from its card (on/off toggle in **My Assistants**), delete all its dedicated rules (`agent_id` filter in Permissions), then re-enable it to start from a clean configuration.

> **Technical reference:** [Apollia reference](/reference) - understand how Apollia decides to approve, refuse or ask for each sensitive action.
