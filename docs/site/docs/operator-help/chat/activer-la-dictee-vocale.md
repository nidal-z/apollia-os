# Enable voice dictation

> For any operator who wants to talk to their AI instead of typing: set up a keyboard shortcut that transcribes your voice straight into the chat field, locally.

## Prerequisites

- A Whisper model is downloaded via [Download local models](../installation/telecharger-des-modeles-locaux.md).
- Your machine's microphone works and Apollia is allowed to use it.
- A free keyboard shortcut that does not conflict with the OS or another application.

## Steps

1. In the sidebar, click **Settings**, then the **Speech-to-Text** section.
   ![Settings page, Speech-to-Text section, Whisper model status shown at the top](/img/operator-help/chat-activer-la-dictee-vocale-1.png)

2. Check that the Whisper model appears with a green **Loaded** pill. If not, go back to the Model Hub to download it.

3. Select the dictation **Language** (French, English, Spanish, etc.) in the picker. Choosing the right language improves accuracy noticeably.

4. Click the **Global hotkey** field. A window prompts you to press the key combination you want (for example **Cmd + Shift + Space**).
   ![HotkeyCapture window with the message "Press your hotkey combination" and the captured combination](/img/operator-help/chat-activer-la-dictee-vocale-2.png)

5. In the **Trigger mode** picker, choose one of the two modes:
   - **Toggle (press = start/stop)**: a first press on the shortcut starts recording, a second one stops it.
   - **Push-to-talk (hold)**: you hold the shortcut down while you speak, and transcription starts when you release it.

   For the rest of this procedure, choose **Push-to-talk (hold)**.

6. Open a chat from the sidebar.

7. Hold your shortcut down. A **full-screen dark overlay** appears with a bar audio visualiser. Speak naturally.
   On screen: the full-screen recording overlay, with the bar audio visualiser and the text {hotkey} to stop · Esc to cancel.

8. Release the shortcut. The transcription is injected into the input field through the clipboard.

   > **Note:** the transcription is inserted by simulating a paste (`Ctrl+V` / `Cmd+V`). The input field must be focused to receive the text.

9. Read it back, fix it if needed, then press **Enter** to send.

## Verification

A spoken sentence of a few seconds shows up transcribed in the input field, without any data leaving your machine.

## If it does not work

- **No transcription:** check that the microphone is properly selected in your system preferences, then read [Voice dictation transcribes nothing](../troubleshooting/la-dictee-vocale-ne-transcrit-rien.md).
- **Shortcut ignored:** another application may be capturing the same combination; pick a less common one.
- **Rough transcription:** check that the selected language matches the one you speak, and consider a larger Whisper model.
- **The text does not show up in the field:** make sure the chat input field is focused (click it) before using the shortcut.

> **Technical reference:** [Apollia reference](/reference) - supported engines, Whisper model sizes, audio formats, latency optimisations.
