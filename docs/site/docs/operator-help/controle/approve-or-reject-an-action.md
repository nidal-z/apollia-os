---
title: Approve or reject an agent action
slug: /operator-help/control/approve-or-reject-an-action
sidebar_position: 1
---

# Approve or reject an agent action

> For operators who want to keep control over every sensitive action triggered by an agent (write, command, external tool call).

> **Note on autonomy levels:** the approval flow depends on the level chosen at launch. At the `assisted` level (default), every sensitive action goes through your validation, as described on this page. From the `supervised` level on, the automatic verification loop can fix anomalies without asking you, and only submits for approval what resists that correction. The `bounded_autonomous` and `long_autonomous` levels reduce interruptions even further. See [Autonomy levels](../agents/choose-an-autonomy-level.md).

## Prerequisites

- An agent running a task that touches files, commands or external tools.
- You understand what the agent is supposed to do (the mission is clear to you).

## Where the consent request appears

A consent request shows in two places:

- **In the chat**: a card is inserted into the conversation, at the place where the agent asked. It says what the agent wants to do and carries the answers.

- **In the sidebar, under To review**: one list gathers everything that waits for you, whichever conversation, task or batch it comes from. A request asked in a conversation shows in both places, and answering it in one removes it from the other. See [Use the To review list](../transversal/use-the-inbox.md).

> **Note:** requests appear without refreshing. The badge next to **To review** in the sidebar counts what is waiting.

## The three answers

A consent request has three answers and nothing else. There is no scope to pick.

1. **Once**: the action runs, for this request only. The next action of the same kind asks again.

2. **Always for this agent**: the action runs, and this agent may repeat this kind of action without asking you again. The consent holds for the agent that asked and for the family of the action (for example write or delete), not for the other agents. You withdraw it at any time on the **Permissions** screen (see [Manage tool permissions](manage-tool-permissions.md)).

3. **Refuse**: the action does not run. The field next to the button takes a reason, which is optional: leave it empty and click **Refuse**, or type a short sentence first (500 characters at most). The reason is passed to the agent, which lets it correct its course instead of trying the same thing again.

An action of the **Read** family needs no consent: the agent is not interrupted. The other families (**Read personal data**, **Write or delete**, **Send outside**, **Run code**) ask.

<!-- claim:chat-tool-governance-path -->
> **Special case of code** (the Bash and Python tools): **Always for this agent** is never offered for code. A command or a script can do anything the interpreter can, so a consent given to the whole family would be a blank cheque. By default every run asks for your consent, with **Once** and **Refuse** as the only answers.
<!-- claim:prefix-rules-evaluated-per-invocation -->
> With the developer mode on, a simple command also offers **Always allow this command**: this agent may then run exactly this command again without asking, and you withdraw that on the **Permissions** screen. The rule is checked on every run. It is not offered on a command that chains several (with `;`, `&&`, a pipe, a redirection or a substitution): every run of such a command asks for your consent.

<!-- claim:bash-executor-requires-posix-shell -->
> **On Windows**, these two tools also need host prerequisites: a POSIX shell on `PATH` for Bash (Git Bash, MSYS2 or WSL) and an installed Python 3 for Python. When one is missing, the call fails with a message naming the prerequisite instead of executing.

> **Special cases with two answers:** a task that paused to wait for you, and a fact an agent proposes for your profile, offer **Once** and **Refuse** only. Neither can be granted for good.

## Approving a plan (plan mode)

When the assistant works in **plan mode**, a distinct card appears in the chat once the plan is ready: the plan review card, listing the proposed steps with **Approve** and **Request changes** actions. This gate is separate from the consent requests above: it validates the whole plan before any execution starts.

<!-- claim:plan-approval-executes -->
Once you approve, the assistant **executes the approved plan** and does not re-propose it; if you approve while the assistant is still finishing its current turn, execution starts right after that turn ends. Requesting changes with a comment sends your feedback to the assistant, which revises the plan and submits it again through the same card.

## Steps - answering a request

1. Read what the agent asks for. For code, read the whole command: in **To review**, a long command is folded, and the answers appear once you clicked **Show the whole command**.

2. Click **Once** to let this one action through, or **Always for this agent** to stop being asked for this kind of action by this agent.

3. To reject, type a reason that is useful to the agent (for example *"Wrong folder, use ./tmp instead"* rather than *"No"*), then click **Refuse**.

4. The agent receives your answer at once and goes on: with the result of the action, or with your refusal and its reason.

## Review the decisions already taken

The **To review** list keeps no history: an answered request leaves it. Decisions already taken are read in the **Activity log**, from the sidebar. Every action of the agents and every change of access is written there, with its result: succeeded, failed or refused. See [Read the audit trail](../observabilite/read-the-audit-trail.md).

## Verification

- The card leaves the chat, and the line leaves **To review**, right after your answer.
- If you chose **Always for this agent**, open **Permissions**, select the agent, and check that the consent appears under **"Always" given**.
- For a refusal, the agent takes the reason into account in the rest of the conversation.

## If it does not work

- **No card appears while the agent seems stuck**: open **To review** from the sidebar. A task or a batch running in the background leaves its requests there.
- **The agent keeps retrying the same rejected action**: the reason may not have been usable by the agent. Refuse again with a more actionable reason (another path, the expected value).
- **An "always" lets too much through**: open **Permissions**, select the agent and click **Withdraw** on the consent. The next action of that family asks again. See [Manage tool permissions](manage-tool-permissions.md).
- **Always for this agent is missing from a request**: this is expected for code, for a paused task and for a fact proposed for your profile.
- **Fewer consent requests than usual**: this is expected if the agent runs at the `supervised` level or above. The automatic verification loop resolves part of the situations without asking you. If you want full control back, relaunch the agent with `--autonomy assisted`.

> **Concept:** [Apollia explanation](/explanation)
