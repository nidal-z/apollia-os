import { describe, it, expect } from "vitest";
import { exportConversation } from "./exportConversation";
import type { ChatSessionDetail } from "$lib/types";

function makeSession(): ChatSessionDetail {
  return {
    id: "sess-1",
    mode: "libre",
    agent_name: null,
    system_prompt: "",
    status: "active",
    available_tools: [],
    authorized_tools: [],
    messages: [
      { id: "m1", role: "user", content: "Hello", tool_calls: null, tool_name: null, seq: 0, created_at: "2026-04-19T10:00:00Z" },
      {
        id: "m2", role: "assistant", content: "Hi", seq: 1, tool_name: null, created_at: "2026-04-19T10:00:01Z",
        tool_calls: [{ tool_name: "search", input: { q: "x" }, output: "result", status: "executed" }],
      },
    ],
    created_at: "2026-04-19T10:00:00Z",
    closed_at: null,
    llm_backend: "claude-opus",
    title: "My chat",
    project_id: null,
    plan_mode: false,
    plan_phase: "done",
  };
}

describe("exportConversation", () => {
  it("produces JSON identical to the session struct", () => {
    const s = makeSession();
    const { content, filename, mime } = exportConversation(s, "json");
    expect(mime).toBe("application/json");
    expect(filename).toMatch(/\.json$/);
    expect(JSON.parse(content)).toEqual(s);
  });

  it("produces Markdown without tool calls in the plain format", () => {
    const { content, filename } = exportConversation(makeSession(), "markdown");
    expect(filename).toMatch(/\.md$/);
    expect(content).toContain("# My chat");
    expect(content).toContain("Hello");
    expect(content).toContain("Hi");
    expect(content).not.toContain("<details>");
  });

  it("includes tool calls in markdown-with-tools", () => {
    const { content, filename } = exportConversation(makeSession(), "markdown-with-tools");
    expect(filename).toMatch(/\.tools\.md$/);
    expect(content).toContain("🔧 search");
    expect(content).toContain("\"q\": \"x\"");
    expect(content).toContain("result");
  });
});
