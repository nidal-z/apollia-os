import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { get } from "svelte/store";
import { applyTargetChoice, startRequest } from "./applyTargetChoice";
import type { AgentTarget, ModelTarget } from "./chatTargets";
import { chatSessions } from "$lib/stores/sse";
import type { ChatSessionSummary } from "$lib/types";

const mockedInvoke = vi.mocked(invoke);

const model: ModelTarget = {
  kind: "model",
  route: "Qwen3-8B-Q4_K_M.gguf",
  label: "Qwen3-8B-Q4_K_M.gguf",
  source: "local",
  format: "gguf",
  server: null,
  parameters: "8B",
};
const agent: AgentTarget = { kind: "agent", id: "veille", name: "Veille", model: null, builtin: false };
const free = { mode: "libre" as const, agentName: null, model: null };

function summary(id: string): ChatSessionSummary {
  return { id } as ChatSessionSummary;
}

beforeEach(() => {
  mockedInvoke.mockReset();
  chatSessions.set([summary("s-empty"), summary("s-other")]);
});

describe("applying a choice of the selector", () => {
  test("a model chosen on a started conversation updates its model in place", async () => {
    // GIVEN a started free chat
    mockedInvoke.mockResolvedValue(undefined);
    // WHEN a model is chosen on it
    const outcome = await applyTargetChoice(model, "s-1", free, 4);
    // THEN the session's model is updated and the same conversation stays
    expect(mockedInvoke).toHaveBeenCalledWith("update_chat_session", {
      sessionId: "s-1",
      update: { llm_backend: "Qwen3-8B-Q4_K_M.gguf" },
    });
    expect(outcome.openedSessionId).toBeNull();
  });

  test("an agent chosen on an empty conversation takes its place", async () => {
    // GIVEN an empty free chat
    mockedInvoke.mockImplementation(async (cmd) =>
      cmd === "create_chat_session" ? summary("s-agent") : undefined,
    );
    // WHEN an agent is chosen on it
    const outcome = await applyTargetChoice(agent, "s-empty", free, 0);
    // THEN a conversation with the agent opens and the empty one is removed
    expect(mockedInvoke).toHaveBeenCalledWith("create_chat_session", {
      request: { mode: "agent", agent_name: "veille" },
    });
    expect(mockedInvoke).toHaveBeenCalledWith("delete_chat_session", { sessionId: "s-empty" });
    expect(outcome.openedSessionId).toBe("s-agent");
    expect(get(chatSessions).map((s) => s.id)).toEqual(["s-other"]);
  });

  test("an agent chosen on a started conversation opens a new one beside it", async () => {
    // GIVEN a started free chat
    mockedInvoke.mockImplementation(async (cmd) =>
      cmd === "create_chat_session" ? summary("s-agent") : undefined,
    );
    // WHEN an agent is chosen on it
    const outcome = await applyTargetChoice(agent, "s-other", free, 2);
    // THEN the new conversation opens and the started one is kept
    expect(mockedInvoke).not.toHaveBeenCalledWith("delete_chat_session", expect.anything());
    expect(outcome.openedSessionId).toBe("s-agent");
  });
});

describe("starting a conversation on a chosen target", () => {
  test("a model starts Apollia on it, an agent starts the agent", () => {
    // GIVEN each kind of choice, and none
    // WHEN the request that starts the conversation is built
    // THEN it carries the model, or the agent, or nothing
    expect(startRequest(model)).toEqual({ mode: "libre", model: "Qwen3-8B-Q4_K_M.gguf" });
    expect(startRequest(agent)).toEqual({ mode: "agent", agent_name: "veille" });
    expect(startRequest({ ...agent, id: "apollia:chat", builtin: true })).toEqual({ mode: "libre" });
    expect(startRequest(null)).toEqual({ mode: "libre" });
  });
});
