# Download local models

> For any operator who wants to run Apollia 100 % offline: download an AI model and a voice transcription model directly onto your machine.

## Prerequisites

- At least 5 GB of free disk space (up to 30 GB depending on the model).
- Active internet connection during the download only.
- Apollia is running and the top banner reports no error.

## Steps

1. In the sidebar, click **Settings**, then the **Model Hub** section.
   ![Model Hub page, list of available models with Name, Size, Type, Status columns](/img/operator-help/en/installation-telecharger-des-modeles-locaux-1.png)

2. Filter the list by type **GGUF** for conversational AI models.

   > **Note:** Whisper models (voice dictation) are managed from the **Speech-to-Text** section in Settings, not from the Model Hub.

3. Click the row of the model you are interested in. A panel shows the exact size, the estimated download time and the recommended hardware configuration.

4. Click **Download**. A progress bar appears next to the model.
   ![model row "Llama 3.1 8B" with a progress bar at 42 % and a Cancel button](/img/operator-help/en/installation-telecharger-des-modeles-locaux-2.png)

5. Leave the window open (the download can take 5 to 30 minutes depending on your throughput and the model size). You can keep using the rest of Apollia. The download is not interrupted because it takes too long - only establishing the initial connection is subject to a timeout (30 seconds).

6. When it finishes, the model status becomes **Available locally** with a green dot.

7. (Optional) Click **Set as default** to use this model automatically in new chats (GGUF) or for dictation (Whisper).
   ![Model Hub: the Installed models section, with the active model marked by an In use badge](/img/operator-help/en/installation-telecharger-des-modeles-locaux-1bis.png)

8. The disk space used by all your models is displayed at the bottom of the page. To free space, click **Delete** on any model already downloaded.

## Verification

For a GGUF model, open a new chat, select your local model in the backend picker, and send a message: the answer arrives without an internet connection. For a Whisper model, follow the [Enable voice dictation](../chat/activer-la-dictee-vocale.md) page.

## If it does not work

- **Download stuck at 0 %:** check your internet connection and restart the download.
- **Not enough disk space:** delete an existing model or free up space before starting again.
- **Model missing from the picker after download:** restart Apollia so it detects the new model.

> **Tool calling with a local model:** it works without tuning on your part, but not because raw schemas happen to suit the engine. Apollia rewrites each tool schema before sending it, because constructs a schema may legitimately contain break the engine's tool grammar. Expect a smaller model to call tools less accurately than a frontier one; that part is the model, not the plumbing.

> **Technical reference:** [Apollia reference](/reference) - supported GGUF formats, quantization parameters, hardware recommendations.
