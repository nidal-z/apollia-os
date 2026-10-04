import { describe, it, expect } from "vitest";
import { tryConnectorPrompt, type TryConnectorPromptDeps } from "./tryConnectorPrompt";
import type { ChatSessionSummary } from "$lib/types";

const SESSION: ChatSessionSummary = {
  id: "sess-1",
  mode: "libre",
  agent_name: null,
  status: "active",
  last_message_preview: null,
  message_count: 0,
  created_at: "2026-09-30T09:00:00Z",
  closed_at: null,
  title: null,
};

interface Recorder {
  deps: TryConnectorPromptDeps;
  createdWith: string[][];
  sent: Array<[string, string]>;
  named: Array<[string, string]>;
  remembered: string[];
  reported: unknown[];
  navigations: number;
}

function recorder(overrides: Partial<TryConnectorPromptDeps> = {}): Recorder {
  const r: Recorder = {
    createdWith: [],
    sent: [],
    named: [],
    remembered: [],
    reported: [],
    navigations: 0,
    deps: undefined as unknown as TryConnectorPromptDeps,
  };
  r.deps = {
    loadToolNames: async () => ["mcp:notion/search", "mcp:notion/fetch"],
    createSession: async (tools) => {
      r.createdWith.push(tools);
      return SESSION;
    },
    sendMessage: async (id, prompt) => {
      r.sent.push([id, prompt]);
    },
    nameSession: (id, prompt) => {
      r.named.push([id, prompt]);
    },
    rememberSession: (id) => {
      r.remembered.push(id);
    },
    report: (err) => {
      r.reported.push(err);
    },
    navigate: () => {
      r.navigations += 1;
    },
    noToolsMessage: "no tools",
    ...overrides,
  };
  return r;
}

describe("tryConnectorPrompt", () => {
  it("opens a chat limited to the connector tools, sends the prompt and routes to it", async () => {
    // GIVEN a connector that exposes two tools
    const r = recorder();
    // WHEN the operator tries a coaching prompt
    const ok = await tryConnectorPrompt("List my databases", r.deps);
    // THEN the session is limited to those tools, the prompt is sent and the route changes
    expect(ok).toBe(true);
    expect(r.createdWith).toEqual([["mcp:notion/search", "mcp:notion/fetch"]]);
    expect(r.sent).toEqual([["sess-1", "List my databases"]]);
    expect(r.named).toEqual([["sess-1", "List my databases"]]);
    expect(r.remembered).toEqual(["sess-1"]);
    expect(r.navigations).toBe(1);
    expect(r.reported).toEqual([]);
  });

  it("reports and stays put when the connector exposes no tool", async () => {
    // GIVEN a connector with no tool
    const r = recorder({ loadToolNames: async () => [] });
    // WHEN the operator tries a prompt
    const ok = await tryConnectorPrompt("x", r.deps);
    // THEN no session is created, nothing is sent and the operator is told why
    expect(ok).toBe(false);
    expect(r.createdWith).toEqual([]);
    expect(r.sent).toEqual([]);
    expect(r.navigations).toBe(0);
    expect((r.reported[0] as Error).message).toBe("no tools");
  });

  it("reports and stays put when the session cannot be created", async () => {
    // GIVEN a backend that refuses to create the session (no model configured)
    const failure = new Error("no llm");
    const r = recorder({
      createSession: async () => {
        throw failure;
      },
    });
    // WHEN the operator tries a prompt
    const ok = await tryConnectorPrompt("x", r.deps);
    // THEN the failure is reported, no id is handed over and the route is unchanged
    expect(ok).toBe(false);
    expect(r.reported).toEqual([failure]);
    expect(r.remembered).toEqual([]);
    expect(r.navigations).toBe(0);
  });

  it("still routes to the session when only the opening send fails", async () => {
    // GIVEN a session that exists but whose first message cannot be sent
    const r = recorder({
      sendMessage: async () => {
        throw new Error("send failed");
      },
    });
    // WHEN the operator tries a prompt
    const ok = await tryConnectorPrompt("x", r.deps);
    // THEN the operator still lands on the session and can retype the message
    expect(ok).toBe(true);
    expect(r.remembered).toEqual(["sess-1"]);
    expect(r.navigations).toBe(1);
  });
});
