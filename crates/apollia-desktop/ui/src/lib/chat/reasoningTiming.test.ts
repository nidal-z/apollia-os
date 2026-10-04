import { describe, test, expect, beforeEach } from "vitest";
import {
  createReasoningClock,
  forgetReasoningDurations,
  reasoningDurationFor,
  rememberReasoningDuration,
  REMEMBERED_MAX,
} from "./reasoningTiming";

beforeEach(() => {
  forgetReasoningDurations();
});

describe("reasoning timing - the live clock", () => {
  test("a fragment is timed from its first token to its close", () => {
    // GIVEN a clock on a controlled time
    let now = 1_000;
    const clock = createReasoningClock(() => now);

    // WHEN a fragment opens, grows, then closes four seconds later
    clock.observe([{ content: "I read", closed: false }]);
    now = 3_000;
    const open = clock.observe([{ content: "I read the workbook", closed: false }]);
    now = 5_000;
    const closed = clock.observe([{ content: "I read the workbook.", closed: true }]);

    // THEN it has no duration while open, four seconds once closed
    expect(open).toEqual([null]);
    expect(closed).toEqual([4_000]);
  });

  test("a closed fragment keeps its duration as later ones stream", () => {
    // GIVEN a first fragment closed after two seconds
    let now = 0;
    const clock = createReasoningClock(() => now);
    clock.observe([{ content: "A", closed: false }]);
    now = 2_000;
    clock.observe([{ content: "A", closed: true }]);

    // WHEN a second fragment opens and closes later
    now = 7_000;
    clock.observe([{ content: "A", closed: true }, { content: "B", closed: false }]);
    now = 8_000;
    const both = clock.observe([{ content: "A", closed: true }, { content: "B", closed: true }]);

    // THEN the first stays at two seconds and the second measures its own second
    expect(both).toEqual([2_000, 1_000]);
  });

  test("the finished turn finds the duration by the fragment's text", () => {
    // GIVEN a fragment closed by a clock
    let now = 0;
    const clock = createReasoningClock(() => now);
    clock.observe([{ content: "  I filter the Date column.  ", closed: false }]);
    now = 3_000;
    clock.observe([{ content: "  I filter the Date column.  ", closed: true }]);

    // WHEN the stored answer looks it up by its trimmed text
    // THEN the measure is there; an unknown text has none
    expect(reasoningDurationFor("I filter the Date column.")).toBe(3_000);
    expect(reasoningDurationFor("never seen")).toBeNull();
  });

  test("the remembered measures stay bounded, the oldest dropped first", () => {
    // GIVEN one more measure than the bound
    for (let i = 0; i <= REMEMBERED_MAX; i += 1) rememberReasoningDuration(`f-${i}`, i);

    // WHEN the oldest and the newest are looked up
    // THEN the oldest is gone and the newest is kept
    expect(reasoningDurationFor("f-0")).toBeNull();
    expect(reasoningDurationFor(`f-${REMEMBERED_MAX}`)).toBe(REMEMBERED_MAX);
  });
});
