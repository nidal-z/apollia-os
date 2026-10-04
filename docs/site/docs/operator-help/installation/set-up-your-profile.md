---
title: Set up your profile on first launch
slug: /operator-help/installation/set-up-your-profile
sidebar_position: 4
---

# Set up your profile on first launch

> On the first start of Apollia (or after a reset), a configuration flow opens automatically.
> It walks you through three steps - welcome, AI setup, a short conversation that proposes your profile - so that Apollia is ready to use. It never asks you to pick a usage profile: the interface is the same for everyone, and the extra technical screens sit behind the developer mode in Settings, off by default.
> Total duration: 3 to 8 minutes depending on model downloads.
>
> This page covers **the initial flow only**. To edit your profile day to day from the installed application, see **[My profile](../memoire/manage-my-profile.md)**.

## Prerequisites

- Apollia is installed and started for the first time (or your profile has been reset).
- Active internet connection during the **Models** step if you download a local model or use a cloud provider.

## Flow overview

The flow is paced by a progress rail visible at the top of the window: **Welcome · Models · About me**. You can go back at any step, and **Configure later** is available at the bottom to postpone the whole thing.

| Step | What happens there |
|---|---|
| **Welcome** | Introduction to Apollia and start of the flow. |
| **Models** | LLM setup (local GGUF with built-in download, or cloud provider) and, optionally, the Whisper voice dictation model. |
| **About me** | A short conversation with an agent, every question of which can be skipped. The agent then proposes a profile, which you read over and correct before it is saved. |

## Detailed steps

### Step 1 - Welcome

At launch, Apollia detects that no profile exists and opens the configuration window centered on the screen.

![Welcome window with the Apollia logo, subtitle "The sovereign runtime to run your AI agents locally", three cards Local-first / LLM of your choice / Autonomous agents, "Start configuration" button](/img/operator-help/installation-configurer-votre-profil-1.png)

Read the welcome banner, then click **Start configuration**.

### Step 2 - Configure the AI engine

This step calibrates Apollia to your hardware and your sovereignty preferences. A system information banner (RAM, OS, GPU) appears at the top to help you choose.

![Models step, RAM · macOS · GPU banner, LLM section with a curated list of Qwen3 models and a "Recommended" badge](/img/operator-help/installation-configurer-votre-profil-3.png)

#### LLM section (required)

Four possible paths, depending on your situation:

1. **Models already present on the machine.** If you have already placed a `.gguf` file in `~/.apollia/models/` or `~/Downloads/`, Apollia detects it automatically. Click the matching row to configure it in one click.
2. **Recommended models to download.** Apollia displays a curated list of Qwen3 models (4B, 8B, 14B, 30B-A3B) filtered by your RAM, with a **Recommended** badge on the most relevant one. The list stays available whether or not a GGUF was already found, and after a first model has been configured. Click the row to start the download - a progress bar with the throughput in MB/s appears, and you can cancel at any time.
3. **HuggingFace search.** The **Search on HuggingFace** button opens a built-in mini browser: type a model name, expand the available GGUF files, and click to download the one that matches your RAM (files are tagged *fits* / *might fit* / *too large*).
4. **Cloud provider.** The **Use a cloud provider** button closes the window and takes you to the **LLM Backends** settings page to plug in Anthropic, OpenAI or Ollama. Once a backend is added, the onboarding flow reopens automatically at this same step.

![Qwen3 download in progress with a progress bar and throughput, plus a Whisper model downloaded in parallel](/img/operator-help/installation-configurer-votre-profil-4.png)

Once the LLM setup succeeds (green **Configured** badge or a detected backend list), the **Continue** button at the bottom becomes active.

#### Voice recognition section (optional)

The **Voice recognition** toggle enables dictation. If no Whisper model is present, you can download one from the curated list:

| Model | Size | Best for |
|---|---|---|
| Whisper Tiny | 75 MB | Quick tests, constrained machines |
| Whisper Base | 142 MB | Balanced everyday use |
| Whisper Large-v3 Turbo Q5 | 547 MB | **Recommended** - high quality, 6× faster than Large-v3 |
| Whisper Large-v3 Q5 | 1.1 GB | Maximum precision, multilingual |
| Whisper Large-v3 French | 1.1 GB | Fine-tuned specifically for French |

You can skip this section and enable it later from **Settings → Speech-to-Text**.

#### Buttons available in the footer

- **← Back** - returns to the Welcome step.
- **Configure later** - jumps straight to the conversation (only if an LLM is already available).
- **Continue** - disabled as long as no LLM is usable, becomes active as soon as a cloud or local backend is configured.

### Step 3 - A short conversation, then your profile to review

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

- **Save** - the form is written to your default profile, and the window closes.
- **Skip without saving** - nothing is written, and the window closes. Your profile stays empty until you fill it from **Settings → Profile**.
- **Back** - returns to the conversation; **See the proposal again** reopens the form.

<!-- claim:onboarding-skip-direct -->
The **Skip the questions** button (in the window footer) ends the conversation **immediately, with no extra AI turn**: the form opens at once, empty or holding what the agent had already proposed, and you fill it by hand or skip it. The in-chat **I'm done** button, by contrast, lets the agent propose a profile from what you already said.

The onboarding no longer proposes permission rules. Rules are created from the approval prompts, or by hand in **Settings → Permissions**.

### Skip the flow

If you prefer to configure Apollia later, click **Configure later** (at the bottom of the window, available at every step). Apollia opens normally, but without a configured LLM the chat cannot answer - you will have to plug in a provider from **Settings → LLM Backends** before the first conversation.

To reopen the flow afterwards, see the [Restart the flow](#restart-the-flow) section below.

### Resume after an interruption

The flow is **resumable**: if you leave the window midway (for example by clicking **Use a cloud provider** to add an Anthropic backend), Apollia picks up exactly where you left off when you come back. Progress is persisted on the backend side, not in session memory.

## Restart the flow

If you want to go through the first-launch flow again:

1. Open **Settings → Danger Zone**.
2. Click **Reset Onboarding**. This action only clears the progress and profile markers - your LLM backends, downloaded models and other data stay intact.
3. A confirmation window asks you to type `RESET`. Confirm.
4. The three-step flow restarts from Welcome.

For **small adjustments** (fixing your name, adding a daily tool) without going through the flow again, open **Settings → Profile** directly - see [My profile](../memoire/manage-my-profile.md). On a profile with empty fields, **Complete with an agent** starts the same short conversation for that profile; its proposal comes back into the form, where **Save** writes it and **Skip** drops it.

:::warning Reset Apollia entirely
The **Factory Reset** button (danger zone, bottom of the page) deletes **all** your data - agents, memory, downloaded models, LLM backends, integrations. This action cannot be undone. See [Reset Apollia](../troubleshooting/factory-reset.md).
:::

## Verification

Once the flow is finished, open **Settings → Profile**: the default profile holds what you saved, editable at any time. If you skipped without saving, its fields are empty.

![Settings → Profile page after onboarding, Identity (first name, role, sector), Goals, and Agent supervision sections with the HITL levels](/img/operator-help/installation-configurer-votre-profil-7.png)

## If it does not work

- **The window does not open on first launch:** check the startup logs. If the error mentions the onboarding agent, restart Apollia; it is provisioned automatically at every launch.
- **The Continue button stays disabled on the Models step:** check that at least one LLM backend is listed. A GGUF download must be complete (green **Configured** dot), or a cloud backend must be added through **Use a cloud provider**.
- **A model download is stuck at 0 %:** check your internet connection, cancel with the **X** button, then start again. Expected throughput is 5 to 50 MB/s depending on your link.
- **The agent stops answering during the conversation:** close the window through **Configure later** and restart from **Settings → Danger Zone → Reset Onboarding**.
- **The form opens empty:** the agent proposed nothing, or you skipped the questions. Fill the fields by hand, or skip and come back from **Settings → Profile**.

> **Technical reference:** [Apollia reference](/reference) - full spec of the 4 steps, backend persistence, IPC commands, runtime events.
