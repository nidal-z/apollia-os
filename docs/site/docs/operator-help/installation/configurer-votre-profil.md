---
title: Set up your profile on first launch
slug: /operator-help/installation/set-up-your-profile
sidebar_position: 4
---

# Set up your profile on first launch

> On the first start of Apollia (or after a reset), a configuration flow opens automatically.
> It walks you through four steps - welcome, profile choice, AI setup, conversational calibration - so that Apollia is ready to use.
> Total duration: 3 to 8 minutes depending on model downloads.
>
> This page covers **the initial flow only**. To edit your profile day to day from the installed application, see **[My profile](../memoire/gerer-mon-profil.md)**.

## Prerequisites

- Apollia is installed and started for the first time (or your profile has been reset).
- Active internet connection during the **Models** step if you download a local model or use a cloud provider.

## Flow overview

The flow is paced by a progress rail visible at the top of the window: **Welcome · Profile · Models · Calibration**. You can go back at any step, and **Configure later** is available at the bottom to postpone the whole thing.

| Step | What happens there |
|---|---|
| **Welcome** | Introduction to Apollia and start of the flow. |
| **Profile** | Choice between **Operator** (simplified interface, explicit validation) and **Builder** (full observability, Python SDK). |
| **Models** | LLM setup (local GGUF with built-in download, or cloud provider) and, optionally, the Whisper voice dictation model. |
| **Calibration** | Conversation with an agent that collects 4 quick pieces of information (name, role, supervision level, data sovereignty) and suggests matching permission rules. |

## Detailed steps

### Step 1 - Welcome

At launch, Apollia detects that no profile exists and opens the configuration window centered on the screen.

![Welcome window with the Apollia logo, subtitle "The sovereign runtime to run your AI agents locally", three cards Local-first / LLM of your choice / Autonomous agents, "Start configuration" button](/img/operator-help/installation-configurer-votre-profil-1.png)

Read the welcome banner, then click **Start configuration**.

### Step 2 - Choose your profile

Two cards are offered side by side. Click the one that best matches your use - you can change it later from **Settings → Profile**.

![Profile step with two cards Operator (sparkles icon) and Builder (code icon), each with 3 bullets and a sample role](/img/operator-help/installation-configurer-votre-profil-2.png)

- **Operator**: for anyone who wants agents that carry out concrete tasks (emails, monitoring, summaries) without touching code. Explicit validation of sensitive actions, ready-to-use agents.
- **Builder**: for anyone who wants to design, debug and observe their agents. Full observability (timeline, traces, costs), Python SDK, granular permissions.

A discreet link at the bottom - **I'm both → Builder mode** - switches you to Builder mode if you hesitate (Builder mode exposes every Operator feature on top of its own).

### Step 3 - Configure the AI engine

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

- **← Back** - returns to the Profile step.
- **Configure later** - jumps straight to the conversational calibration (only if an LLM is already available).
- **Continue** - disabled as long as no LLM is usable, becomes active as soon as a cloud or local backend is configured.

### Step 4 - Conversational calibration

The onboarding agent opens in the window. It asks you up to 4 questions, one after another.

![Calibration step with 4 progress pips at the top, the onboarding agent asking the first question and the user answering by describing their role](/img/operator-help/installation-configurer-votre-profil-5.png)

The questions cover:

| Information collected | Why |
|---|---|
| Your name or alias | Agents address you by your name |
| Your role | Answers and suggestions stay within your domain |
| Desired supervision level | How often agents ask you to confirm |
| Data sovereignty preference | With or without third-party cloud services |

<!-- claim:onboarding-skip-direct -->
The first questions are the important ones; the rest is optional enrichment. The **Skip the optional questions** button (in the window footer) ends the interview **immediately, with no extra AI turn**: the flow goes straight to the permission suggestions below. The in-chat **Finish** button, by contrast, lets the agent wrap up the conversation itself.

Answer naturally, no precise wording required. After the last question, the agent derives a set of permission rules from your answers and offers them **inline in the onboarding window, right before the "Finish" button**. Each suggestion is a small card with two buttons:

- **Apply** - the rule is stored immediately in the `governance.db` database with author `onboarding-agent`.
- **Dismiss** - the rule is discarded, no permission is created. You can always add it later from **Settings → Permissions**.

> The **Finish** button only becomes active once every card has been applied or dismissed - this is how Apollia makes sure you have seen each suggestion.

The number and nature of the cards offered depend on your earlier choices:

| Your choices | Suggested rules |
|---|---|
| Sovereignty `Strictly local` | `deny http_fetch https://` and `http://` (global) - only reaches `http_fetch` |
| Sovereignty `Local preferred` | `deny http_fetch` on `api.openai.com` and `api.anthropic.com` (global) - blocks cloud LLMs by default |
| Sovereignty `Cloud allowed` | no network rule |
| Supervision `Critical only` or `Never` | `allow file_read` (global) - reduces friction on read actions |
| Supervision `Always confirm` | no allow rule - every sensitive action will raise an approval card |
| Integrations ticked (GitHub, Slack, Notion, Gmail) | `allow http_fetch` on the matching API (global) |

:::caution What sovereignty covers, and what it does not

The rules suggested here are permission rules on `http_fetch`, and that is
the only tool they reach. `Strictly local` is therefore not a network
cut-off for the machine.

Whatever the setting, these stay open: `web_search` and `web_read`, MCP
servers joined over HTTP, a remote model backend you may have configured,
and any outbound command launched by `bash_executor`, `curl` for example.
If you need a real cut-off, it belongs at the system level, firewall or
network namespace, not in this profile.

:::

![Permission rule cards suggested by the agent at the end of calibration: deny http_fetch on api.openai.com and api.anthropic.com, allow file_read, Dismiss and Apply buttons on each card](/img/operator-help/installation-configurer-votre-profil-6.png)

Once every card is handled, the window closes automatically. Apollia is ready.

### Skip the flow

If you prefer to configure Apollia later, click **Configure later** (at the bottom of the window, available at every step). Apollia opens normally, but without a configured LLM the chat cannot answer - you will have to plug in a provider from **Settings → LLM Backends** before the first conversation.

To reopen the flow afterwards, see the [Restart the flow](#restart-the-flow) section below.

### Resume after an interruption

The flow is **resumable**: if you leave the window midway (for example by clicking **Use a cloud provider** to add an Anthropic backend), Apollia picks up exactly where you left off when you come back. Progress is persisted on the backend side, not in session memory.

## Restart the flow

If you have significantly changed your profile (new role, new sovereignty policy and so on) and want your permission rules recalibrated:

1. Open **Settings → Danger Zone**.
2. Click **Reset Onboarding**. This action only clears the progress and profile markers - your LLM backends, downloaded models and other data stay intact.
3. A confirmation window asks you to type `RESET`. Confirm.
4. The four-step flow restarts from Welcome.

For **small adjustments** (fixing your name, adding a daily tool, toggling an integration) without going through the flow again, open **Settings → Profile** directly - see [My profile](../memoire/gerer-mon-profil.md).

:::warning Reset Apollia entirely
The **Factory Reset** button (danger zone, bottom of the page) deletes **all** your data - agents, memory, downloaded models, LLM backends, integrations. This action cannot be undone. See [Reset Apollia](../troubleshooting/reinitialiser-apollia-factory-reset.md).
:::

## Verification

Once the flow is finished, open a chat and ask a question about your domain. The agent addresses you by your name and adapts its answer to your context. In **Settings → Permissions**, the rules suggested during calibration are visible, and in **Settings → Profile** you find every piece of information collected, editable at any time.

![Settings → Profile page after onboarding, Identity (first name, role, sector), Goals, and Agent supervision sections with the HITL levels](/img/operator-help/installation-configurer-votre-profil-7.png)

## If it does not work

- **The window does not open on first launch:** check the startup logs. If the error mentions the onboarding agent, restart Apollia; it is provisioned automatically at every launch.
- **The Continue button stays disabled on the Models step:** check that at least one LLM backend is listed. A GGUF download must be complete (green **Configured** dot), or a cloud backend must be added through **Use a cloud provider**.
- **A model download is stuck at 0 %:** check your internet connection, cancel with the **X** button, then start again. Expected throughput is 5 to 50 MB/s depending on your link.
- **The agent stops answering during calibration:** close the window through **Configure later** and restart from **Settings → Danger Zone → Reset Onboarding**.
- **The permission rules do not apply:** see [An action was denied](../troubleshooting/une-action-est-refusee.md).

> **Technical reference:** [Apollia reference](/reference) - full spec of the 4 steps, backend persistence, IPC commands, runtime events.
