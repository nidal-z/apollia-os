---
title: Download local models
slug: /operator-help/installation/download-local-models
sidebar_position: 5
---

# Download local models

> For anyone who wants to run Apollia without an internet connection: download an AI model, a model that searches by meaning or a dictation model directly onto your machine.

## Prerequisites

- At least 5 GB of free disk space (up to 30 GB depending on the model).
- Active internet connection during the download only.
- Apollia is running.

## Steps

1. In the sidebar, click **AI**. The screen lists the models already on this computer, by category: **Language models**, **Embeddings**, **Dictation**.

2. Click **Discover models**. A window opens on the **Apollia selection**, the models chosen and tried by the Apollia team. Type in the search field to look among the recognized publishers on Hugging Face instead. Models are gathered by publisher, and the format filter starts on **All formats**.

   > **Note:** the same window holds the dictation models and the models that search by meaning: pick what the model is for at the top of the window.

3. Click the model you are interested in. Its card shows what it is, what it can do, and its **Download options** for this computer: each variant says whether it **Fits this computer**, **Barely fits** or is **Too large**, and the one suited to your hardware is marked **Recommended**.

4. Choose a variant and click the download button, which states its size. The download starts.

5. Follow it in the downloads panel, under the downloads icon of the top bar: it shows the progress and the speed, and **Cancel** stops it. You can close the discovery window and keep using the rest of Apollia. A download is not interrupted because it takes too long: only establishing the initial connection is subject to a timeout (30 seconds).

6. When it finishes, the file is verified, then the model appears on the **AI** screen, under **On this computer**.

7. (Optional) Open the menu at the end of the row of a language model and choose **Use by default**: conversations and agents then answer with it when no other model is chosen, and the row carries the **Default** badge. A dictation model is chosen in **Settings → Dictation**. Right after selecting a local model during onboarding, the chat step briefly shows a **Starting the engine…** status while the model registers; the conversation starts on its own as soon as it is ready.

8. The number of your models and the disk space they take are displayed at the bottom of the list. To free space, open the menu of a model and choose **Delete**.

## Models suggested during onboarding

<!-- claim:onboarding-recommends-for-hardware -->

During onboarding, the **Recommended models** list is not a fixed list. Apollia
measures your machine (its memory, and whether its graphics card has memory of
its own or shares it, as on a Mac) and starts from the models of its registry
that call tools, are meant for general use and carry an open licence (Apache
2.0 or MIT). Among those, a larger model comes before a smaller one and a
recent one before an older one of the same size. Apollia then asks HuggingFace
which files exist for the best placed, and reads the first megabytes of each
candidate to check that the bundled engine can load it and call tools with it.
The list is then ranked for your machine: the same model can come first on a
Mac and further down on a PC with a small graphics card. The other models of
the registry stay in **Discover models**, each with its licence.

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
  or use a server, and try again later.
- **No curated model fits this machine:** use a server, or import a smaller
  model from disk.

The same ranking is available from the command line, with the daemon running:
`apollia-os model recommend`.

## Verification

For a language model, open a new chat, choose your model in the selector of agents and models, and send a message: the answer arrives without an internet connection. From the discovery window, **Use in a new conversation** does the same on a variant already downloaded. For a dictation model, follow the [Enable voice dictation](../chat/enable-voice-dictation.md) page.

## If it does not work

- **Download stuck at 0 %:** check your internet connection, cancel the download from the downloads panel and start it again.
- **Not enough disk space:** delete an existing model or free up space before starting again.
- **The download is refused:** some models have restricted access. Store your **Hugging Face token** in **Settings → General**, under **Models**, then start the download again.
- **The download failed:** the downloads panel lists it under **Failed** with the reason, for example a lost connection or a file that does not match the one published.

> **Tool calling with a local model:** it works without tuning on your part, but not because raw schemas happen to suit the engine. Apollia rewrites each tool schema before sending it, because constructs a schema may legitimately contain break the engine's tool grammar. Expect a smaller model to call tools less accurately than a frontier one; that part is the model, not the plumbing.

> **Technical reference:** [Apollia reference](/reference).
