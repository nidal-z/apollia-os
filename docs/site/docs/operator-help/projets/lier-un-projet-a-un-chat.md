---
title: Link a project to a chat
sidebar_position: 2
---

# Link a project to a chat

> For operators who want an AI conversation to load a project's files, git history and documents automatically.

## Prerequisites

- A project created with at least one context provider enabled.
- An AI provider connected.
- (Optional) The project folder is a git repo, to enable the Git provider.

## Steps

1. **From the Projects page**: click **Projects** in the sidebar, open the project, then click **+ New Chat** at the top right.
   ![project detail page, + New Chat button highlighted](/img/operator-help/projets-lier-un-projet-a-un-chat-1.png)

2. The chat opens automatically. The project icon appears in its header: it is attached.

3. **From an existing chat**: open the chat, click the menu at the top (three dots), then click **Link to a project**.
   ![chat header, drop-down menu with the Link to a project option](/img/operator-help/projets-lier-un-projet-a-un-chat-2.png)

4. Select the target project in the drop-down list. The context is attached instantly.

5. Check the **context blocks** shown at the bottom of the chat. You can collapse or expand them to see what is actually handed to the AI.

6. Ask a project-specific question to validate, for example: *"Which files changed this week?"*. The answer must cite real files and commits.

7. You can create several chats linked to the same project. Each keeps its own history but shares the same context.
   ![project page with the list of linked chats, each with its title and date](/img/operator-help/projets-lier-un-projet-a-un-chat-3.png)

8. To unlink a chat, open its menu at the top and click **Unlink from the project**. The chat is kept, only the project context goes away.

## Verification

The project icon is visible in the chat header and the context bar shows the Git, File tree or Documents blocks you enabled.

## If it does not work

- **The + New Chat button is greyed out**: no AI provider is connected, open **Settings → LLM backends** to configure one.
- **The context blocks are empty**: go back to the project page and enable at least one context provider.
- **The AI seems to know nothing about the project**: the context is probably collapsed, expand it or reload the chat.

> **Concept:** [Apollia explanation](/explanation) - understanding how a project's context is used by the AI.
