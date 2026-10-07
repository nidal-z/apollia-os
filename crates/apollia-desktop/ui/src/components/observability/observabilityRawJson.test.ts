import { describe, test, expect } from "vitest";
import {
  isActiveHook,
  normalizeActiveHooks,
} from "$lib/ipc/observability";

// `get_active_hooks` relays `serde_json::Value` untouched, so the typed
// boundary is this normalizer. It is the only thing standing between a shape
// change on the runtime side and a broken panel.

describe("normalizeActiveHooks", () => {
  const validHook = {
    id: 0,
    type: "command",
    events: ["pre_tool_use"],
    timeout_ms: 5000,
    target: "/usr/bin/notify --quiet",
  };

  test("a well-formed handler summary is kept", () => {
    // GIVEN the array the hooks route documents
    // WHEN normalizing it
    const hooks = normalizeActiveHooks([validHook]);

    // THEN the entry survives with all five fields
    expect(hooks).toEqual([validHook]);
  });

  test("an empty registry is a clean state, not an error", () => {
    // GIVEN no declared hook
    // WHEN normalizing it
    // THEN the result is an empty list
    expect(normalizeActiveHooks([])).toEqual([]);
  });

  test("entries missing a documented field are dropped", () => {
    // GIVEN one valid entry and two malformed ones
    const raw = [validHook, { id: 1, type: "http" }, { ...validHook, events: [1] }];

    // WHEN normalizing it
    const hooks = normalizeActiveHooks(raw);

    // THEN only the entry matching the documented shape is rendered
    expect(hooks).toHaveLength(1);
    expect(hooks[0].id).toBe(0);
  });

  test("a non-array payload yields an empty list", () => {
    // GIVEN a payload that is not an array
    // WHEN normalizing it
    // THEN the panel falls back to the empty state
    expect(normalizeActiveHooks({ hooks: [] })).toEqual([]);
    expect(isActiveHook(undefined)).toBe(false);
  });
});
