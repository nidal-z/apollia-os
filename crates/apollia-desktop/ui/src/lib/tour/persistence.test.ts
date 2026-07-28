import { describe, expect, it } from "vitest";
import { parsePersistedState, runStateOf } from "./persistence";
import type { PersistedTourState } from "./persistence";

const EMPTY: PersistedTourState = { tours: {}, followVisited: false, bandDismissed: false };

describe("tour persistence - parsePersistedState", () => {
  it("round-trips a state it wrote itself", () => {
    // GIVEN a state serialised the way the store writes it
    const original: PersistedTourState = {
      tours: { landmarks: { status: "in_progress", index: 2 } },
      followVisited: true,
      bandDismissed: false,
    };
    // WHEN it is parsed back
    const parsed = parsePersistedState(JSON.stringify(original));
    // THEN nothing is lost
    expect(parsed).toEqual(original);
  });

  it("falls back to the empty state when nothing is stored", () => {
    // GIVEN no stored payload
    // WHEN parsed
    const parsed = parsePersistedState(null);
    // THEN the empty state is returned
    expect(parsed).toEqual(EMPTY);
  });

  it("falls back to the empty state on malformed JSON", () => {
    // GIVEN a truncated payload, as a crashed write would leave
    // WHEN parsed
    const parsed = parsePersistedState('{"tours":');
    // THEN it degrades to the empty state rather than throwing at module load
    expect(parsed).toEqual(EMPTY);
  });

  it("drops a tour entry whose status is not a known one", () => {
    // GIVEN a payload holding a status this version no longer knows
    const raw = JSON.stringify({
      tours: { landmarks: { status: "paused", index: 1 }, frame: { status: "done", index: 3 } },
      followVisited: false,
      bandDismissed: false,
    });
    // WHEN parsed
    const parsed = parsePersistedState(raw);
    // THEN the unknown entry is dropped and the valid one survives
    expect(parsed.tours.landmarks).toBeUndefined();
    expect(parsed.tours.frame).toEqual({ status: "done", index: 3 });
  });

  it("treats a non-boolean flag as false", () => {
    // GIVEN flags of the wrong type
    const raw = JSON.stringify({ tours: {}, followVisited: "yes", bandDismissed: 1 });
    // WHEN parsed
    const parsed = parsePersistedState(raw);
    // THEN they read as false rather than as truthy
    expect(parsed.followVisited).toBe(false);
    expect(parsed.bandDismissed).toBe(false);
  });
});

describe("tour persistence - runStateOf", () => {
  it("defaults an unknown tour to not started", () => {
    // GIVEN a state with no entry for the tour
    // WHEN its run state is read
    const run = runStateOf(EMPTY, "landmarks");
    // THEN it reads as not started at index zero
    expect(run).toEqual({ status: "not_started", index: 0 });
  });

  it("returns the stored entry when there is one", () => {
    // GIVEN a stored entry
    const state: PersistedTourState = {
      ...EMPTY,
      tours: { landmarks: { status: "skipped", index: 2 } },
    };
    // WHEN its run state is read
    const run = runStateOf(state, "landmarks");
    // THEN the stored value comes back
    expect(run).toEqual({ status: "skipped", index: 2 });
  });
});
