import { describe, test, expect } from "vitest";
import { createAnswerHold, type HoldTimers } from "./answerHold";

/** A clock the test advances by hand. */
function manualTimers(): HoldTimers & { run: () => void; pending: () => number } {
  const waiting = new Map<number, () => void>();
  let next = 0;
  return {
    set: (run) => {
      next += 1;
      waiting.set(next, run);
      return next;
    },
    clear: (handle) => void waiting.delete(handle as number),
    run: () => {
      const due = [...waiting.values()];
      waiting.clear();
      for (const fire of due) fire();
    },
    pending: () => waiting.size,
  };
}

function setup(fail = false) {
  const timers = manualTimers();
  const sent: string[] = [];
  const settled: [string, unknown][] = [];
  const hold = createAnswerHold<string>({
    delayMs: 6000,
    timers,
    send: async (id, request) => {
      sent.push(`${id}=${request}`);
      if (fail) throw new Error("refused by the runtime");
    },
    settled: (id, error) => settled.push([id, error]),
  });
  return { timers, sent, settled, hold };
}

describe("an answer given on To review is held before it is sent", () => {
  test("nothing is sent while the answer is held, and it is sent when the delay runs out", async () => {
    // GIVEN an answer held for an item
    const { timers, sent, settled, hold } = setup();
    hold.hold("consent:1", "once");
    expect(sent).toEqual([]);
    expect(hold.holds("consent:1")).toBe(true);

    // WHEN the delay runs out
    timers.run();
    await Promise.resolve();
    await Promise.resolve();

    // THEN the answer is sent once and the screen is told it settled
    expect(sent).toEqual(["consent:1=once"]);
    expect(settled).toEqual([["consent:1", null]]);
    expect(hold.holds("consent:1")).toBe(false);
  });

  test("cancel takes the answer back: it is never sent", () => {
    // GIVEN an answer held for an item
    const { timers, sent, hold } = setup();
    hold.hold("consent:1", "always");

    // WHEN it is cancelled, then the delay runs out
    const cancelled = hold.cancel("consent:1");
    timers.run();

    // THEN nothing was sent, and a second cancel has nothing to take back
    expect(cancelled).toBe(true);
    expect(sent).toEqual([]);
    expect(timers.pending()).toBe(0);
    expect(hold.cancel("consent:1")).toBe(false);
  });

  test("leaving the screen sends every held answer at once", async () => {
    // GIVEN two answers held
    const { sent, hold } = setup();
    hold.hold("consent:1", "once");
    hold.hold("question:2", "answer");

    // WHEN the screen is left
    hold.flush();
    await Promise.resolve();

    // THEN both are sent, none is lost
    expect(sent).toEqual(["consent:1=once", "question:2=answer"]);
  });

  test("an answer the runtime refuses is reported so the item comes back", async () => {
    // GIVEN a runtime that refuses the answer
    const { timers, settled, hold } = setup(true);
    hold.hold("consent:1", "once");

    // WHEN the delay runs out
    timers.run();
    await Promise.resolve();
    await Promise.resolve();

    // THEN the screen is told with the error
    expect(settled).toHaveLength(1);
    expect(settled[0][0]).toBe("consent:1");
    expect(String(settled[0][1])).toContain("refused by the runtime");
  });
});
