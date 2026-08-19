import { describe, it, expect } from "vitest";
import { startAgentChat, type StartAgentChatDeps } from "./startAgentChat";
import type { ChatSessionSummary } from "$lib/types";

const SESSION: ChatSessionSummary = {
  id: "sess-1",
  mode: "agent",
  agent_name: "apollia-guide",
  status: "active",
  last_message_preview: null,
  message_count: 0,
  created_at: "2026-08-18T09:00:00Z",
  closed_at: null,
  title: null,
  project_id: null,
};

interface Recorder {
  deps: StartAgentChatDeps;
  remembered: string[];
  reported: unknown[];
  navigations: number;
}

function recorder(createSession: () => Promise<ChatSessionSummary>): Recorder {
  const remembered: string[] = [];
  const reported: unknown[] = [];
  const state = { navigations: 0 };
  return {
    remembered,
    reported,
    get navigations() {
      return state.navigations;
    },
    deps: {
      createSession,
      rememberSession: (sessionId) => {
        remembered.push(sessionId);
      },
      report: (err) => {
        reported.push(err);
      },
      navigate: () => {
        state.navigations += 1;
      },
    },
  };
}

describe("startAgentChat", () => {
  it("routes to the conversation once the session exists", async () => {
    // GIVEN a backend that creates the agent-bound session
    const rec = recorder(() => Promise.resolve(SESSION));

    // WHEN the operator opens a discussion with the agent
    const opened = await startAgentChat(rec.deps);

    // THEN the session id reaches the chat route, which is then displayed
    expect(opened).toBe(true);
    expect(rec.remembered).toEqual(["sess-1"]);
    expect(rec.navigations).toBe(1);
    expect(rec.reported).toEqual([]);
  });

  it("stays put and reports when the session was never created", async () => {
    // GIVEN a backend that refuses to create the session
    const refusal = new Error("create_chat_session failed");
    const rec = recorder(() => Promise.reject(refusal));

    // WHEN the operator opens a discussion with the agent
    const opened = await startAgentChat(rec.deps);

    // THEN the failure reaches the operator, and no empty conversation is
    // presented as a working one
    expect(opened).toBe(false);
    expect(rec.reported).toEqual([refusal]);
    expect(rec.navigations).toBe(0);
    expect(rec.remembered).toEqual([]);
  });

  it("hands the raw rejection to the reporter, whatever its shape", async () => {
    // GIVEN a backend that rejects with a bare string, as Tauri commands do
    const rec = recorder(() => Promise.reject("agent not found"));

    // WHEN the operator opens a discussion with the agent
    await startAgentChat(rec.deps);

    // THEN the reporter receives the value untouched, so it can humanize it
    expect(rec.reported).toEqual(["agent not found"]);
  });
});
