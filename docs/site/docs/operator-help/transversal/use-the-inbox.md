---
title: Use the To review list
slug: /operator-help/transversal/use-the-inbox
sidebar_position: 2
---

# Use the To review list

> For operators who want to answer in one place everything their agents are waiting for: consent requests, questions, paused tasks and batch items.

## Prerequisites

- At least one agent, task or batch has asked for something.

## Where to find it

In the sidebar, the **To review** entry. The badge next to the label counts what is waiting for your answer. The same number heads the screen: *"3 items are waiting for your answer."*

## How the screen is laid out

**To review** is one list, with no tabs. It gathers the consents asked by agents in the conversations, the questions of agents, the tasks that paused to wait for you and the batch items that wait for an access, the most recent first. Answering here resumes the conversation, the task or the batch concerned.

Each item carries, above its card:

- **Where it comes from**: *From the conversation "…"*, *From a paused task* or *From a batch, item 4*.
- **A link back to it**: **Open the conversation**, **Open the agent** or **Open the batch**.
- **When it was asked**.

When nothing is waiting, the screen says so: *Nothing is waiting for your answer*.

## Filter the list

Four chips above the list, each with its count:

- **All**
- **Decisions**: the consent requests, the pauses of tasks included.
- **Questions**: what is answered with words.
- **Batch items**: the items of a batch that wait for an access.

## Answer a consent request

The card is the one of the chat, with the same answers: **Once**, **Always for this agent** and **Refuse**, with an optional reason. A paused task offers **Once** and **Refuse** only. See [Approve or reject an agent action](../controle/approve-or-reject-an-action.md) for what each answer covers.

A long command is folded: the list shows its first lines, and the answers appear once you clicked **Show the whole command**. You answer on what you read in full.

When you answer **Always for this agent** to a batch item, the other items of that agent waiting for the same kind of access run again, and a line says how many were resumed.

## Answer a question

When an agent asks you something, the item shows a card headed *"… asks you a question"*, with the **Question** badge. Under it, when the agent supplied one, a sentence gives the context.

Each question is drawn according to what the agent expects:

- **An open question**: a text field.
- **A choice among answers proposed by the agent**: one answer, or several when the agent allows it. When the agent accepts it, a field **Another answer…** sits beside the choices.
- **A yes or no question**: **Yes** and **No**.

Then:

- **Answer** sends what you filled in. The button becomes active as soon as one question carries an answer.
- **Decline to answer** refuses, with an optional reason that is passed to the agent.

## Take an answer back

Once you answered, the item shrinks to one line that says what you answered, with the mention *Answer recorded. This line leaves in a few seconds.* For those few seconds, **Cancel** takes the answer back and the item waits again. After that the answer is sent and the line leaves the list.

## Decisions already taken

There is no history here: a decision taken is read in the **Activity log**, from the sidebar. See [Read the audit trail](../observabilite/read-the-audit-trail.md).

The notifications Apollia sent are not listed here either. The last ones are shown in **Settings**, section **Notifications**, under **Recent sends**. See [Set up a notification channel](../notifications/set-up-a-channel.md).

## Verification

- The list empties out once you have answered everything, and the sidebar badge drops back to zero.
- A new request pushes the badge up without a manual refresh.
- An item answered in the chat leaves the list.

## If it does not work

- **The sidebar badge stays at zero while an agent is waiting**: check the **state dot** next to the word *Apollia* in the top bar. If it is red, the engine is disconnected; quit and reopen the application.
- **The answers of a request to run code are not shown**: the command is folded. Click **Show the whole command**.
- **The Answer button stays greyed out**: no question carries an answer yet.
- **You want to avoid answering the same request over and over**: answer **Always for this agent**. See [Approve or reject an agent action](../controle/approve-or-reject-an-action.md).

## Requests coming from an MCP server

**Nothing arrives here from an MCP server.** The specification defines
two ways a server can call back to its client, a structured input request
(`elicitation/create`) and an LLM call request (`sampling/createMessage`), and
Apollia implements neither. It does not tell servers it does, so a server that
supports them will simply not use them against Apollia.

What reaches this list comes from agents, tasks and batches: consent requests,
questions and pauses.

> **Concept:** [Apollia explanation](/explanation)
