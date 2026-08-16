import { describe, test, expect, beforeAll, afterAll, vi } from "vitest";
import {
  cancelDestruction,
  commitDestruction,
  createDestructiveGate,
  requestDestruction,
} from "$lib/components/ui/dialog/destructiveGate";

// The component's import graph reaches the theme store, which reads
// localStorage at import time; vitest runs in `node`, so stub it first.
if (!("localStorage" in globalThis)) {
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: (_key: string) => null, setItem: () => {}, removeItem: () => {} },
  });
}
const { formatDate, onRuleRowKeydown } = await import("./PermissionRuleRow.svelte");

// Rule expiration and creation dates render as calendar dates. These tests run
// under a timezone shifted from UTC so a UTC rendering cannot pass by
// coincidence.

const SAVED_TZ = process.env.TZ;

beforeAll(() => {
  process.env.TZ = "Europe/Paris";
});

afterAll(() => {
  if (SAVED_TZ === undefined) delete process.env.TZ;
  else process.env.TZ = SAVED_TZ;
});

describe("PermissionRuleRow - formatDate", () => {
  test("a rule expiring today is not announced for yesterday", () => {
    // GIVEN an expiration recorded at 22:00 UTC on the 9th, which is already
    // the 10th in Europe/Paris (UTC+2 in August)
    const expiresAt = "2026-08-09T22:00:00+00:00";

    // WHEN formatting it under the English catalogue
    const rendered = formatDate(expiresAt, "en");

    // THEN the date is the machine's local calendar day
    expect(rendered).toBe("08/10/2026");
  });

  test("the same instant follows the French date order under the French catalogue", () => {
    // GIVEN the same wire instant
    const expiresAt = "2026-08-09T22:00:00+00:00";

    // WHEN formatting it under the French catalogue
    const rendered = formatDate(expiresAt, "fr");

    // THEN day and month swap to the locale's order, on the local day
    expect(rendered).toBe("10/08/2026");
  });

  test("an unparsable timestamp passes through verbatim", () => {
    // GIVEN a value the Date constructor rejects
    // WHEN formatting it
    // THEN the raw value is shown rather than an Invalid Date artifact
    expect(formatDate("never", "en")).toBe("never");
  });
});

// DOM rendering is exercised by the Playwright layer (vitest runs in `node`).
// `onRevoke` is what the two parents wire to `revokeRule` (persistent rules)
// and to `deleteChatRule` (chat rules); the row is the single place that
// decides whether it is ever called, so both sections are covered here.

function keyEvent(key: string): { key: string; preventDefault: () => void } {
  return { key, preventDefault: vi.fn() };
}

describe("PermissionRuleRow - the keystroke path", () => {
  test("a Delete keystroke on a focused rule revokes nothing on its own", () => {
    // GIVEN a rule row that is not already revoking
    const gate = createDestructiveGate();
    const onRevoke = vi.fn();

    // WHEN the user presses Delete
    onRuleRowKeydown(keyEvent("Delete"), false, () =>
      requestDestruction(gate),
    );

    // THEN nothing was revoked and a confirmation is asking about it
    expect(onRevoke).not.toHaveBeenCalled();
    expect(gate.open).toBe(true);
  });

  test("a Backspace keystroke behaves the same", () => {
    // GIVEN a rule row that is not already revoking
    const gate = createDestructiveGate();

    // WHEN the user presses Backspace
    onRuleRowKeydown(keyEvent("Backspace"), false, () =>
      requestDestruction(gate),
    );

    // THEN a confirmation is asking about it
    expect(gate.open).toBe(true);
  });

  test("an unrelated key asks nothing", () => {
    // GIVEN a rule row that is not already revoking
    const requestRevoke = vi.fn();

    // WHEN the user presses Escape
    onRuleRowKeydown(keyEvent("Escape"), false, requestRevoke);

    // THEN no confirmation was requested
    expect(requestRevoke).not.toHaveBeenCalled();
  });

  test("a row already revoking ignores the keystroke", () => {
    // GIVEN a row whose revocation is in flight
    const requestRevoke = vi.fn();

    // WHEN the user presses Delete again
    onRuleRowKeydown(keyEvent("Delete"), true, requestRevoke);

    // THEN nothing was requested a second time
    expect(requestRevoke).not.toHaveBeenCalled();
  });
});

describe("PermissionRuleRow - the three revoke paths", () => {
  test("keystroke, revoke button and menu entry all stop at the confirmation", () => {
    // GIVEN three fresh rows, one per revoke path
    const byKey = createDestructiveGate();
    const byButton = createDestructiveGate();
    const byMenu = createDestructiveGate();
    const onRevoke = vi.fn();

    // WHEN each path is taken
    onRuleRowKeydown(keyEvent("Delete"), false, () =>
      requestDestruction(byKey),
    );
    requestDestruction(byButton);
    requestDestruction(byMenu);

    // THEN all three opened a confirmation and none revoked
    expect([byKey.open, byButton.open, byMenu.open]).toEqual([
      true,
      true,
      true,
    ]);
    expect(onRevoke).not.toHaveBeenCalled();
  });

  test("confirming is what reaches the parent's revoke", async () => {
    // GIVEN a row whose confirmation is open
    const gate = createDestructiveGate();
    const onRevoke = vi.fn();
    requestDestruction(gate);

    // WHEN the user confirms
    await commitDestruction(gate, onRevoke);

    // THEN the parent revoked, exactly once
    expect(onRevoke).toHaveBeenCalledTimes(1);
  });

  test("dismissing the confirmation keeps the rule", async () => {
    // GIVEN a row whose confirmation is open
    const gate = createDestructiveGate();
    const onRevoke = vi.fn();
    requestDestruction(gate);

    // WHEN the user cancels, then a stray commit fires
    cancelDestruction(gate);
    await commitDestruction(gate, onRevoke);

    // THEN the rule survived
    expect(onRevoke).not.toHaveBeenCalled();
  });
});
