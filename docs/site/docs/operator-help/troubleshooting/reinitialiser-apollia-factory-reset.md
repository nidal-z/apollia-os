# Reset Apollia (factory reset)

> For operators who want to put Apollia back in its factory state: erase agents, memory, projects, integrations and preferences. This action is irreversible - read every step before acting.

## Before you start - read this carefully

A reset deletes **all** your local data: installed agents, conversation memory, projects, MCP integrations, transcriptions, chat history, permission rules, saved API keys and preferences.

**None of this data can be recovered after confirmation.** Files stored elsewhere on your disk (your documents, your reports) are untouched.

Before continuing, ask yourself:

- Do you really want to lose everything, or do you simply want to solve one specific problem? Check [An agent is stuck](un-agent-est-bloque.md) or [The AI provider does not answer](le-fournisseur-d-ia-ne-repond-pas.md) first.
- Have you made a **backup** of what matters? See step 1 below.

## Step 1 - Back up what matters (recommended)

1. **Memory:** use the CLI `apollia-os memory export --namespace <namespace> --output <file>` to export each agent's memory. Import it back later with `apollia-os memory import <namespace> --input <file>`.
2. **Transcriptions:** open **Transcriptions** *(Builder mode)* and note the transcriptions that matter.
3. **List of your agents and connections:** take a screenshot or write down the names: you will have to reinstall them manually after the reset.

## Step 2 - Start the reset

1. In the sidebar, click **Settings**, then the **Danger Zone** section.
   ![Settings Danger Zone page, red "Factory Reset" box with a clearly isolated button](/img/operator-help/en/troubleshooting-reinitialiser-apollia-factory-reset-1.png)
2. Find the **Factory Reset** block. Read the list of data that will be deleted carefully.
3. Click the red **Factory Reset** button.

## Step 3 - Confirm explicitly

A confirmation window opens with a **3-second safety pause** during which the confirmation button stays disabled.

1. Read the list of data concerned again.
2. In the confirmation field, **type exactly** `FACTORY RESET` (in capitals, space included). Pasting from the clipboard is **blocked**: you must type the phrase on the keyboard.
3. The **Confirm the reset** button becomes active only when the word is correct **and** the 3-second pause has elapsed.
4. Click **Confirm the reset**.

## Step 4 - After the reset

1. Apollia restarts automatically. If the automatic restart fails (development environment without a packaged bundle), an orange banner invites you to relaunch the application manually.
2. On restart, the **four-step setup flow** opens automatically: **Welcome → Profile → Models → Calibration**. It is the same flow as on the very first launch.
3. At the **Models** step, you must configure the LLM again (download a local model or add a cloud backend) - the reset erased all your LLM backends. See also [Connect a remote model](../installation/connecter-un-modele-distant.md) if you would rather not go through the built-in flow.
4. Once the flow is finished, reinstall your agents, your MCP integrations and your projects as needed.
5. If you exported your memory through the CLI in step 1, import it back with `apollia-os memory import <namespace> --input <file>`.

## If something goes wrong

- **Apollia does not restart after the reset:** launch the application manually from your applications menu.
- **The setup flow does not appear:** the reset may have failed partially. Check that the data directory is absent or empty, otherwise delete it manually and relaunch. Its exact path is shown on **Settings → About**, under *Where your data lives*: see [Find your version and your data](../transversal/trouver-sa-version-et-ses-donnees.md). If the problem persists, contact support and state the exact time of the reset.
- **The Continue button stays greyed out at the Models step:** that is expected as long as no LLM is configured. Download a GGUF model from the curated list, or click **Use a cloud provider** to add an Anthropic, OpenAI or Ollama backend. The flow resumes automatically once the backend is added.
- **You regret the deletion:** restore the backups from step 1. Without a backup, the data is permanently lost.

> **Concept:** [Apollia reference](/reference) - understand where Apollia stores your data and what exactly is erased during a reset.
