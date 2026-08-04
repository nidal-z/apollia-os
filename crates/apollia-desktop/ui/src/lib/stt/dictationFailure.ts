/**
 * Terminal outcome of a dictation that produced no text.
 *
 * The microphone button flips to "recording" as soon as `start_tour_recording`
 * returns, and only a transcription used to clear it. Every path that ends a
 * dictation without text now broadcasts `stt-dictation-failed`, so the button
 * always has an end of course instead of staying lit until the window is
 * reloaded. A stuck flag is not only cosmetic: it also re-arms the in-app
 * transcription listener, which is what made hotkey dictation insert its text
 * twice inside the Apollia window.
 *
 * Mirrors `DictationFailure` in `crates/apollia-desktop/src/stt/flow.rs`.
 */
export type DictationFailureReason =
  | "no_microphone"
  | "capture_failed"
  | "no_audio_captured"
  | "audio_unusable"
  | "nothing_audible"
  | "too_short"
  | "no_model"
  | "transcription_failed"
  | "empty_transcript"
  | "cancelled";

/** Payload of the `stt-dictation-failed` Tauri event. */
export interface DictationFailedPayload {
  reason: DictationFailureReason;
}

/** Tauri event name carrying a terminal dictation failure. */
export const DICTATION_FAILED_EVENT = "stt-dictation-failed";

const FALLBACK_REASON: DictationFailureReason = "transcription_failed";

/**
 * Narrows an untyped event payload to a known reason.
 *
 * An unrecognised reason (an older frontend against a newer runtime) degrades
 * to the generic failure rather than leaving the caller without a message.
 */
export function readFailureReason(payload: unknown): DictationFailureReason {
  const reason =
    typeof payload === "object" && payload !== null
      ? (payload as { reason?: unknown }).reason
      : payload;
  return typeof reason === "string"
    ? (reason as DictationFailureReason)
    : FALLBACK_REASON;
}

/** i18n key holding the operator-facing sentence for a failure reason. */
export function failureMessageKey(reason: DictationFailureReason): string {
  return `stt_failure.${reason}`;
}
