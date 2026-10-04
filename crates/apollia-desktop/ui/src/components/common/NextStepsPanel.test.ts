import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { nextStepsLoadKey } from "./NextStepsPanel.svelte";

describe("NextStepsPanel - what a load depends on", () => {
  test("the developer mode changes the load key, the same facts do not", () => {
    // GIVEN one set of facts
    const facts = { tasksCompleted: 2, toolsUsed: ["web_search"] };

    // WHEN the key is computed with the developer mode off, twice, then on
    const off = nextStepsLoadKey(facts, false);
    const offAgain = nextStepsLoadKey({ ...facts }, false);
    const on = nextStepsLoadKey(facts, true);

    // THEN the same state gives the same key, and the flip gives another
    expect(offAgain).toBe(off);
    expect(on).not.toBe(off);
  });

  test("different facts change the load key in the same mode", () => {
    // GIVEN the developer mode off
    // WHEN the facts change
    // THEN the key changes, so the panel asks the store again
    expect(nextStepsLoadKey({ tasksCompleted: 1 }, false)).not.toBe(
      nextStepsLoadKey({ tasksCompleted: 2 }, false),
    );
  });
});
