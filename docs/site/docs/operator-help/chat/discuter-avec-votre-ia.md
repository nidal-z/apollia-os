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

7. If you are talking to an Assistant, the reasoning steps show up **inline** in the message bubbles as expandable reasoning cards (not in a separate right-hand pane).
   ![answer bubble with an expanded reasoning card showing the agent's steps](/img/operator-help/chat-discuter-avec-votre-ia-3.png)

8. If the AI wants to perform a sensitive action (write a file, run a command), an approval card appears: see [Approve or reject an action](../controle/approuver-ou-refuser-une-action.md).

9. To organise your conversations, click the menu at the top of the conversation: **Rename** or **Delete**.

## Verification

Your conversation shows up in the list on the left with a title and the date of the last message. You can reopen it at any time, the history is kept.

## If it does not work

- **No answer:** check that the provider pill is green in the top bar.
- **Answer in error or truncated:** switch models in the configuration panel, or read [The AI provider does not respond](../troubleshooting/le-fournisseur-d-ia-ne-repond-pas.md).
- **The AI does not know about your files:** link the conversation to a project and enable the context providers.

> **Concept:** [Apollia explanation](/explanation) - understand how context is injected and how the Free and Agent modes differ.
