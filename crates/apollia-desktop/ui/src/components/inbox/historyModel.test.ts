import { describe, it, expect } from "vitest";
import {
  chatToHistoryEntry,
  mergeApprovalHistory,
  normalizeChatDecision,
  taskToHistoryEntry,
} from "./historyModel";
import type { ResolvedTaskApproval } from "$lib/ipc/inbox";
import type { ResolvedChatApproval } from "$lib/types";

function chatRow(over: Partial<ResolvedChatApproval> = {}): ResolvedChatApproval {
  return {
    session_id: "sess-1234abcd",
    message_id: "msg-1",
    tool_name: "shell",
    decision: "accept",
    resolved_at: "2026-07-01T10:00:00Z",
    reason: null,
    ...over,
  };
}

function taskRow(over: Partial<ResolvedTaskApproval> = {}): ResolvedTaskApproval {
  return {
    task_id: "task-5678efgh",
    agent_name: "digest-agent",
    approved: true,
    reason: null,
    wait_duration_ms: 1000,
    responded_at: "2026-07-01T11:00:00Z",
    ...over,
  };
}

describe("normalizeChatDecision", () => {
  it("maps the three known decisions and falls back to refuse", () => {
    // GIVEN the decision strings the runtime persists
    // WHEN each is normalized
    // THEN accept and always_accept are preserved, anything else refuses
    expect(normalizeChatDecision("accept")).toBe("accept");
    expect(normalizeChatDecision("always_accept")).toBe("always_accept");
    expect(normalizeChatDecision("refuse")).toBe("refuse");
    expect(normalizeChatDecision("something-else")).toBe("refuse");
  });
});

describe("chatToHistoryEntry", () => {
  it("carries the tool name, the session reference and the refusal reason", () => {
    // GIVEN a refused chat authorization with a reason
    const row = chatRow({ decision: "refuse", reason: "not safe", tool_name: "bash" });

    // WHEN adapted
    const entry = chatToHistoryEntry(row);

    // THEN the entry describes a chat decision
    expect(entry.origin).toBe("chat");
    expect(entry.decision).toBe("refuse");
    expect(entry.label).toBe("bash");
    expect(entry.reason).toBe("not safe");
    expect(entry.reference).toBe("sess-1234abcd");
    expect(entry.resolvedAt).toBe("2026-07-01T10:00:00Z");
  });
});

describe("taskToHistoryEntry", () => {
  it("turns the approved boolean into a decision and keeps the agent name", () => {
    // GIVEN a rejected task approval
    const row = taskRow({ approved: false, reason: "budget" });

    // WHEN adapted
    const entry = taskToHistoryEntry(row);

    // THEN the entry describes a task decision
    expect(entry.origin).toBe("task");
    expect(entry.decision).toBe("refuse");
    expect(entry.label).toBe("digest-agent");
    expect(entry.reason).toBe("budget");
    expect(entry.reference).toBe("task-5678efgh");
  });

  it("tolerates a missing responded_at", () => {
    // GIVEN a legacy row with no decision timestamp
    const row = taskRow({ responded_at: null });

    // WHEN adapted
    const entry = taskToHistoryEntry(row);

    // THEN the timestamp degrades to an empty string instead of throwing
    expect(entry.resolvedAt).toBe("");
  });
});

describe("mergeApprovalHistory", () => {
  it("interleaves both origins by decision date, most recent first", () => {
    // GIVEN two chat decisions and two task decisions with interleaved dates
    const chat = [
      chatRow({ message_id: "m1", resolved_at: "2026-07-01T09:00:00Z" }),
      chatRow({ message_id: "m2", resolved_at: "2026-07-01T12:00:00Z" }),
    ];
    const tasks = [
      taskRow({ task_id: "t1", responded_at: "2026-07-01T10:30:00Z" }),
      taskRow({ task_id: "t2", responded_at: "2026-07-01T13:00:00Z" }),
    ];

    // WHEN merged
    const merged = mergeApprovalHistory(chat, tasks);

    // THEN the list is strictly descending and mixes both origins
    expect(merged.map((e) => e.resolvedAt)).toEqual([
      "2026-07-01T13:00:00Z",
      "2026-07-01T12:00:00Z",
      "2026-07-01T10:30:00Z",
      "2026-07-01T09:00:00Z",
    ]);
    expect(merged.map((e) => e.origin)).toEqual(["task", "chat", "task", "chat"]);
  });

  it("keeps undated rows at the end instead of dropping them", () => {
    // GIVEN a task row without a timestamp alongside a dated chat row
    const merged = mergeApprovalHistory([chatRow()], [taskRow({ responded_at: null })]);

    // WHEN inspecting the merged list
    // THEN both rows survive and the undated one sorts last
    expect(merged).toHaveLength(2);
    expect(merged[1].origin).toBe("task");
  });

  it("produces unique keys across origins", () => {
    // GIVEN rows from both origins
    const merged = mergeApprovalHistory([chatRow(), chatRow({ message_id: "m9" })], [taskRow()]);

    // WHEN collecting the entry ids
    const ids = new Set(merged.map((e) => e.id));

    // THEN each entry has its own key
    expect(ids.size).toBe(3);
  });

  it("returns an empty list when both origins are empty", () => {
    // GIVEN no decision at all
    // WHEN merged
    // THEN the result is empty
    expect(mergeApprovalHistory([], [])).toEqual([]);
  });
});
