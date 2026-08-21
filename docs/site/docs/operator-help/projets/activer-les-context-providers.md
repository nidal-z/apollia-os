---
title: Enable context providers
sidebar_position: 3
---

# Enable context providers

> For operators who want the AI to arrive already briefed on their project, without pasting context into every message.

## Prerequisites

- A project already created.
- (For Git) The root folder is a git repo.

## Steps

1. In the sidebar, click **Projects**, then click the card of the project to configure. The detail panel opens.

2. Scroll down to the **Context Providers** section. Three provider types are available.
   ![Context Providers section in the project panel, provider list with ON/OFF toggle](/img/operator-help/projets-activer-les-context-providers-1.png)

3. **Git Status** (`git`) - click **Add provider** and select *Git Status*. When active, this provider injects the current git state (modified files, branch) into every message sent to the agents linked to the project.

4. **File tree** (`tree`) - add *Directory Tree* to include the file structure of the root folder in the context.

5. **Project Rules** (`rules`) - add *Project Rules (APOLLIA.md)* to automatically include the instructions from the `APOLLIA.md` file at the project root.

6. Switch each provider to ON or OFF depending on your needs.
   ![Git Status provider enabled (green toggle), Directory Tree provider disabled (grey toggle)](/img/operator-help/projets-activer-les-context-providers-2.png)

7. To see exactly what will be handed to the AI, click **Preview context** (Workspace Snapshot). A collapsible panel shows the content of each active provider.
   ![detailed preview of a context provider with git diff / file tree content](/img/operator-help/projets-activer-les-context-providers-3.png)

   > **⚠️ Not available in this version:** an "Injected context" banner with the estimated token total is not available in the interface yet. To estimate the size of the context, look at the preview and use the rough rule: 1 token ≈ 4 characters.

## Verification

Open a chat linked to the project and ask a precise question (for example: *"Which files changed this week?"*). The answer must cite real files and commits.

## If it does not work

- **The Git preview is empty**: your folder is not a git repo, or it has no commit. Initialize it or disable the provider.
- **The provider does not appear**: click **Add provider** to create it if it does not exist yet.
- **The context is too heavy**: disable the *Directory Tree* provider if the file structure is large.

> **Concept:** [Apollia explanation](/explanation) - knowing which provider to enable depending on the project type.
