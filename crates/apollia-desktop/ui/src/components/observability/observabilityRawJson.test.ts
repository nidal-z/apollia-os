import { describe, test, expect } from "vitest";
import {
  isActiveHook,
  normalizeActiveHooks,
  normalizeAuditStats,
} from "$lib/ipc/observability";

// `get_audit_stats` and `get_active_hooks` relay `serde_json::Value` untouched,
// so the typed boundary is these two normalizers. They are the only thing
// standing between a shape change on the runtime side and a broken panel.

describe("normalizeAuditStats", () => {
  test("the three documented counters are read as-is", () => {
    // GIVEN the body the audit stats route documents
    const raw = { total_events: 128, unique_tools: 7, unique_agents: 3 };

    // WHEN normalizing it
    const stats = normalizeAuditStats(raw);

    // THEN nothing is altered and nothing is added
    expect(stats).toEqual({ total_events: 128, unique_tools: 7, unique_agents: 3 });
  });

  test("a missing or non-numeric counter reads as zero", () => {
    // GIVEN a partial body
    const raw = { total_events: "many", unique_tools: 2 };

    // WHEN normalizing it
    const stats = normalizeAuditStats(raw);

    // THEN the panel shows 0 rather than NaN
    expect(stats).toEqual({ total_events: 0, unique_tools: 2, unique_agents: 0 });
  });

  test("a non-object payload degrades to zeroes", () => {
    // GIVEN a payload that is not an object
    // WHEN normalizing it
    // THEN the strip still renders
    expect(normalizeAuditStats(null)).toEqual({
      total_events: 0,
      unique_tools: 0,
      unique_agents: 0,
    });
  });
});

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
