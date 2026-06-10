import { describe, test, expect } from "vitest";
import { hookErrorMessage, handlerHasPreToolUse } from "./HooksView.svelte";
import type { HookHandler } from "$lib/ipc/hooks";

// DOM rendering is exercised by the Playwright E2E layer (vitest runs in `node`).
// These tests lock the PreToolUse detection and error normalization.

function handler(events: HookHandler["events"]): HookHandler {
  return { id: 0, type: "command", events, timeout_ms: 5000, target: "guard.sh" };
}

describe("HooksView - handlerHasPreToolUse", () => {
  test("flags a handler subscribed to pre_tool_use", () => {
    // GIVEN a handler on pre_tool_use and post_tool_use
    // THEN it is tagged as a PreToolUse item
    expect(handlerHasPreToolUse(handler(["pre_tool_use", "post_tool_use"]))).toBe(true);
  });

  test("does not flag a handler without pre_tool_use", () => {
    // GIVEN a handler only on post_tool_use
    // THEN it is not tagged as a PreToolUse item
    expect(handlerHasPreToolUse(handler(["post_tool_use"]))).toBe(false);
  });
});

describe("HooksView - hookErrorMessage", () => {
  test("unwraps an Error", () => {
    expect(hookErrorMessage(new Error("ECONNREFUSED"))).toBe("ECONNREFUSED");
  });

  test("stringifies a non-Error", () => {
    expect(hookErrorMessage("runtime down")).toBe("runtime down");
  });
});
