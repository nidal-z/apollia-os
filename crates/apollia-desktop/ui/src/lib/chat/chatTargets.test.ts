import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  APOLLIA_AGENT_ID,
  buildTargetGroups,
  filterTargetGroups,
  modelLabel,
  rowName,
  targetAction,
  targetLabel,
  type ChatTargetsPayload,
} from "./chatTargets";
import { chordMatches } from "$lib/shortcuts/chatShortcuts";

/** A payload as `list_chat_targets` answers it on a machine with two agents,
 * Apollia, three installed models of which one is for embeddings, and one
 * server reached and one not. */
function payload(): ChatTargetsPayload {
  return {
    agents: [
      { id: "veille", name: "Market watch", enabled: true, model: { model_id: "lab/llama-3.3-70b" } },
      { id: APOLLIA_AGENT_ID, name: "Apollia", enabled: true, model: null },
      { id: "comptable", name: "Comptable", enabled: true, model: { model_id: "Qwen3-8B-Q4_K_M.gguf" } },
      { id: "ancien", name: "Ancien", enabled: false, model: null },
    ],
    installed: [
      { id: "Qwen3-8B-Q4_K_M.gguf", category: "language", format: "gguf", parameters: "8B" },
      { id: "Qwen3-4B-MLX-4bit", category: "language", format: "mlx", parameters: null },
      { id: "nomic-embed-text.gguf", category: "embeddings", format: "gguf", parameters: null },
      { id: "ggml-base.bin", category: "dictation", format: "gguf", parameters: null },
    ],
    servers: [
      {
        server: "lab",
        models: [{ id: "llama-3.3-70b", route: "lab/llama-3.3-70b", context_window: 131072 }],
        error: null,
      },
      { server: "bureau", models: [], error: "API error (502)" },
    ],
    default_model: { backend: "local", model_source: "local", model_id: "Qwen3-8B-Q4_K_M.gguf" },
  };
}

describe("selector groups", () => {
  test("lists the agents then the models, Apollia first, disabled agents left out", () => {
    // GIVEN the targets of the machine
    // WHEN the selector builds its groups
    const groups = buildTargetGroups(payload());
    // THEN the agents come with their model, and the local models precede the served one
    expect(groups.agents.map((a) => a.id)).toEqual([APOLLIA_AGENT_ID, "comptable", "veille"]);
    expect(groups.agents[1].model).toBe("Qwen3-8B-Q4_K_M.gguf");
    expect(groups.models.map((m) => m.route)).toEqual([
      "Qwen3-8B-Q4_K_M.gguf",
      "Qwen3-4B-MLX-4bit",
      "lab/llama-3.3-70b",
    ]);
    expect(groups.models.map((m) => m.format)).toEqual(["gguf", "mlx", null]);
    expect(groups.models[2].server).toBe("lab");
  });

  test("never offers a model the chat cannot talk to", () => {
    // GIVEN installed models of every category
    // WHEN the groups are built
    const routes = buildTargetGroups(payload()).models.map((m) => m.route);
    // THEN the embeddings and the dictation models are absent
    expect(routes).not.toContain("nomic-embed-text.gguf");
    expect(routes).not.toContain("ggml-base.bin");
  });
});

describe("selector search", () => {
  test("filters both groups as it is typed, case and accents ignored", () => {
    // GIVEN the groups of the machine
    const groups = buildTargetGroups(payload());
    // WHEN a word is typed
    const qwen = filterTargetGroups(groups, "QWEN");
    const marches = filterTargetGroups(groups, "MÁRKET");
    const lab = filterTargetGroups(groups, "lab");
    const none = filterTargetGroups(groups, "mistral");
    // THEN each group keeps what matches, an agent also by its model
    expect(qwen.models.map((m) => m.route)).toEqual(["Qwen3-8B-Q4_K_M.gguf", "Qwen3-4B-MLX-4bit"]);
    expect(qwen.agents.map((a) => a.id)).toEqual(["comptable"]);
    expect(marches.agents.map((a) => a.id)).toEqual(["veille"]);
    expect(marches.models).toEqual([]);
    expect(lab.models.map((m) => m.route)).toEqual(["lab/llama-3.3-70b"]);
    expect(none).toEqual({ agents: [], models: [] });
    expect(filterTargetGroups(groups, "  ")).toBe(groups);
  });
});

describe("selector label", () => {
  test("is empty before a conversation, then names the agent and its model", () => {
    // GIVEN the groups and the default model
    const groups = buildTargetGroups(payload());
    const fallback = "Qwen3-8B-Q4_K_M.gguf";
    // WHEN the label is read for no conversation, an agent, Apollia with a chosen model
    const empty = targetLabel(groups, null, fallback, "Apollia");
    const agent = targetLabel(groups, { mode: "agent", agentName: "veille", model: null }, fallback, "Apollia");
    const free = targetLabel(
      groups,
      { mode: "libre", agentName: null, model: rowName("Qwen3-4B-MLX-4bit") },
      fallback,
      "Apollia",
    );
    const defaulted = targetLabel(groups, { mode: "libre", agentName: null, model: null }, fallback, "Apollia");
    // THEN the empty selector shows its invitation, the others their target
    expect(empty).toBeNull();
    expect(agent).toEqual({ agent: "Market watch", model: "llama-3.3-70b" });
    expect(free).toEqual({ agent: "Apollia", model: "Qwen3-4B-MLX-4bit" });
    expect(defaulted).toEqual({ agent: "Apollia", model: "Qwen3-8B-Q4_K_M.gguf" });
  });

  test("reads a row name back as the model it was written for", () => {
    // GIVEN the row name the runtime gives an index identifier
    // WHEN it is read back
    // THEN it is the model's own label, and an unknown name stays as it is
    const groups = buildTargetGroups(payload());
    expect(rowName("Qwen3-8B-Q4_K_M.gguf")).toBe("qwen3-8b-q4_k_m-gguf");
    expect(modelLabel(groups, "qwen3-8b-q4_k_m-gguf")).toBe("Qwen3-8B-Q4_K_M.gguf");
    expect(modelLabel(groups, "local")).toBe("local");
    expect(modelLabel(groups, null)).toBeNull();
  });
});

describe("what a choice does", () => {
  const groups = buildTargetGroups(payload());
  const model = groups.models[1];
  const agent = groups.agents.find((a) => a.id === "veille")!;
  const apollia = groups.agents[0];

  test("a model becomes the model of the conversation, started or not", () => {
    // GIVEN a free chat and an agent conversation, both started
    const free = { mode: "libre" as const, agentName: null, model: null };
    const withAgent = { mode: "agent" as const, agentName: "comptable", model: null };
    // WHEN a model is chosen
    // THEN it is set in place, the agent kept
    expect(targetAction(model, free, 0)).toEqual({ kind: "set_model", route: "Qwen3-4B-MLX-4bit" });
    expect(targetAction(model, free, 4)).toEqual({ kind: "set_model", route: "Qwen3-4B-MLX-4bit" });
    expect(targetAction(model, withAgent, 4)).toEqual({ kind: "set_model", route: "Qwen3-4B-MLX-4bit" });
  });

  test("a model on an empty agent conversation starts it with Apollia on that model", () => {
    // GIVEN an empty conversation with an agent
    const withAgent = { mode: "agent" as const, agentName: "comptable", model: null };
    // WHEN a model is chosen
    // THEN Apollia takes the conversation on that model, in place
    expect(targetAction(model, withAgent, 0)).toEqual({
      kind: "new_session",
      mode: "libre",
      agentName: null,
      model: "Qwen3-4B-MLX-4bit",
      replace: true,
    });
  });

  test("an agent opens a new conversation, in place of an empty one", () => {
    // GIVEN a free chat, empty then started
    const free = { mode: "libre" as const, agentName: null, model: null };
    // WHEN an agent, then Apollia, is chosen
    // THEN a conversation opens with it, replacing only an empty one
    expect(targetAction(agent, free, 0)).toEqual({
      kind: "new_session",
      mode: "agent",
      agentName: "veille",
      model: null,
      replace: true,
    });
    expect(targetAction(agent, free, 3)).toMatchObject({ kind: "new_session", replace: false });
    expect(targetAction(apollia, free, 3)).toMatchObject({ kind: "new_session", mode: "libre", agentName: null });
  });
});

describe("selector placement and shortcut", () => {
  const read = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");

  test("the conversation header and the new conversation both carry the selector", () => {
    // GIVEN the header of a conversation and the new conversation picker
    const header = read("../../components/chat/ChatConversationHeader.svelte");
    const picker = read("../../components/chat/QuickPicker.svelte");
    // WHEN their markup is read
    // THEN each renders the one selector
    expect(header).toContain("<ChatTargetSelector");
    expect(picker).toContain("<ChatTargetSelector");
  });

  test("Cmd L, Ctrl L off a Mac, asks the selector to open", () => {
    // GIVEN the chat shortcuts
    const chat = read("../../routes/Chat.svelte");
    // WHEN the binding of the selector is read
    // THEN it is the L key with the platform modifier, and it requests the selector
    expect(chat).toMatch(/id: "chat\.target_selector",\s*chord: \{ code: "KeyL", modifiers: \["mod"\] \}/);
    expect(chat).toContain("run: () => requestChatTargetSelector()");
    const isMac =
      typeof navigator !== "undefined" &&
      ((navigator as Navigator & { platform?: string }).platform ?? "").toLowerCase().includes("mac"); // NOSONAR typescript:S1874
    const press = { code: "KeyL", metaKey: isMac, ctrlKey: !isMac } as unknown as KeyboardEvent;
    const plain = { code: "KeyL" } as unknown as KeyboardEvent;
    expect(chordMatches(press, { code: "KeyL", modifiers: ["mod"] })).toBe(true);
    expect(chordMatches(plain, { code: "KeyL", modifiers: ["mod"] })).toBe(false);
  });
});
