# Approve or reject an agent action

> For operators who want to keep control over every sensitive action triggered by an agent (write, command, external tool call).

> **Note on autonomy levels:** the approval flow depends on the level chosen at launch. At the `assisted` level (default), every sensitive action goes through your validation, as described on this page. From the `supervised` level on, the automatic verification loop can fix anomalies without asking you, and only submits for approval what resists that correction. The `bounded_autonomous` and `long_autonomous` levels reduce interruptions even further. See [Autonomy levels](../agents/choisir-un-palier-d-autonomie.md).

## Prerequisites

- An agent running a task that touches files, commands or external tools.
- You understand what the agent is supposed to do (the mission is clear to you).

## Where the approval request appears

An approval request can pop up in two places, depending on the context:

- **In the chat** (with Apollia Chat or a conversational agent): an **approval card** is inserted into the message flow, at the chronological position of the request. The card carries a shield icon ⛨ and an orange border.
  ![Inline approval card in the chat, with an orange shield icon, a preview of the command to authorise, and the Allow once, Refuse and Always allow buttons](/img/operator-help/controle-approuver-ou-refuser-une-action-1.png)

- **In the sidebar → Approvals** (which opens the **Inbox** page): for agents running in the background or that paused their task for a human check. Each request appears as a row in a list grouped by date (Today / Yesterday / Earlier). Clicking a row expands the **HITL card** with the details and the action buttons.
  ![Inbox page with the filter chips at the top and an expanded approval card showing its risk badge](/img/operator-help/controle-approuver-ou-refuser-une-action-2.png)

> **Note:** Requests appear **in real time** without refreshing. A counter in the page subtitle shows the total number pending.

## The three possible decisions

Whatever the entry point (chat or Inbox for a tool call), the available actions are the same:

1. **Allow once** - the action runs immediately, for this request only. The agent resumes, and the next occurrence will ask for confirmation again.

2. **Refuse** - a **Reject action** dialog opens. Enter an explanation of **5 to 500 characters** (counter at the bottom of the textarea) then confirm. The button only becomes active from 5 characters on.
   ![Reject action dialog with textarea, "12 / 500" counter, Cancel / Confirm rejection buttons at the bottom](/img/operator-help/controle-approuver-ou-refuser-une-action-3.png)

   The reason is **passed on to the agent**: it is injected into the tool message the LLM sees at the next iteration, in the form *"Tool refused by the user. Reason: ..."*. This lets the agent correct its trajectory instead of retrying blindly. The reason is also **persisted** in the recent history (see below) so you can find the context later.

3. **Always allow** - opens a menu with **4 scopes** to choose from:

   | Scope | Effect |
   |---|---|
   | **For this session** | Auto-approved until the chat is closed. Not persisted. |
   | **Always for this assistant** | Persisted rule - the current assistant will no longer ask for this tool. |
   | **Always for this project** | Persisted rule for every assistant used in the current project. *Disabled if the session is not attached to any project.* |
   | **Always, everywhere** | Persisted rule, globally - every assistant, every project. Shown in orange as a signal of the widest scope. |

   Persisted rules can be reviewed and revoked in **Settings → Permissions** (see [Manage tool permissions](configurer-les-permissions-de-fichiers.md)).

> **Special case of code executors** (`bash_executor`, `python_executor`): *Always allow* is never honoured for them, whatever scope you pick. Their argument is a shell command or arbitrary code; a blanket authorisation would be a blank cheque on the whole interpreter. The current call does run once, but the next invocation asks for confirmation again. To auto-approve a specific command, set up a targeted prefix rule in **Settings → Permissions**: it only applies to a single simple command (no chaining with `;`, `&&`, no pipe, redirection or substitution).

> **Special case of paused tasks** ("task approval"): an agent that suspends itself through a HITL checkpoint only exposes **Allow** / **Refuse** (no *Always allow*) since this is not a memorisable tool. The reason dialog is still mandatory on rejection.

## Steps - resolving a request

1. Click **Allow once** to validate this one occurrence, or open the **Always allow** menu to install a persistent rule.

2. To reject, click **Refuse**: the reason dialog opens. Type a short explanation that is useful to the agent (e.g. *"Wrong folder - use ./tmp instead"* rather than *"No"*), then click **Confirm rejection**.

3. The card disappears from the chat (or from the Inbox), and a toast confirms the decision (*"Action approved"* / *"Action rejected"* / *"Rule saved - future calls auto-approved"*).

4. In the chat, the agent immediately receives the result (rejection + reason, or the tool result) and continues its thinking at the next reasoning iteration.

## Review the decision history

At the bottom of the **Inbox** page, below the list of pending actions, a **Recent history (last 14 days)** section shows the **last 50** resolved HITL decisions (reverse chronological order):

- Coloured icon: ✅ Approved (green) · 🛡 Always approved (primary blue) · ❌ Rejected (red).
- Name of the tool involved.
- For rejections: the **reason you entered** at rejection time, in red.
- Relative timestamp (`5min ago`, `2h ago`…) with the absolute date in a tooltip.
- Short prefix of the originating session.

![Recent history section - four rows with different icons, one rejection with its reason shown in red](/img/operator-help/controle-approuver-ou-refuser-une-action-4.png)

The history is **read-only**; it does not replace the Settings → Permissions → Recent audit page, which also shows automatic decisions (triggered by persisted rules) over 20 entries.

## Verification

- The approval card disappears from the chat (or the Inbox row) immediately after your decision.
- A toast confirms the operation.
- If you chose **Always allow**, open **Settings → Permissions** and check that a new rule appears in the list, with the right scope.
- For a rejection, the agent should take the reason into account at its next iteration (you will see it in the rest of the conversation or in the agent logs).

## If it does not work

- **No card appears while the agent seems stuck**: open the **Inbox** from the sidebar. Background agents drop their requests there instead of showing them in the chat.
- **The agent keeps retrying the same rejected action**: the reason may not have been usable by the agent. Open its logs from **My Assistants**; the reason that was passed on shows up there in the rejected tool output. Reject again with a more actionable reason (alternative path, expected value…).
- **An "Always" rule creates too many automatic actions**: open **Settings → Permissions** and revoke or narrow the rule's scope. See [Manage tool permissions](configurer-les-permissions-de-fichiers.md).
- **The "Always for this project" option is greyed out**: the current chat session is not attached to any project. Link it from the chat header, or use the *Always for this assistant* scope instead.
- **Fewer approval requests than usual**: this is expected if the agent runs at the `supervised` level or above. The automatic verification loop resolves part of the situations without asking you. If you want full control back, relaunch the agent with `--autonomy assisted`.

> **Concept:** [Apollia explanation](/explanation)
