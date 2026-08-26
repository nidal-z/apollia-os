/**
 * The microphone button of the composer.
 *
 * The Rust pipeline broadcasts `stt-transcribed` for every transcription. Only
 * the in-app mic button feeds the composer, gated on `recording` (set solely by
 * `toggle`). The global hotkey delivers its text through the OS-level clipboard
 * paste (see `SttFlow::dispatch_result`); appending it again would double-insert
 * the text when the Apollia window is focused.
 *
 * That guard is only as good as the flag it reads. `recording` used to be
 * cleared by `stt-transcribed` alone, so any dictation that ended without text
 * (silence, too short, engine error) left it stuck true, and every later hotkey
 * dictation was then inserted twice inside the window. The
 * `stt-dictation-failed` listener is what closes that hole.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { startTourRecording, stopTourRecording } from "$lib/ipc/stt";
import {
  DICTATION_FAILED_EVENT,
  failureMessageKey,
  readFailureReason,
} from "$lib/stt/dictationFailure";

export interface ComposerDictation {
  /** True between the press that starts the capture and the text that ends it. */
  readonly recording: boolean;
  /** True while a start, a stop or an inference is in flight. */
  readonly busy: boolean;
  /** Why the last dictation produced nothing, or `null`. */
  readonly error: string | null;
  toggle(): Promise<void>;
  /**
   * Subscribe to the two pipeline events. Call from `onMount` and return the
   * cleanup it hands back.
   */
  start(onTranscribed: (text: string) => void): () => void;
}

export function createComposerDictation(): ComposerDictation {
  let recording = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  return {
    get recording() {
      return recording;
    },
    get busy() {
      return busy;
    },
    get error() {
      return error;
    },

    async toggle(): Promise<void> {
      if (busy) return;
      busy = true;
      error = null;
      try {
        if (recording) {
          await stopTourRecording();
          // The transcription event flips recording=false once it arrives, and
          // `stt-dictation-failed` does the same when there is no text to
          // deliver. busy stays true until one of them lands, so the user
          // cannot double-click during the inference.
        } else {
          await startTourRecording();
          recording = true;
          busy = false;
        }
      } catch (err) {
        // STT engine unavailable, no model configured, etc.
        recording = false;
        busy = false;
        error = err instanceof Error ? err.message : String(err);
      }
    },

    start(onTranscribed: (text: string) => void): () => void {
      let cancelled = false;
      const unlisteners: UnlistenFn[] = [];
      const keep = (unlisten: UnlistenFn) => {
        if (cancelled) unlisten();
        else unlisteners.push(unlisten);
      };

      void listen<{ text?: string } | string>("stt-transcribed", (event) => {
        if (!recording) return;
        const text =
          typeof event.payload === "string"
            ? event.payload
            : event.payload?.text ?? "";
        if (!text) return;
        recording = false;
        busy = false;
        error = null;
        onTranscribed(text);
      }).then(keep);

      void listen(DICTATION_FAILED_EVENT, (event) => {
        if (!recording && !busy) return;
        recording = false;
        busy = false;
        error = get(t)(failureMessageKey(readFailureReason(event.payload)));
      }).then(keep);

      return () => {
        cancelled = true;
        for (const unlisten of unlisteners) unlisten();
        unlisteners.length = 0;
      };
    },
  };
}
