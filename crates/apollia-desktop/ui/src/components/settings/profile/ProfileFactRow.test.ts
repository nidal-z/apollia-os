import { describe, test, expect, vi } from "vitest";
import { onFactRowKeydown } from "./ProfileFactRow.svelte";
import {
  cancelDestruction,
  commitDestruction,
  createDestructiveGate,
  requestDestruction,
} from "$lib/components/ui/dialog/destructiveGate";

// DOM rendering is exercised by the Playwright layer (vitest runs in `node`).
// These tests stand in for the row: `deleteProfileEntry` is the effect, and the
// three delete paths of the component are replayed against the same gate the
// component holds.

interface Row {
  deleted: string[];
  gate: ReturnType<typeof createDestructiveGate>;
}

function row(): Row {
  return { deleted: [], gate: createDestructiveGate() };
}

/** Stands for `remove()`, which is the only caller of `deleteProfileEntry`. */
function removeFact(r: Row): () => void {
  return () => {
    r.deleted.push("prefs.schedule");
  };
}

function keyEvent(key: string): { key: string; preventDefault: () => void } {
  return { key, preventDefault: vi.fn() };
}

describe("ProfileFactRow - the keystroke path", () => {
  test("a Delete keystroke on a focused row deletes nothing on its own", () => {
    // GIVEN a row that is not being edited
    const r = row();

    // WHEN the user presses Delete
    onFactRowKeydown(keyEvent("Delete"), false, {
      startEdit: vi.fn(),
      requestDelete: () => requestDestruction(r.gate),
    });

    // THEN the fact is still there and a confirmation is asking about it
    expect(r.deleted).toEqual([]);
    expect(r.gate.open).toBe(true);
  });

  test("a Backspace keystroke behaves the same", () => {
    // GIVEN a row that is not being edited
    const r = row();

    // WHEN the user presses Backspace
    onFactRowKeydown(keyEvent("Backspace"), false, {
      startEdit: vi.fn(),
      requestDelete: () => requestDestruction(r.gate),
    });

    // THEN the fact is still there and a confirmation is asking about it
    expect(r.deleted).toEqual([]);
    expect(r.gate.open).toBe(true);
  });

  test("Enter still opens the inline editor and asks nothing", () => {
    // GIVEN a row that is not being edited
    const r = row();
    const startEdit = vi.fn();

    // WHEN the user presses Enter
    onFactRowKeydown(keyEvent("Enter"), false, {
      startEdit,
      requestDelete: () => requestDestruction(r.gate),
    });

    // THEN the editor opened and no confirmation appeared
    expect(startEdit).toHaveBeenCalledTimes(1);
    expect(r.gate.open).toBe(false);
  });

  test("keystrokes are inert while the value is being edited", () => {
    // GIVEN a row in edit mode, where Delete belongs to the text field
    const r = row();
    const startEdit = vi.fn();
    const requestDelete = vi.fn();

    // WHEN the user presses Delete
    onFactRowKeydown(keyEvent("Delete"), true, { startEdit, requestDelete });

    // THEN neither the editor nor the confirmation reacted
    expect(startEdit).not.toHaveBeenCalled();
    expect(requestDelete).not.toHaveBeenCalled();
    expect(r.gate.open).toBe(false);
  });
});

describe("ProfileFactRow - the three delete paths", () => {
  test("keystroke, trash button and menu entry all stop at the confirmation", () => {
    // GIVEN three fresh rows, one per delete path
    const byKey = row();
    const byButton = row();
    const byMenu = row();

    // WHEN each path is taken
    onFactRowKeydown(keyEvent("Delete"), false, {
      startEdit: vi.fn(),
      requestDelete: () => requestDestruction(byKey.gate),
    });
    requestDestruction(byButton.gate);
    requestDestruction(byMenu.gate);

    // THEN all three opened a confirmation and none deleted
    expect([byKey.gate.open, byButton.gate.open, byMenu.gate.open]).toEqual([
      true,
      true,
      true,
    ]);
    expect([byKey.deleted, byButton.deleted, byMenu.deleted]).toEqual([
      [],
      [],
      [],
    ]);
  });

  test("confirming is what reaches deleteProfileEntry", async () => {
    // GIVEN a row whose confirmation is open
    const r = row();
    requestDestruction(r.gate);

    // WHEN the user confirms
    await commitDestruction(r.gate, removeFact(r));

    // THEN the fact is deleted, exactly once
    expect(r.deleted).toEqual(["prefs.schedule"]);
  });

  test("dismissing the confirmation keeps the fact", async () => {
    // GIVEN a row whose confirmation is open
    const r = row();
    requestDestruction(r.gate);

    // WHEN the user cancels, then a stray commit fires
    cancelDestruction(r.gate);
    await commitDestruction(r.gate, removeFact(r));

    // THEN the fact survived
    expect(r.deleted).toEqual([]);
  });
});
