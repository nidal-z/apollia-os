import { beforeEach, describe, expect, test, vi } from "vitest";

const ipc = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import { openModelLease, releaseModelLease } from "$lib/ipc/llmLoad";
import { buildTargetGroups } from "./chatTargets";
import {
  MAIN_PANE,
  createLeaseKeeper,
  syncConversationLease,
  type LeaseReading,
} from "./modelLease";

const QWEN = "/models/Qwen3-8B-Q4_K_M.gguf";
const MLX = "/models/Qwen3-4B-MLX-4bit";

const reading: LeaseReading = {
  loaded: true,
  defaultModel: "Qwen3-4B-MLX-4bit",
  groups: buildTargetGroups({
    agents: [{ id: "apollia:chat", name: "Apollia", enabled: true, model: null }],
    installed: [
      { id: "Qwen3-8B-Q4_K_M.gguf", category: "language", format: "gguf", path: QWEN },
      { id: "Qwen3-4B-MLX-4bit", category: "language", format: "mlx", path: MLX },
    ],
    servers: [{ server: "lab", models: [{ id: "llama-70b", route: "lab/llama-70b" }], error: null }],
    default_model: { model_id: "Qwen3-4B-MLX-4bit" },
  }),
};

const free = (model: string | null) => ({ mode: "libre" as const, agentName: null, model });

function keeper() {
  return createLeaseKeeper({ open: openModelLease, release: releaseModelLease });
}

/** The commands sent, in order, with their arguments. */
function sent(): unknown[][] {
  return ipc.mock.calls.map((call) => [call[0], call[1]]);
}

beforeEach(() => {
  ipc.mockReset();
  ipc.mockResolvedValue({});
});

describe("the lease of an open conversation", () => {
  test("opening a conversation opens its lease on the model it talks to", async () => {
    // GIVEN no conversation open
    const leases = keeper();
    // WHEN a conversation on a local model is opened
    await syncConversationLease(leases, MAIN_PANE, "conv-1", free("Qwen3-8B-Q4_K_M.gguf"), reading);
    // THEN its lease is opened on the path of that model
    expect(sent()).toEqual([["open_model_lease", { conversationId: "conv-1", modelPath: QWEN }]]);
    expect(leases.held(MAIN_PANE)).toEqual({ conversationId: "conv-1", modelPath: QWEN });
  });

  test("closing the conversation releases its lease", async () => {
    // GIVEN a conversation open on a local model
    const leases = keeper();
    await leases.hold(MAIN_PANE, "conv-1", QWEN);
    ipc.mockClear();
    // WHEN it is closed
    await leases.drop(MAIN_PANE);
    // THEN its lease is released, and the pane holds nothing
    expect(sent()).toEqual([["release_model_lease", { conversationId: "conv-1" }]]);
    expect(leases.held(MAIN_PANE)).toBeNull();
  });

  test("changing the model of the conversation releases the lease, then opens the new one", async () => {
    // GIVEN a conversation open on a local model
    const leases = keeper();
    await syncConversationLease(leases, MAIN_PANE, "conv-1", free("Qwen3-8B-Q4_K_M.gguf"), reading);
    ipc.mockClear();
    // WHEN another local model is chosen for it
    await syncConversationLease(leases, MAIN_PANE, "conv-1", free("Qwen3-4B-MLX-4bit"), reading);
    // THEN the first lease is released before the second is opened
    expect(sent()).toEqual([
      ["release_model_lease", { conversationId: "conv-1" }],
      ["open_model_lease", { conversationId: "conv-1", modelPath: MLX }],
    ]);
  });

  test("a model of a server releases the lease and opens none", async () => {
    // GIVEN a conversation open on a local model
    const leases = keeper();
    await leases.hold(MAIN_PANE, "conv-1", QWEN);
    ipc.mockClear();
    // WHEN a model of a server is chosen for it
    await syncConversationLease(leases, MAIN_PANE, "conv-1", free("lab/llama-70b"), reading);
    // THEN nothing is held on this computer any more
    expect(sent()).toEqual([["release_model_lease", { conversationId: "conv-1" }]]);
    expect(leases.held(MAIN_PANE)).toBeNull();
  });

  test("another conversation taking the pane releases the one it replaces", async () => {
    // GIVEN a conversation open on a local model
    const leases = keeper();
    await leases.hold(MAIN_PANE, "conv-1", QWEN);
    ipc.mockClear();
    // WHEN another conversation, on the default model, takes its pane
    await syncConversationLease(leases, MAIN_PANE, "conv-2", free(null), reading);
    // THEN the first is released and the second holds the default model
    expect(sent()).toEqual([
      ["release_model_lease", { conversationId: "conv-1" }],
      ["open_model_lease", { conversationId: "conv-2", modelPath: MLX }],
    ]);
  });

  test("showing the same conversation again asks nothing", async () => {
    // GIVEN a conversation open on a local model
    const leases = keeper();
    await syncConversationLease(leases, MAIN_PANE, "conv-1", free("Qwen3-8B-Q4_K_M.gguf"), reading);
    ipc.mockClear();
    // WHEN the chat is left and shown again on the same conversation
    await syncConversationLease(leases, MAIN_PANE, "conv-1", free("Qwen3-8B-Q4_K_M.gguf"), reading);
    // THEN the lease stays as it is
    expect(ipc).not.toHaveBeenCalled();
  });

  test("nothing moves while the conversation or the models are not known", async () => {
    // GIVEN a conversation open on a local model
    const leases = keeper();
    await leases.hold(MAIN_PANE, "conv-1", QWEN);
    ipc.mockClear();
    // WHEN the lease is synced before the detail, then before the list, is read
    const beforeDetail = syncConversationLease(leases, MAIN_PANE, "conv-1", null, reading);
    const beforeList = syncConversationLease(leases, MAIN_PANE, "conv-1", free(null), {
      ...reading,
      loaded: false,
    });
    // THEN no lease is released on a guess
    expect(beforeDetail).toBeNull();
    expect(beforeList).toBeNull();
    expect(ipc).not.toHaveBeenCalled();
    expect(leases.held(MAIN_PANE)).toEqual({ conversationId: "conv-1", modelPath: QWEN });
  });

  test("a lease the runtime refuses does not block the next one", async () => {
    // GIVEN a runtime that refuses the first lease
    const leases = keeper();
    ipc.mockRejectedValueOnce("no local engine on this host");
    // WHEN two conversations follow each other in the pane
    await leases.hold(MAIN_PANE, "conv-1", QWEN);
    await leases.hold(MAIN_PANE, "conv-2", MLX);
    // THEN the calls kept their order and all were made
    expect(ipc.mock.calls.map((c) => c[0])).toEqual([
      "open_model_lease",
      "release_model_lease",
      "open_model_lease",
    ]);
  });
});
