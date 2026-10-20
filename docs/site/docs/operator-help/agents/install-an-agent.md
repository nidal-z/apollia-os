---
title: Install an agent
slug: /operator-help/agents/install-an-agent
sidebar_position: 1
---

# Install an agent

> For anyone who wants one more agent in Apollia: install a package you received, start from an example, or create an agent by describing the work.

## Prerequisites

- To install a package: the package file, whose name ends in `.apo`, is on this machine.
- To create an agent: a model answers. Models are set up in the **AI** screen.

## Three ways to get an agent

The **Agents** screen lists your agents in one column and your workflows in the other. A new agent arrives in one of three ways:

- **Install a package**: someone handed you a `.apo` file. A package brings one or several agents, workflows and tools at once.
- **Start from an example**: Apollia proposes ready-to-use agents, installed only if you choose to.
- **Create an agent**: you say what the agent is for and Apollia writes its sheet.

## Steps - Install a package

1. In the sidebar, open **Agents**.

2. At the top right, click **Install a package**. A file picker opens, limited to `.apo` files.

3. Choose the file. A preview window opens. Nothing is installed before you agree.

4. Read the preview:
   - **Publisher**: who made the package, and whether its signature confirms it. **Trust this publisher** adds a signed publisher to your trusted ones.
   - **What the package brings**: the agents, workflows and tools it contains.
   - **Accesses asked for, by family**: what its agents are allowed to do (read, write or delete, send outside, run code). The sensitive families are marked.
   - **Connections**: the connections the package needs, each marked as connected or missing. A missing one can be added now or later.

5. Click **Install**. For a package that is not signed, the window says so and the button reads **Install anyway**: go on only if you know where the file comes from.

6. A message confirms the installation. The agents of the package appear in the **Agents** column and its workflows in the **Workflows** column.

Opening the file of a newer version of a package already installed updates it. The window then puts forward the new accesses and the new connections, to tick before you click **Update**. The settings you made on the sheet of the agent are kept.

## Steps - Start from an example

1. In the sidebar, open **Agents**.

2. Under the two columns, click **See the examples**. When you have no agent and no workflow yet, the examples are already shown.

3. On the card of an example, click **Install**. The same preview as for any package opens.

4. Once the example is installed, its card offers **Open the sheet**. Some examples also offer **Try with the example**: Apollia copies a made-up demonstration dataset into the folder you choose, so you can see the agent work without real data.

## Steps - Create an agent

1. In the sidebar, open **Agents**, then click **Create an agent** at the top right.

2. Fill in the form. Only the first field is needed:
   - **What is this agent for?**: describe the work as you would hand it to a colleague.
   - **What it must give back**: the content and the form of its answer.
   - **What it must always or never do**: the rules it does not depart from.
   - **Requests it will receive**: one per line, as you would write them.
   - **What it may consult**: the sources of this machine, your profile and a memory of its own among them.
   - **What it may do, beyond reading**: with nothing checked, the agent reads and answers.

3. Click **Prepare the sheet**. Apollia reads your request and asks you up to three questions about what the form leaves unsaid. Each question comes with answers to pick. You can also write your own answer, or click **Skip**.

4. The draft of the sheet opens under **Review the draft**. Give the agent a **Name** and adjust what Apollia wrote: mission, model, accesses and sources all stay editable. **Back to the form** returns to the form.

5. Click **Save the agent**. No agent is saved before this click.

6. The sheet of the new agent opens. **Try it in the Chat** starts a conversation with it.

To write the sheet without the questions, click **Fill in the sheet myself** on the form.

## Leave a creation and come back to it

A creation under way is kept as a draft. Leaving the screen loses nothing: the form, your answers and the sheet in progress wait for you. Closing the application keeps the form and a sheet already under review; a preparation still going stops, and you launch it again from the form as you typed it.

- While Apollia prepares the sheet, you can leave: the preparation goes on while you are away. When you come back, it is followed again where it stands, and a question it asked meanwhile waits for your answer.
- The **Agents** screen shows **An agent is being created**, with where the creation stands: the form started, the sheet being prepared with a question that may be waiting for you, or the sheet to review before saving.
- **Resume** reopens the creation where you left it.
- **Discard the draft** erases the form, the answers and the sheet in progress, and stops the preparation, once you confirm with **Discard**.

Only **Save the agent**, or a confirmed discard, ends the draft.

## Verification

- The agent is listed in the **Agents** column, and its sheet shows the mention **available**.
- Its row offers **Chat**, and its sheet opens when you click the row.

For the next step, see [Hand a task to an agent](start-an-agent.md).

## Remove an agent

1. Open **Agents** and click the row of the agent to open its sheet.

2. Click **Uninstall** at the top of the sheet. The header turns into a confirmation that names the agent.

3. Tick **Also delete agent memory and data** if its memory is to go with it.

4. Click **Delete**. The agent leaves the list.

The built-in **Apollia Chat** agent follows the version of the application: it has no uninstall.

## If it does not work

- **"This file cannot be read as a package."**: the file is not a package, or it is damaged. Ask whoever sent it for a new copy.
- **Installation refused, invalid signature**: the file may have been altered after it was signed. Nothing was installed. Ask its publisher for a new copy.
- **Installation refused by your organisation**: the policy of your organisation requires signed packages, or does not recognise this publisher. **See the policy** opens the rules in force.
- **"An agent already has this name"**: an agent of the same name exists on this machine. The window says whether it was created in the application or installed by another package.
- **"This version is older"**: the file holds a version older than the one installed. Nothing is changed.
- **"The default model does not answer"** when creating an agent: start the server named in the message, or choose another default model with **Open the AI screen**.
- **"The sheet could not be prepared"**: try again, or click **Fill in the sheet myself**.
- **A source, an access or a model is listed as "Not on this machine"**: add what is missing in **Connections**, then grant it on the sheet of the agent.

> **For technical profiles:** [Apollia reference](/reference) (format of a package, native tools, command line).
