---
title: Set up your profile on first launch
slug: /operator-help/installation/set-up-your-profile
sidebar_position: 4
---

# Set up your profile on first launch

> On the first start of Apollia (or after a reset), a configuration flow opens automatically.
> It walks you through six steps - the language, the AI, the model that searches by meaning, a short conversation that proposes your profile, a first agent, a connection that agent needs - so that Apollia is ready to use. It never asks you to pick a usage profile: the interface is the same for everyone, and the extra technical screens sit behind the developer mode in Settings, off by default.
> Total duration: 3 to 8 minutes depending on model downloads.
>
> This page covers **the initial flow only**. To edit your profile day to day from the installed application, see **[My profile](../memoire/manage-my-profile.md)**.

## Prerequisites

- Apollia is installed and started for the first time (or your profile has been reset).
- Active internet connection during the **AI** step if you download a local model.

## Flow overview

The flow is paced by a progress rail visible at the top of the window, with the count of the steps above it: **Language · AI · Search · About me · First agent · Connection**. You can go back at any step, and **Configure later** is available at the top right of the window, on every step, to postpone the whole thing.

| Step | What happens there |
|---|---|
| **Language** | A word of welcome and the choice of the interface language. |
| **AI** | The language model your agents work with: a model on your computer, with a built-in download, or a server you already use. It is the only step you cannot skip. |
| **Search** | A second, smaller model that lets your agents find a document by its meaning. It can be skipped. |
| **About me** | A short conversation with an agent, every question of which can be skipped. The agent then proposes a profile, which you read over and correct before it is saved. |
| **First agent** | Three ways to an agent that is ready to work: install a package, create one by describing your need, or start from an example. It can be skipped. |
| **Connection** | The connections your first agent needs and you do not have yet, each one added in place. It is shown only when one is missing, and it can be skipped. |

## Detailed steps

### Step 1 - Language

At launch, Apollia detects that no profile exists and opens the configuration window centered on the screen.

![Language step, the title "Welcome to Apollia", the two languages Français and English side by side, and the Continue button](/img/operator-help/installation-configurer-votre-profil-1.png)

Pick **Français** or **English**: the window switches at once, and every step after this one is read in the language you chose. You can change it at any time in **Settings → General**. Then click **Continue**.

### Step 2 - Choose the AI

This is the only step you cannot skip: without a model or a server, no agent can answer. A system information banner (RAM, OS, GPU) appears at the top to help you choose. Nothing else is asked here. The context window follows the default of the model, and voice dictation is set up later in **Settings → Speech-to-Text**.

![AI step, RAM · macOS · GPU banner, the language model section with a list of recommended models and a "Recommended" badge](/img/operator-help/installation-configurer-votre-profil-3.png)

Two cards open the step: **On this computer** and **Use a server**. Four possible paths, depending on your situation, the first three on this computer:

1. **Models already present on the machine.** If you have already placed a `.gguf` file in `~/.apollia/models/` or `~/Downloads/`, Apollia detects it automatically. Click the matching row to configure it in one click.
2. **Recommended models to download.** Apollia displays a list of models sized for your machine, with a **Recommended** badge on the most relevant one. The list stays available whether or not a GGUF was already found, and after a first model has been configured. Click the row to start the download - a progress bar with the throughput in MB/s appears, and you can cancel at any time.
3. **HuggingFace search.** The **Search on HuggingFace** button opens a built-in mini browser: type a model name, expand the available GGUF files, and click to download the one that matches your RAM (files are tagged *fits* / *might fit* / *too large*).
4. **A server.** Click the **Use a server** card when your organisation runs an AI server, or when you already use one (Ollama, llama-server, vLLM, or any server with an OpenAI-compatible interface). The form stays inside the window: type the **server address** (for example `http://localhost:11434`), add the **key** only if the server asks for one, then click **Test**. Apollia lists the models the server offers; choose one, and it becomes the default model of your agents. If the test fails, the message says what to check: the address, the key, or whether the server is started.

![Download of a recommended model in progress, with a progress bar and the throughput](/img/operator-help/installation-configurer-votre-profil-4.png)

**Continue** becomes active as soon as a model is configured (green **Configured** badge), a model of a server is chosen, or a download is started. You do not have to wait for the download: go on with the next steps while the file arrives. Its progress stays shown at the top of the window on every step, and the model is set up by itself when the file lands.

#### Buttons available in the footer

- **← Back** - returns to the Language step.
- **Continue** - disabled as long as there is no model, no server and no download in progress.

### Step 3 - Find your documents by meaning

Apollia offers a second model, smaller than the one you talk to, recommended for your machine. With it, your agents find an idea in your documents even when it is worded differently.

- **Download** fetches the recommended model; **Import from a file** takes it from a file you already have, which is the way in on a machine without a connection.
- **Skip** shows what you give up before you confirm: search by meaning in your sources and in the memory of your agents. Keyword search still works, and the model can be installed later from the AI screen. **Continue without this model** confirms.

### Step 4 - A short conversation, then your profile to review

If the model you chose at step 2 is still downloading, the window says so and the conversation starts by itself as soon as the model is ready.

The onboarding agent opens in the window. It asks a few questions, one topic at a time, and every one of them can be skipped.

![About me step with the progress pips at the top, the onboarding agent asking its first question and the user answering by describing their role](/img/operator-help/installation-configurer-votre-profil-5.png)

The questions cover the fields of a profile:

| Field | Why |
|---|---|
| Name | Agents address you by your name |
| Role | Answers and suggestions stay within your job |
| Function | The department or field you work in |
| Tools you use | Agents know the tools of your day |
| Preferences | The language, tone and answer format you prefer |
| What your agents should know | Anything else, in your own words |

Answer naturally, no precise wording required. **Nothing is written during the conversation.** When the questions are done, or as soon as you click **I'm done**, the agent proposes a profile and the conversation gives way to a form, **Your profile, to review**, filled with what it understood.

No field is required. Correct anything that is off, then choose:

- **Save** - the form is written to your default profile, and the flow moves on to the first agent.
- **Skip without saving** - nothing is written, and the flow moves on to the first agent. Your profile stays empty until you fill it from **Settings → Profile**.
- **Back** - returns to the conversation; **See the proposal again** reopens the form.

<!-- claim:onboarding-skip-direct -->
The **Skip the questions** button (in the window footer) ends the conversation **immediately, with no extra AI turn**: the form opens at once, empty or holding what the agent had already proposed, and you fill it by hand or skip it. The in-chat **I'm done** button, by contrast, lets the agent propose a profile from what you already said.

The onboarding no longer proposes permission rules. Rules are created from the approval prompts, or by hand in **Settings → Permissions**.

### Step 5 - Your first agent

Three cards, three ways to an agent that is ready to work when the window closes:

- **Install an agent** opens a package you received (a `.apo` file). Apollia shows what it contains, what it asks for and who published it; nothing is installed before you confirm.
- **Create by describing my need** opens a short conversation. Apollia drafts the sheet of the agent as you answer, you read it over and correct it, and **Save the agent** creates it.
- **Start from an example** lists the ready-made agents Apollia proposes, each with its mission and the connection it requires. Choose one: only that one is installed, after the same confirmation as any package.

Once an agent is there, the step names it and **Continue** moves on. **Skip** closes the window with no agent: you talk to Apollia, and you can add an agent at any time from the **Agents** screen.

### Step 6 - A useful connection

This step appears only when the agent you kept needs a connection you do not have yet. An agent you created by describing your need, or one whose connections are all there, goes straight past it.

Each missing connection has its line, with the reason the agent gives for needing it:

- **Add** opens, above the window, the same dialog as the **Connections** screen, directly on that connection. Once it is added, its line reads **Present**.
- A connection the selection of Apollia does not list has no **Add** button: add it later from the **Connections** screen.

**Skip** and **Continue** both close the window, whatever the lines show: nothing here is required, and a missing connection can be added at any time from the **Connections** screen.

### Skip the flow

If you prefer to configure Apollia later, click **Configure later** (at the top right of the window, available at every step). Apollia opens normally, but without a configured LLM the chat cannot answer - you will have to plug in a provider from **Settings → LLM Backends** before the first conversation.

To reopen the flow afterwards, see the [Restart the flow](#restart-the-flow) section below.

### Resume after an interruption

The flow is **resumable**: if you leave the window midway (for example by closing Apollia), Apollia picks up exactly where you left off when you come back. Progress is persisted on the backend side, not in session memory.

## Restart the flow

If you want to go through the first-launch flow again:

1. Open **Settings → Danger Zone**.
2. Click **Reset Onboarding**. This action only clears the progress and profile markers - your LLM backends, downloaded models and other data stay intact.
3. A confirmation window asks you to type `RESET`. Confirm.
4. The six-step flow restarts from the choice of the language.

For **small adjustments** (fixing your name, adding a daily tool) without going through the flow again, open **Settings → Profile** directly - see [My profile](../memoire/manage-my-profile.md). On a profile with empty fields, **Complete with an agent** starts the same short conversation for that profile; its proposal comes back into the form, where **Save** writes it and **Skip** drops it.

:::warning Reset Apollia entirely
The **Factory Reset** button (danger zone, bottom of the page) deletes **all** your data - agents, memory, downloaded models, LLM backends, integrations. This action cannot be undone. See [Reset Apollia](../troubleshooting/factory-reset.md).
:::

## Verification

Once the flow is finished, open **Settings → Profile**: the default profile holds what you saved, editable at any time. If you skipped without saving, its fields are empty.

![Settings → Profile page after onboarding, Identity (first name, role, sector), Goals, and Agent supervision sections with the HITL levels](/img/operator-help/installation-configurer-votre-profil-7.png)

## If it does not work

- **The window does not open on first launch:** check the startup logs. If the error mentions the onboarding agent, restart Apollia; it is provisioned automatically at every launch.
- **The Continue button stays disabled on the AI step:** it opens once a model is configured (green **Configured** badge), a model is chosen under **Use a server**, or a download is started. A download that failed or was cancelled closes it again.
- **A model download is stuck at 0 %:** check your internet connection, cancel with the **X** button, then start again. Expected throughput is 5 to 50 MB/s depending on your link.
- **The agent stops answering during the conversation:** close the window through **Configure later** and restart from **Settings → Danger Zone → Reset Onboarding**.
- **The form opens empty:** the agent proposed nothing, or you skipped the questions. Fill the fields by hand, or skip and come back from **Settings → Profile**.

> **Technical reference:** [Apollia reference](/reference) - full spec of the 5 steps, backend persistence, IPC commands, runtime events.
