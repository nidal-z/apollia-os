import { describe, test, expect } from "vitest";
import type { ChatMessageView } from "$lib/types";
import { groupMessages, GROUP_WINDOW_MS } from "./groupMessages";

function makeMsg(
  id: string,
  role: ChatMessageView["role"],
  createdAt: string,
  overrides: Partial<ChatMessageView> = {},
): ChatMessageView {
  return {
    id,
    role,
    content: `msg ${id}`,
    tool_calls: null,
    tool_name: null,
    seq: 0,
    created_at: createdAt,
    ...overrides,
  };
}

describe("groupMessages", () => {
  test("returns empty array for empty input", () => {
    expect(groupMessages([])).toEqual([]);
  });

  test("groups two consecutive user messages within the 5-min window", () => {
    // GIVEN two user messages 2 minutes apart
    const m1 = makeMsg("a", "user", "2026-04-19T10:00:00Z");
    const m2 = makeMsg("b", "user", "2026-04-19T10:02:00Z");
    // WHEN
    const groups = groupMessages([m1, m2]);
    // THEN a single group contains both
    expect(groups).toHaveLength(1);
    expect(groups[0].messages).toEqual([m1, m2]);
    expect(groups[0].role).toBe("user");
    expect(groups[0].key).toBe("a");
  });

  test("splits into two groups when the gap exceeds GROUP_WINDOW_MS", () => {
    // GIVEN two user messages 6 minutes apart
    const m1 = makeMsg("a", "user", "2026-04-19T10:00:00Z");
    const m2 = makeMsg("b", "user", "2026-04-19T10:06:00Z");
    const gap = new Date(m2.created_at).getTime() - new Date(m1.created_at).getTime();
    expect(gap).toBeGreaterThan(GROUP_WINDOW_MS);
    // WHEN
    const groups = groupMessages([m1, m2]);
    // THEN each message lives in its own group
    expect(groups).toHaveLength(2);
  });

  test("splits when roles differ", () => {
    const m1 = makeMsg("a", "user", "2026-04-19T10:00:00Z");
    const m2 = makeMsg("b", "assistant", "2026-04-19T10:00:30Z");
    const groups = groupMessages([m1, m2]);
    expect(groups).toHaveLength(2);
    expect(groups[0].role).toBe("user");
    expect(groups[1].role).toBe("assistant");
  });

  test("never groups system messages", () => {
    const m1 = makeMsg("a", "system", "2026-04-19T10:00:00Z");
    const m2 = makeMsg("b", "system", "2026-04-19T10:00:30Z");
    const groups = groupMessages([m1, m2]);
    expect(groups).toHaveLength(2);
  });

  test("never groups tool messages", () => {
    const m1 = makeMsg("a", "tool", "2026-04-19T10:00:00Z");
    const m2 = makeMsg("b", "tool", "2026-04-19T10:00:30Z");
    const groups = groupMessages([m1, m2]);
    expect(groups).toHaveLength(2);
  });

  test("does not group assistant messages with tool_calls", () => {
    // GIVEN an assistant message carrying tool calls followed by a plain assistant reply
    const m1 = makeMsg("a", "assistant", "2026-04-19T10:00:00Z", {
      tool_calls: [
        { tool_name: "file_read", input: {}, output: null, status: "executed" },
      ],
    });
    const m2 = makeMsg("b", "assistant", "2026-04-19T10:01:00Z");
    // WHEN
    const groups = groupMessages([m1, m2]);
    // THEN the tool-carrying message stands alone (keeps its own avatar anchor)
    expect(groups).toHaveLength(2);
  });

  test("preserves order across mixed groups", () => {
    const u1 = makeMsg("u1", "user", "2026-04-19T10:00:00Z");
    const a1 = makeMsg("a1", "assistant", "2026-04-19T10:00:10Z");
    const a2 = makeMsg("a2", "assistant", "2026-04-19T10:01:00Z");
    const u2 = makeMsg("u2", "user", "2026-04-19T10:02:00Z");
    const groups = groupMessages([u1, a1, a2, u2]);
    expect(groups.map((g) => g.messages.map((m) => m.id))).toEqual([
      ["u1"],
      ["a1", "a2"],
      ["u2"],
    ]);
  });
});
