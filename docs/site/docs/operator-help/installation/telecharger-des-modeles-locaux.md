---
title: Download local models
slug: /operator-help/installation/download-local-models
sidebar_position: 5
---

# Download local models

> For any operator who wants to run Apollia 100 % offline: download an AI model and a voice transcription model directly onto your machine.

## Prerequisites

- At least 5 GB of free disk space (up to 30 GB depending on the model).
- Active internet connection during the download only.
- Apollia is running and the top banner reports no error.

## Steps

1. In the sidebar, click **Settings**, then the **Model Hub** section.
   ![Model Hub page, list of available models with Name, Size, Type, Status columns](/img/operator-help/installation-telecharger-des-modeles-locaux-1.png)

2. Filter the list by type **GGUF** for conversational AI models.

   > **Note:** Whisper models (voice dictation) are managed from the **Speech-to-Text** section in Settings, not from the Model Hub.

3. Click the row of the model you are interested in. A panel shows the exact size, the estimated download time and the recommended hardware configuration.

4. Click **Download**. A progress bar appears next to the model.
   ![model row "Llama 3.1 8B" with a progress bar at 42 % and a Cancel button](/img/operator-help/installation-telecharger-des-modeles-locaux-2.png)

5. Leave the window open (the download can take 5 to 30 minutes depending on your throughput and the model size). You can keep using the rest of Apollia. The download is not interrupted because it takes too long - only establishing the initial connection is subject to a timeout (30 seconds).

6. When it finishes, the model status becomes **Available locally** with a green dot.

7. (Optional) Click **Set as default** to use this model automatically in new chats (GGUF) or for dictation (Whisper). Right after selecting a local model during onboarding, the chat step briefly shows a **Starting the engine** status while the model registers; the conversation starts on its own as soon as it is ready.
   ![Model Hub: the Installed models section, with the active model marked by an In use badge](/img/operator-help/installation-telecharger-des-modeles-locaux-1bis.png)

8. The disk space used by all your models is displayed at the bottom of the page. To free space, click **Delete** on any model already downloaded.

## Models suggested during onboarding

<!-- claim:onboarding-recommends-for-hardware -->

During onboarding, the **Recommended models** list is not a fixed list. Apollia
measures your machine (its memory, and whether its graphics card has memory of
its own or shares it, as on a Mac), asks HuggingFace which files exist for the
model generations it knows, and reads the first megabytes of each candidate to
check that the bundled engine can load it and call tools with it. The list is
then ranked for your machine: the same model can come first on a Mac and
further down on a PC with a small graphics card.

Above the list, **Context window** sets how much of a conversation the model
holds at once (32k tokens by default). A larger window needs more memory, so the
list is recomputed when you change it, and the model you pick is set up with
that window. You can change it later in **Settings**, on the model's backend,
under **Context window**; the engine, the automatic compaction and the **Ctx**
gauge all follow it. A model trained on a shorter window runs at its own: past
it the model reads positions it was never trained on, and its answers degrade.
The row then says so.

Each row says why it is there: the memory it needs, split between the model
itself, its conversation memory and the engine, how it is compressed (for
example `Q4_K_M`), and which memory it runs from. On a PC with a graphics card,
that is either the card's own video memory or, for a model too large for it, a
split between video memory and system RAM. The line under the list names both
memories separately.

- **Runs entirely on your GPU:** the fastest case, measured against the card's
  video memory only.
- **Tight fit** or **with little room left:** the model fits, with little room
  left for other applications.
- **Too large for your GPU alone:** part of the model sits in system RAM, which
  is much slower. A smaller model may answer faster.
- **N of M layers on your GPU, the rest on the CPU:** the same split, counted in
  layers.
- A model that would generate fewer than about four tokens per second on your
  machine is not offered, unless nothing faster fits.
- **No tools:** the model can chat but cannot run agents.
- **Could not reach HuggingFace:** the list is fetched live, so without a
  connection there is nothing to suggest. Import a model file you already have,
  or use a cloud provider, and try again later.
- **No curated model fits this machine:** use a cloud provider, or import a
  smaller model from disk.

The same ranking is available from the command line, with the daemon running:
`apollia-os model recommend`.

## Verification

For a GGUF model, open a new chat, select your local model in the backend picker, and send a message: the answer arrives without an internet connection. For a Whisper model, follow the [Enable voice dictation](../chat/enable-voice-dictation.md) page.

## If it does not work

- **Download stuck at 0 %:** check your internet connection and restart the download.
- **Not enough disk space:** delete an existing model or free up space before starting again.
- **Model missing from the picker after download:** restart Apollia so it detects the new model.

> **Tool calling with a local model:** it works without tuning on your part, but not because raw schemas happen to suit the engine. Apollia rewrites each tool schema before sending it, because constructs a schema may legitimately contain break the engine's tool grammar. Expect a smaller model to call tools less accurately than a frontier one; that part is the model, not the plumbing.

> **Technical reference:** [Apollia reference](/reference) - supported GGUF formats, quantization parameters, hardware recommendations.
