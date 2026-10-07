import { describe, test, expect } from "vitest";
import { guideModelState } from "$lib/apolliaGuide/guideModel";
import type { ModelTarget } from "$lib/chat/chatTargets";
import type { ChatTargetsState } from "$lib/stores/chatTargets";

function model(source: "local" | "server"): ModelTarget {
  return {
    kind: "model",
    route: source === "local" ? "qwen3-8b" : "office/qwen3",
    label: "qwen3",
    source,
    format: source === "local" ? "gguf" : null,
    server: source === "server" ? "office" : null,
    parameters: null,
    path: null,
  };
}

function targets(loaded: boolean, models: ModelTarget[]): ChatTargetsState {
  return { loaded, groups: { agents: [], models }, defaultModel: null, unreachable: [] };
}

describe("guideModelState", () => {
  test("before the models were read once, the state is unknown", () => {
    // GIVEN the list of the chat's models never read
    // WHEN the state is derived
    // THEN nothing is concluded yet
    expect(guideModelState(targets(false, []))).toBe("unknown");
  });

  test("a model installed on the machine can answer the guide", () => {
    // GIVEN one language model on this machine
    // WHEN the state is derived
    // THEN the guide can answer
    expect(guideModelState(targets(true, [model("local")]))).toBe("available");
  });

  test("a model offered by a server can answer the guide", () => {
    // GIVEN no model on this machine and one on a connected server
    // WHEN the state is derived
    // THEN the guide can answer
    expect(guideModelState(targets(true, [model("server")]))).toBe("available");
  });

  test("with the list read and empty, no model can answer", () => {
    // GIVEN the list read, with no model on the machine nor on a server
    // WHEN the state is derived
    // THEN the tab shows the documentation instead
    expect(guideModelState(targets(true, []))).toBe("none");
  });
});
