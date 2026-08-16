import { describe, test, expect, vi } from "vitest";
import {
  cancelDestruction,
  commitDestruction,
  createDestructiveGate,
  isDestructiveKey,
  requestDestruction,
} from "./destructiveGate";

// DOM rendering is exercised by the Playwright layer (vitest runs in `node`).
// These tests lock the invariant the rows depend on: nothing destroys until a
// confirmation has been opened and confirmed.

describe("destructiveGate - isDestructiveKey", () => {
  test("Delete and Backspace request destruction, other keys do not", () => {
    // GIVEN the keys a focused row can receive
    const keys = ["Delete", "Backspace", "Enter", "Escape", "a", " "];

    // WHEN classifying each of them
    const destructive = keys.filter(isDestructiveKey);

    // THEN only the two erasing keys count
    expect(destructive).toEqual(["Delete", "Backspace"]);
  });
});

describe("destructiveGate - requestDestruction", () => {
  test("opens the confirmation and runs no effect", () => {
    // GIVEN a shut gate
    const gate = createDestructiveGate();
    expect(gate.open).toBe(false);

    // WHEN a path requests destruction
    requestDestruction(gate);

    // THEN the confirmation is open, which is all that happened
    expect(gate.open).toBe(true);
  });
});

describe("destructiveGate - commitDestruction", () => {
  test("refuses to run the effect while the gate is shut", async () => {
    // GIVEN a gate nobody opened and an effect that would destroy
    const gate = createDestructiveGate();
    const effect = vi.fn();

    // WHEN committing anyway, as a mis-wired path would
    const ran = await commitDestruction(gate, effect);

    // THEN nothing ran
    expect(ran).toBe(false);
    expect(effect).not.toHaveBeenCalled();
  });

  test("runs the effect once the confirmation is open, then shuts the gate", async () => {
    // GIVEN a gate opened by a request
    const gate = createDestructiveGate();
    const effect = vi.fn();
    requestDestruction(gate);

    // WHEN the user confirms
    const ran = await commitDestruction(gate, effect);

    // THEN the effect ran exactly once and the confirmation closed
    expect(ran).toBe(true);
    expect(effect).toHaveBeenCalledTimes(1);
    expect(gate.open).toBe(false);
  });

  test("a second confirm click cannot run the effect twice", async () => {
    // GIVEN an open confirmation and a slow effect
    const gate = createDestructiveGate();
    const effect = vi.fn(() => Promise.resolve());
    requestDestruction(gate);

    // WHEN the confirm button is clicked twice
    await Promise.all([
      commitDestruction(gate, effect),
      commitDestruction(gate, effect),
    ]);

    // THEN only the first click got through
    expect(effect).toHaveBeenCalledTimes(1);
  });

  test("cancelling shuts the gate and leaves the effect unreachable", async () => {
    // GIVEN an open confirmation
    const gate = createDestructiveGate();
    const effect = vi.fn();
    requestDestruction(gate);

    // WHEN the user cancels, then something tries to commit
    cancelDestruction(gate);
    const ran = await commitDestruction(gate, effect);

    // THEN nothing was destroyed
    expect(gate.open).toBe(false);
    expect(ran).toBe(false);
    expect(effect).not.toHaveBeenCalled();
  });
});
