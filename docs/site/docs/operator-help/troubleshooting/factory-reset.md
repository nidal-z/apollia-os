---
title: Reset Apollia (factory reset)
slug: /operator-help/troubleshooting/factory-reset
sidebar_position: 6
---

# Reset Apollia (factory reset)

> For operators who want to put Apollia back in its factory state: erase agents, memory, sources, integrations and preferences. This action is irreversible - read every step before acting.

## Before you start - read this carefully

A reset deletes **all** your local data: installed agents, conversation memory, sources, connections, transcriptions, chat history, permission rules, saved keys, downloaded models and preferences.

**None of this data can be recovered after confirmation.** Files stored elsewhere on your disk (your documents, your reports) are untouched.

Before continuing, ask yourself:

- Do you really want to lose everything, or do you simply want to solve one specific problem? Check [An agent is stuck](an-agent-is-stuck.md) or [The AI provider does not answer](the-ai-provider-does-not-answer.md) first.
- Have you made a **backup** of what matters? See step 1 below.

## Step 1 - Back up what matters (recommended)

1. **Memory:** use the CLI `apollia-os memory export --namespace <namespace> --output <file>` to export each agent's memory. Import it back later with `apollia-os memory import --namespace <namespace> --input <file>`.
2. **Transcriptions:** open **Transcriptions** *(visible in the sidebar with the developer mode on)* and note the transcriptions that matter.
3. **List of your agents and connections:** take a screenshot or write down the names: you will have to reinstall them manually after the reset.

## Step 2 - Start the reset

1. In the sidebar, click **Settings**. The **General** section opens: scroll down to the **Danger Zone** block.
2. Find the **Factory Reset** card, under **Last resort**. Read carefully what it says will be deleted.
3. Click the red **Factory Reset** button.

## Step 3 - Confirm explicitly

A confirmation window opens with a **3-second safety pause** during which the confirmation button stays disabled.

1. Read the list of data concerned again.
2. In the confirmation field, **type exactly** `FACTORY RESET` (in capitals, space included). Pasting from the clipboard is **blocked**: you must type the phrase on the keyboard.
3. The **Factory Reset** button of the window becomes active only when the phrase is correct **and** the 3-second pause has elapsed.
4. Click **Factory Reset**.

## Step 4 - After the reset

1. Apollia restarts automatically. If the automatic restart fails (development environment without a packaged bundle), a message invites you to restart the application manually.
2. On restart, the **seven-step setup flow** opens automatically: **Language → AI → Search, voice → About me → First agent → Connection → First task**. It is the same flow as on the very first launch.
3. At the **AI** step, you must configure the LLM again (download a local model or connect to a server) - the reset erased your models and your servers. See also [Connect a remote model](../installation/connecter-un-modele-distant.md) if you would rather not go through the built-in flow.
4. Once the flow is finished, reinstall your agents, set up your connections and add your sources again, as needed.
5. If you exported your memory through the CLI in step 1, import it back with `apollia-os memory import --namespace <namespace> --input <file>`.

## If something goes wrong

- **Apollia does not restart after the reset:** launch the application manually from your applications menu.
- **The setup flow does not appear:** the reset may have failed partially. Check that the data directory is absent or empty, otherwise delete it manually and relaunch. Its exact path is shown in **Settings → General**, in the **Application** block, on the **Data directory** line: see [Find your version and your data](../transversal/find-your-version-and-data.md). If the problem persists, contact support and state the exact time of the reset.
- **The Continue button stays greyed out at the AI step:** that is expected as long as no model is configured. Start the download of a model from the recommended list, which opens it at once, or click the **Use a server** card to add one.
- **You regret the deletion:** restore the backups from step 1. Without a backup, the data is permanently lost.

> **Concept:** [Find your version and your data](../transversal/find-your-version-and-data.md).
