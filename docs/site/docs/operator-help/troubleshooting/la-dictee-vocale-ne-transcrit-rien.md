# Voice dictation transcribes nothing

> For operators who press their dictation hotkey without seeing any text appear: get a working transcription back in a few minutes.

## Quick checks (in order of likelihood)

### 1. The keyboard shortcut is not recognized

The dictation hotkey can clash with a system shortcut (Spotlight, screen capture) or with another application.

**Solution:**
1. In the sidebar, click **Settings**, then the **Speech-to-Text** section.
2. Find the **Global hotkey** row: the current combination is shown as styled keys.
3. **Click the combination**: a full-screen capture dialog opens. Press the new combination you want, it is captured live and saved when you validate.
   ![Keyboard shortcut capture dialog, waiting for a key combination](/img/operator-help/troubleshooting-la-dictee-vocale-ne-transcrit-rien-1.png)
4. Leave the dialog with **Escape** to cancel.
5. Test the hotkey again: **a recording overlay with an audio visualizer** must appear as soon as you press it.

### 2. The transcription model is not downloaded

Apollia transcribes locally with a Whisper model. With no model loaded, pressing the hotkey produces nothing.

**Solution:**
1. Open **Settings → Speech-to-Text** and look at the engine status at the top of the page.
2. If the status reads *"Model not loaded"* or equivalent, open **Settings → Model Hub** and download at least the **Whisper Small** model (enough for French).
3. Go back to the **Speech-to-Text** page: the status must read **Model loaded**.

### 3. Your system microphone is muted or wrongly selected

If the microphone is disabled at the operating system level, Apollia hears nothing. It says so rather than staying silent: dictation ends with **"Nothing audible was captured"**.

A virtual input device is a common cause. Tools such as BlackHole, Soundflower or an aggregate device appear as microphones and can end up being the system default, in which case Apollia records a stream nobody is speaking into.

**Solution:**
1. Open **Settings → Speech-to-Text** and use the **Input device** picker to name the microphone explicitly, rather than relying on the system default.
2. Save. The choice applies to the next dictation, with no restart.
3. Press **Test** on the same page and speak: the bars must follow your voice. Flat bars mean the selected device is delivering nothing.
4. If nothing moves, unplug and plug the microphone back in (or raise the input volume).

### 4. Apollia is not allowed to use the microphone

On first use, the system asks for microphone access. If it was denied, Apollia stays silent.

**Solution:**
1. In the **Privacy** settings of your system, open the **Microphone** section.
2. Check that **Apollia** appears in the list of allowed applications and that the box is ticked.
3. If it is not, tick it, then restart Apollia.

### 5. The transcription language does not match

The Whisper model transcribes according to the configured language. A wrong language produces incoherent text, or nothing useful.

**Solution:**
1. In **Settings → Speech-to-Text**, open the **Language** picker.
2. Select **Français** (or the language you actually dictate in). Leaving it on **Auto-detect** lets the model decide, which is unreliable on short or noisy recordings.
3. Try again with a short test of a few seconds.

### 6. The microphone stays lit and the dictation never finishes

<!-- claim:stt-dictation-always-reports-an-outcome -->

A dictation always ends, whether or not it produced text. If the microphone button stays red without a result, the run reports why instead of leaving you waiting: no microphone detected, nothing audible captured, recording too short, no model loaded, or a transcription error.

**Solution:**
1. Read the message shown next to the composer, or under the **Test** card in **Settings → Speech-to-Text**.
2. Act on what it names: it points at the capture device, the model, or the recording itself.
3. If the button ever stays red with no message at all, that is a defect worth reporting: the outcome should always be stated.

## If nothing works

1. Go to **Transcriptions** *(visible in the sidebar in Builder mode)* to see whether recent attempts produced empty or incoherent content: this helps narrow the problem down.
2. Download a more accurate Whisper model (**Medium** or **Large**) from **Settings → Model Hub** if your dictations are consistently blurry.
3. Dictation settings apply on the next dictation, with no restart. Restarting is only needed if the application itself becomes unresponsive.

> **Technical reference:** [Apollia reference](/reference) - understand how Apollia captures, processes and stores your dictations locally.
