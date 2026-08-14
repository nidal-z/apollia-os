/**
 * Unit tests for the STT test-card phase logic.
 *
 * The regression they pin: pressing Stop used to leave `recording` true until
 * the `stt-transcribed` event arrived, so the header kept announcing listening
 * and the button stayed on the destructive Stop for the whole transcription.
 */
import { describe, it, expect } from "vitest";
import {
  flagsAfterStop,
  showsListening,
  showsTranscribing,
  testInFlight,
} from "./SttTestCard.svelte";

describe("stop-to-transcription window", () => {
  it("leaves the listening state as soon as Stop is acknowledged", () => {
    // GIVEN a recording in progress that the user stops
    // WHEN the stop call has been acknowledged and the text is still pending
    const flags = flagsAfterStop();
    // THEN the header no longer announces listening
    expect(showsListening(flags)).toBe(false);
    // AND the card announces the transcription in progress instead
    expect(showsTranscribing(flags)).toBe(true);
  });

  it("still owns the pending transcription events", () => {
    // GIVEN the window between Stop and the transcription result
    const flags = flagsAfterStop();
    // WHEN stt-transcribed or a dictation failure arrives
    // THEN the card accepts the event instead of dropping it
    expect(testInFlight(flags)).toBe(true);
  });
});

describe("phase display", () => {
  it("announces listening only while the mic captures", () => {
    // GIVEN an active capture
    const capture = { recording: true, busy: false };
    // THEN the card shows listening, not transcribing
    expect(showsListening(capture)).toBe(true);
    expect(showsTranscribing(capture)).toBe(false);
  });

  it("ignores stray dictation events when no test is running", () => {
    // GIVEN an idle card
    const idle = { recording: false, busy: false };
    // WHEN a global dictation broadcast reaches the listeners
    // THEN the card leaves it alone
    expect(testInFlight(idle)).toBe(false);
  });
});
