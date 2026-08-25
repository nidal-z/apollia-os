---
title: Chat with your AI
slug: /operator-help/chat/chat-with-your-ai
sidebar_position: 1
---

# Chat with your AI

> For any operator who wants to start a dialogue with their AI: open a conversation, send messages and get answers that fit the context.

## Prerequisites

- An AI provider is connected (green pill in the top bar).
- (Optional) A project is linked so your working context is injected automatically.
- (Optional) An Assistant agent is started if you want to talk to a specialised agent.

## Steps

1. In the sidebar, click **Chat**. The list of your conversations shows on the left, the input area in the middle.
   ![Chat page, conversation sidebar on the left, empty area in the middle with the input field at the bottom](/img/operator-help/chat-discuter-avec-votre-ia-1.png)

2. Click **New chat** at the top of the list. A blank conversation opens.

3. (Optional) To pick the **AI provider** and the **mode** (Free or a specific Agent), click the configuration button at the top of the conversation. A panel opens with the available options.

4. Type your instruction in plain language in the input field at the bottom. Be specific: *"Summarise this file in 5 bullet points"* works better than *"Help me"*.

5. Press **Enter** or click **Send**. The answer streams in, word by word.
   ![conversation with a user message and an AI answer streaming in, markdown formatting rendered](/img/operator-help/chat-discuter-avec-votre-ia-2.png)

![conversation with a user message and an AI answer streaming in, markdown formatting rendered (continued)](/img/operator-help/chat-discuter-avec-votre-ia-2bis.png)

6. Ask your follow-up questions in the same thread. The AI keeps the whole conversation history.

<!-- claim:chat-timeline-follows-execution-order -->
7. Above the answer, a summary line says how much the turn thought and how many tools it used. Under it, the turn is laid out **in the order it happened**: a thought, then the action it led to, then the next thought, and so on. Every row is collapsed and expands to its own detail, the reasoning as written, a tool call as a plain-language account in Operator mode or its raw input and output in Builder mode.
   ![answer bubble with the summary line and the ordered timeline of thoughts and tool calls](/img/operator-help/chat-discuter-avec-votre-ia-3.png)

<!-- claim:failed-tool-call-is-marked-failed -->
8. A tool call that fails is marked as such, with a red cross rather than a green check, and stays marked when you reopen the conversation later. A call you refused is shown as refused, which is a different thing from one that ran and failed.

9. If the AI wants to perform a sensitive action (write a file, run a command), an approval card appears: see [Approve or reject an action](../controle/approuver-ou-refuser-une-action.md).

10. To organise your conversations, click the menu at the top of the conversation: **Rename** or **Delete**.

<!-- claim:context-gauge-engine-usage -->
11. Under the input field, a small **Ctx** gauge tracks how full the model's context window is. The percentage comes from the **token counts the engine itself reports** on each answer; when the backend reports none (an assistant in Agent mode, for instance), the gauge shows `--` rather than a made-up number. Past 90 % the gauge turns amber: consider starting a fresh conversation, or let the automatic compaction summarise the oldest turns.

## Verification

Your conversation shows up in the list on the left with a title and the date of the last message. You can reopen it at any time, the history is kept.

## If it does not work

- **No answer:** check that the provider pill is green in the top bar.
- **Answer in error or truncated:** switch models in the configuration panel, or read [The AI provider does not respond](../troubleshooting/le-fournisseur-d-ia-ne-repond-pas.md).
- **The AI does not know about your files:** link the conversation to a project and enable the context providers.

> **Concept:** [Apollia explanation](/explanation) - understand how context is injected and how the Free and Agent modes differ.
