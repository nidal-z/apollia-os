# Create a project

> For operators who want to group a working folder, its files and its chats under one reusable envelope.

## Prerequisites

- An AI provider connected (the connection is green in the top bar).
- You know which work you will drive from this project (website, study, client file, and so on).
- No folder to prepare: Apollia derives the workspace folder from the project name and creates it if it does not exist yet.

## Steps

1. In the sidebar, click **Projects**.

2. Click **+ New Project** at the top right.
   ![Projects page, + New Project button highlighted at the top right](/img/operator-help/projets-creer-un-projet-1.png)

3. Step 1 of 2, **Start from a template?**, offers two cards. Pick one, then click **Continue**.
   - **Blank project**: no pre-configured setup.
   - **Developer project**: pre-enables the code-oriented context providers (Git, file tree, APOLLIA.md).

4. Step 2 of 2, give your project a clear **Name** (for example: *Marketing website 2026*). This name appears in the sidebar, in the linked chats and in the notifications. A **Description** and a **Color** are also offered, both optional.
   ![The New Project modal, open on the step where you name the project](/img/operator-help/projets-creer-un-projet-2.png)

5. Click **Create project**. The project appears in the list immediately. Apollia derives its workspace folder from the name, under `Apollia/` in your home folder or in your documents, and creates that folder.

6. Click the project in the left column to open its **detail panel** on the right. A header carries the project name, the number of linked agents and the **New chat** button, and below it a tab bar of six tabs: **Conversations**, **Tasks**, **Agents**, **Memory**, **Context**, **Settings**.
   ![Project detail panel with its header and its six tabs, open on the Conversations tab](/img/operator-help/projets-creer-un-projet-3.png)

7. Carry on with **Enable context providers** to load the right information into your future chats automatically.

## Attach an agent to the project

The **Agents** tab holds the link between a project and the assistants you installed. Its counter is the one shown in the header.

1. Open the project, then the **Agents** tab.

2. Pick an assistant in the **Choose an agent to attach...** list. Only installed assistants appear, and those already attached are not offered twice.

3. Click **Add Agent**. The assistant is added to the list, with its description and a green dot when it is running.

4. To detach one, click the **✕** at the end of its row, then confirm on the **Detach** button that replaces the description. Nothing is uninstalled, only the link goes.

What attaching does, and what it does not do: it groups the assistant under the project and narrows the **Tasks** tab to its tasks. It changes nothing about how the assistant runs. The project instructions, its documents and its context providers are injected into the **chats opened from this project**, never into an agent's task.

## Attach a document

The **Memory** tab holds two stacked sections: the documents attached to the project, and the memory namespaces scoped to it.

1. Open the project, then the **Memory** tab.

2. Click **Attach a file** at the right of the **Memory documents** heading. The native file picker opens.

3. Pick the file. Apollia records its name, its path and its size. **The file is not copied**: it stays where it is, and Apollia reads it from that path each time it builds the project context.

4. To detach one, click the **✕** at the end of its row, then confirm. The message says it plainly: the file stays on disk, only the project's reference to it goes.

What a document is used for: when you open a chat from the project, the contents of each attached document are read from disk and added to the context of that conversation, under the document name. A file that has moved or been deleted is skipped silently, and a long document is cut off past 10 000 bytes with a *[truncated]* marker. A file Apollia cannot read as text brings nothing to the conversation.

## Verification

The project is listed in the sidebar under **Projects** and its **Settings** tab shows the workspace folder Apollia created for it.

## If it does not work

- **Nothing happens when you click Create project**: the name is required, a message says so at the top right when the field is empty.
- **The workspace folder is not the one you wanted**: open the project, go to its **Settings** tab and pick another one with **Choose folder…**.
- **The project appears but stays empty**: that is expected, the context providers are enabled in the next step.
- **The Tasks tab says tasks need an agent**: no assistant is attached yet. The button in that empty state takes you straight to the **Agents** tab.
- **The agent list offers nothing to attach**: every installed assistant is already attached, the list says so instead of staying blank. Install another one from **My Assistants**.
- **An attached agent shows "Agent not installed on this machine"**: the link survived the uninstall of the assistant. Detach it, or reinstall the assistant under the same name.
- **A document brings nothing into the conversation**: check that the file is still at the path it had when you attached it, and that it is a text file. Apollia skips what it cannot read, without an error.

> **Concept:** [Apollia explanation](/explanation) - understanding why a project acts as a context envelope for your chats.
