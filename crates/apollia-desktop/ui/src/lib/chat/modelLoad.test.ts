import { beforeEach, describe, expect, test, vi } from "vitest";

const ipc = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import { unloadLoadedModel } from "$lib/ipc/llm";
import { loadModel, reloadLastLoadedModel, setModelPin } from "$lib/ipc/llmLoad";
import {
  buildTargetGroups,
  conversationModel,
  type ChatTargetsPayload,
  type ModelTarget,
} from "./chatTargets";
import {
  attemptLoad,
  canEject,
  clampContext,
  clampOffload,
  contextBounds,
  ejectModel,
  emptyForm,
  formFromSettings,
  isReloadShortcut,
  loadChosen,
  loadRequest,
  loadedInstance,
  needsLoadWindow,
  refusalOf,
  reloadLast,
  showReloadLast,
  shownOffload,
} from "./modelLoad";
import type {
  LastLoadedModel,
  LoadEstimate,
  LoadedModel,
  LoadedModelsResponse,
  ModelLoadOutcome,
} from "$lib/types";

const QWEN_PATH = "/models/Qwen3-8B-Q4_K_M.gguf";

function payload(): ChatTargetsPayload {
  return {
    agents: [
      { id: "apollia:chat", name: "Apollia", enabled: true, model: null },
      { id: "comptable", name: "Comptable", enabled: true, model: { model_id: "Qwen3-8B-Q4_K_M.gguf" } },
    ],
    installed: [
      { id: "Qwen3-8B-Q4_K_M.gguf", category: "language", format: "gguf", path: QWEN_PATH },
      { id: "Qwen3-4B-MLX-4bit", category: "language", format: "mlx", path: "/models/Qwen3-4B-MLX-4bit" },
    ],
    servers: [{ server: "lab", models: [{ id: "llama-70b", route: "lab/llama-70b" }], error: null }],
    default_model: { model_id: "Qwen3-4B-MLX-4bit" },
  };
}

const groups = buildTargetGroups(payload());
const qwen = groups.models[0];
const served = groups.models[2];

function resident(over: Partial<LoadedModel> = {}): LoadedModel {
  return {
    engine: "llama-server",
    kind: "chat",
    model: "Qwen3-8B-Q4_K_M",
    modelPath: QWEN_PATH,
    port: 41001,
    nCtx: 8192,
    nGpuLayers: 36,
    loadedAt: "2026-10-06T09:00:00+00:00",
    lastUsedAt: "2026-10-06T09:05:00+00:00",
    activeRequests: 0,
    modelId: "Qwen3-8B-Q4_K_M.gguf",
    ...over,
  };
}

function loaded(models: LoadedModel[], engineAvailable = true): LoadedModelsResponse {
  return { engineAvailable, maxLoaded: null, models };
}

function estimate(over: Partial<LoadEstimate> = {}): LoadEstimate {
  return {
    modelId: "Qwen3-8B-Q4_K_M.gguf",
    engine: "llama-server",
    modelPath: QWEN_PATH,
    configurable: true,
    nCtx: 8192,
    nGpuLayers: 36,
    totalLayers: 36,
    maxCtx: 40960,
    quantization: "Q4_K_M",
    memory: { gpuGb: 5.1, totalGb: 6.3, capacityGb: 16, fit: "fits", kvCacheMeasured: true },
    guardrail: { level: "balanced", verdict: "allowed", budgetGb: 16 },
    ...over,
  };
}

const refused = (): LoadEstimate =>
  estimate({
    memory: { gpuGb: 19.6, totalGb: 21.4, capacityGb: 16, fit: "too_large", kvCacheMeasured: true },
    guardrail: { level: "balanced", verdict: "refused", budgetGb: 16 },
  });

function outcome(): ModelLoadOutcome {
  return { modelId: "Qwen3-8B-Q4_K_M.gguf", loaded: resident() };
}

const calls = { load: loadModel, setPin: setModelPin };

beforeEach(() => {
  ipc.mockReset();
});

describe("when the loading window opens", () => {
  test("a model of this computer that is not in memory goes through the window", () => {
    // GIVEN the models of the machine, and nothing in memory
    const nothing = loaded([]);
    // WHEN a local model, a model of a server and an agent are chosen
    const answers = [
      needsLoadWindow(qwen, nothing),
      needsLoadWindow(served, nothing),
      needsLoadWindow(groups.agents[1], nothing),
    ];
    // THEN only the local model opens the window
    expect(answers).toEqual([true, false, false]);
  });

  test("a model already in memory, or a host that runs no model itself, opens nothing", () => {
    // GIVEN the model resident, then a host with no local engine
    // WHEN it is chosen in each case
    const whenResident = needsLoadWindow(qwen, loaded([resident()]));
    const withoutEngine = needsLoadWindow(qwen, loaded([], false));
    // THEN the choice goes straight to the conversation
    expect(whenResident).toBe(false);
    expect(withoutEngine).toBe(false);
  });
});

describe("what the window asks", () => {
  test("an untouched window asks for the model alone", () => {
    // GIVEN a window nobody touched
    // WHEN its request is built
    const request = loadRequest("Qwen3-8B-Q4_K_M.gguf", emptyForm(), true);
    // THEN the model's own settings apply
    expect(request).toEqual({ modelId: "Qwen3-8B-Q4_K_M.gguf" });
  });

  test("the settings travel by the keys the runtime reads, and remembering is asked", () => {
    // GIVEN a window with a context, an offload, the advanced settings and "remember"
    const form = {
      ...emptyForm(),
      nCtx: 16384,
      nGpuLayers: 12,
      batch: 512,
      threads: 8,
      seed: 42,
      kvOffload: false,
      remember: true,
    };
    // WHEN its request is built
    const request = loadRequest("Qwen3-8B-Q4_K_M.gguf", form, true);
    // THEN each setting is sent under its key
    expect(request).toEqual({
      modelId: "Qwen3-8B-Q4_K_M.gguf",
      nCtx: 16384,
      nGpuLayers: 12,
      advanced: { n_batch: 512, n_threads: 8, seed: 42, kv_offload: false },
      remember: true,
    });
  });

  test("a model that takes no setting is asked for alone, whatever the window held", () => {
    // GIVEN an MLX model, and a window that held settings
    const form = { ...emptyForm(), nCtx: 16384, remember: true };
    // WHEN its request is built
    const request = loadRequest("Qwen3-4B-MLX-4bit", form, false);
    // THEN no setting and no wish to remember is sent
    expect(request).toEqual({ modelId: "Qwen3-4B-MLX-4bit" });
  });

  test("the context is held to the largest window the model accepts", () => {
    // GIVEN a model that accepts 40 960 tokens
    const e = estimate();
    // WHEN bounds are read, and values past both ends are typed
    const bounds = contextBounds(e);
    const tooLarge = clampContext(1_000_000, e);
    const tooSmall = clampContext(12, e);
    // THEN the field and the slider stop at the model's maximum
    expect(bounds).toEqual({ min: 1024, max: 40960, step: 1024 });
    expect(tooLarge).toBe(40960);
    expect(tooSmall).toBe(1024);
  });

  test("a header that names no maximum bounds the context at the window it loads with", () => {
    // GIVEN an estimate without a maximum
    const e = estimate({ maxCtx: undefined, nCtx: 8192 });
    // WHEN a larger context is typed
    const held = clampContext(32768, e);
    // THEN it stays at the window the model loads with
    expect(held).toBe(8192);
  });

  test("the offload is held between none and every layer, and 'all' reads as the count", () => {
    // GIVEN a model of 36 layers the plan places entirely (-1)
    const e = estimate({ nGpuLayers: -1 });
    // WHEN the offload is shown untouched, and values past both ends are typed
    const shown = shownOffload(emptyForm(), e);
    const held = [clampOffload(99, e), clampOffload(-4, e)];
    // THEN the window shows 36 of 36, and holds what is typed
    expect(shown).toBe(36);
    expect(held).toEqual([36, 0]);
  });

  test("the settings of the last load fill the window", () => {
    // GIVEN the settings recorded with the last model loaded
    const load = { n_ctx: 16384, n_gpu_layers: 20, n_threads: 6, kv_offload: false, mmap: true };
    // WHEN the window is filled from them
    const form = formFromSettings(load);
    // THEN each known setting is restored, the others left as the model has them
    expect(form).toMatchObject({
      nCtx: 16384,
      nGpuLayers: 20,
      threads: 6,
      kvOffload: false,
      batch: null,
      seed: null,
      remember: false,
    });
  });
});

describe("a load the guardrail refuses", () => {
  test("the refusal carries the estimate and the budget", () => {
    // GIVEN an estimate the balanced guardrail refuses, and one it allows
    // WHEN each is read
    const refusal = refusalOf(refused());
    const none = refusalOf(estimate());
    // THEN the window has the figures to show, and nothing for the allowed one
    expect(refusal).toEqual({ estimatedGb: 21.4, budgetGb: 16, level: "balanced" });
    expect(none).toBeNull();
  });

  test("'Load anyway' sends nothing until the user confirms explicitly", async () => {
    // GIVEN a refused estimate
    ipc.mockResolvedValue(outcome());
    // WHEN "Load anyway" is pressed, before any confirmation
    const attempt = await attemptLoad(calls, "Qwen3-8B-Q4_K_M.gguf", emptyForm(), refused(), false, false);
    // THEN the runtime was asked nothing, and a confirmation is required
    expect(attempt.kind).toBe("needs_confirmation");
    expect(ipc).not.toHaveBeenCalled();
  });

  test("once confirmed, the load is sent with that consent", async () => {
    // GIVEN a refused estimate
    ipc.mockResolvedValue(outcome());
    // WHEN the user confirms
    const attempt = await attemptLoad(calls, "Qwen3-8B-Q4_K_M.gguf", emptyForm(), refused(), true, false);
    // THEN the load goes through the route, carrying the confirmation
    expect(attempt.kind).toBe("loaded");
    expect(ipc).toHaveBeenCalledTimes(1);
    expect(ipc).toHaveBeenCalledWith("load_model", {
      request: { modelId: "Qwen3-8B-Q4_K_M.gguf", confirmed: true },
    });
  });

  test("a load the guardrail allows never carries a consent", async () => {
    // GIVEN an allowed estimate
    ipc.mockResolvedValue(outcome());
    // WHEN "Load" is pressed
    await attemptLoad(calls, "Qwen3-8B-Q4_K_M.gguf", emptyForm(), estimate(), false, false);
    // THEN the request names no confirmation
    expect(ipc).toHaveBeenCalledWith("load_model", { request: { modelId: "Qwen3-8B-Q4_K_M.gguf" } });
  });

  test("a load the runtime rejects reports its message and pins nothing", async () => {
    // GIVEN a runtime that rejects the load
    ipc.mockRejectedValue("the engine did not start");
    // WHEN "Load" is pressed with the pin set
    const form = { ...emptyForm(), pin: true };
    const attempt = await attemptLoad(calls, "Qwen3-8B-Q4_K_M.gguf", form, estimate(), false, false);
    // THEN the window gets the message, and no pin was written
    expect(attempt).toEqual({ kind: "failed", message: "the engine did not start" });
    expect(ipc).toHaveBeenCalledTimes(1);
  });
});

describe("keeping a model in memory from the window", () => {
  test("the pin set in the window is written once the model is loaded", async () => {
    // GIVEN a model that was not pinned, and the pin set in the window
    ipc.mockResolvedValue(outcome());
    const form = { ...emptyForm(), pin: true };
    // WHEN the model is loaded
    await attemptLoad(calls, "Qwen3-8B-Q4_K_M.gguf", form, estimate(), false, false);
    // THEN the pin follows the load, by the path of the model
    expect(ipc.mock.calls.map((c) => c[0])).toEqual(["load_model", "set_model_pin"]);
    expect(ipc).toHaveBeenLastCalledWith("set_model_pin", { modelPath: QWEN_PATH, pinned: true });
  });

  test("a pin left as it was writes nothing, and one taken off is removed", async () => {
    // GIVEN a pinned model
    ipc.mockResolvedValue(outcome());
    // WHEN it is loaded with the pin kept, then with the pin taken off
    await attemptLoad(calls, "Qwen3-8B-Q4_K_M.gguf", { ...emptyForm(), pin: true }, estimate(), false, true);
    const afterKept = ipc.mock.calls.map((c) => c[0]);
    await attemptLoad(calls, "Qwen3-8B-Q4_K_M.gguf", emptyForm(), estimate(), false, true);
    // THEN only the second writes the pin, as removed
    expect(afterKept).toEqual(["load_model"]);
    expect(ipc).toHaveBeenLastCalledWith("set_model_pin", { modelPath: QWEN_PATH, pinned: false });
  });
});

describe("ejecting the model of the conversation", () => {
  const agentChat = { mode: "agent" as const, agentName: "comptable", model: null };
  const freeChat = { mode: "libre" as const, agentName: null, model: null };

  test("the model of a conversation is the one chosen, else its agent's, else the default", () => {
    // GIVEN a conversation with an agent, a free one, and one on a chosen model
    // WHEN the model of each is read
    const ofAgent = conversationModel(groups, agentChat, "Qwen3-4B-MLX-4bit");
    const ofFree = conversationModel(groups, freeChat, "Qwen3-4B-MLX-4bit");
    const chosen = conversationModel(groups, { ...freeChat, model: "qwen3-8b-q4_k_m-gguf" }, null);
    // THEN each resolves to a model of the list, by identifier or by row name
    expect((ofAgent as ModelTarget).path).toBe(QWEN_PATH);
    expect((ofFree as ModelTarget).route).toBe("Qwen3-4B-MLX-4bit");
    expect((chosen as ModelTarget).path).toBe(QWEN_PATH);
  });

  test("the button unloads the resident model of the conversation, by its path", async () => {
    // GIVEN the model of the conversation in memory
    ipc.mockResolvedValue({ modelPath: QWEN_PATH, stopped: 1 });
    const model = loadedInstance(
      loaded([resident()]),
      conversationModel(groups, agentChat, null)?.path ?? null,
    );
    // WHEN it is ejected
    const attempt = await ejectModel(unloadLoadedModel, model);
    // THEN the unload route is called for that model
    expect(attempt.kind).toBe("ejected");
    expect(ipc).toHaveBeenCalledWith("unload_loaded_model", { modelPath: QWEN_PATH });
  });

  test("the button is off without a local model in memory, and asks nothing", async () => {
    // GIVEN nothing in memory, then a conversation on a model of a server
    const none = loadedInstance(loaded([]), QWEN_PATH);
    const ofServer = loadedInstance(
      loaded([resident()]),
      conversationModel(groups, { ...freeChat, model: "lab/llama-70b" }, null)?.path ?? null,
    );
    // WHEN the state of the button is read, and an eject is tried anyway
    const states = [canEject(none), canEject(ofServer)];
    const attempt = await ejectModel(unloadLoadedModel, none);
    // THEN it is disabled in both cases, and the runtime was not called
    expect(states).toEqual([false, false]);
    expect(attempt.kind).toBe("nothing");
    expect(ipc).not.toHaveBeenCalled();
  });

  test("a model that is answering cannot be ejected", () => {
    // GIVEN the model of the conversation answering a request
    // WHEN the state of the button is read
    const state = canEject(resident({ activeRequests: 1 }));
    // THEN it is disabled
    expect(state).toBe(false);
  });
});

describe("reloading the last model", () => {
  const last: LastLoadedModel = {
    modelId: "Qwen3-8B-Q4_K_M.gguf",
    load: { n_ctx: 16384 },
    loadedAt: "2026-10-05T18:00:00+00:00",
  };

  test("the line shows only with no chat model in memory and a last model known", () => {
    // GIVEN the embeddings instance alone in memory
    const embeddings = loaded([resident({ kind: "embeddings", modelPath: "/models/nomic.gguf" })]);
    // WHEN the line is asked for in each state
    const shown = [
      showReloadLast(embeddings, last),
      showReloadLast(loaded([resident()]), last),
      showReloadLast(embeddings, null),
      showReloadLast(loaded([], false), last),
      showReloadLast(null, last),
    ];
    // THEN it shows in the first alone
    expect(shown).toEqual([true, false, false, false, false]);
  });

  test("the shortcut is Cmd R on macOS and Ctrl R elsewhere", () => {
    // GIVEN the R key with each modifier
    const key = (over: Partial<KeyboardEvent>) => ({
      code: "KeyR",
      metaKey: false,
      ctrlKey: false,
      shiftKey: false,
      altKey: false,
      ...over,
    });
    // WHEN it is read on each platform
    const answers = [
      isReloadShortcut(key({ metaKey: true }), true),
      isReloadShortcut(key({ ctrlKey: true }), true),
      isReloadShortcut(key({ ctrlKey: true }), false),
      isReloadShortcut(key({ metaKey: true }), false),
      isReloadShortcut(key({ ctrlKey: true, shiftKey: true }), false),
    ];
    // THEN each platform answers to its own modifier, alone
    expect(answers).toEqual([true, false, true, false, false]);
  });

  test("the line reloads through the route of the last model", async () => {
    // GIVEN a runtime that loads the last model again
    ipc.mockResolvedValue(outcome());
    // WHEN the line is pressed
    const attempt = await reloadLast(reloadLastLoadedModel, last);
    // THEN the reload route is called, without a consent
    expect(attempt.kind).toBe("loaded");
    expect(ipc).toHaveBeenCalledWith("reload_last_loaded_model", { confirmed: false });
  });

  test("a reload the runtime refuses hands the model to the loading window", async () => {
    // GIVEN a runtime that refuses the reload
    ipc.mockRejectedValue("loading was refused by the balanced load guardrail");
    // WHEN the line is pressed
    const attempt = await reloadLast(reloadLastLoadedModel, last);
    // THEN the window is to open on that model, with the reason
    expect(attempt).toEqual({
      kind: "window",
      last,
      message: "loading was refused by the balanced load guardrail",
    });
  });
});

describe("choosing a model that is not in memory", () => {
  test("with the full loading window on, the window opens and nothing is loaded", async () => {
    // GIVEN the setting on, as it is by default
    // WHEN a model that is not in memory is chosen
    const chosen = await loadChosen(true, loadModel, qwen.route);
    // THEN the answer is the window, with no refusal, and the runtime was not asked
    expect(chosen).toEqual({ kind: "window", refusal: null });
    expect(ipc).not.toHaveBeenCalled();
  });

  test("with it off, the model loads at once under its default preset", async () => {
    // GIVEN the setting off, and a runtime that loads
    const outcome: ModelLoadOutcome = { modelId: qwen.route, loaded: resident() };
    ipc.mockResolvedValue(outcome);
    // WHEN the model is chosen
    const chosen = await loadChosen(false, loadModel, qwen.route);
    // THEN the explicit load route is called with the model alone: no
    //      setting over its default preset, and no consent to the guardrail
    expect(chosen).toEqual({ kind: "loaded", outcome });
    expect(ipc).toHaveBeenCalledTimes(1);
    expect(ipc).toHaveBeenCalledWith("load_model", { request: { modelId: qwen.route } });
  });

  test("with it off, a load the guardrail refuses opens the window with the refusal", async () => {
    // GIVEN the setting off, and a runtime whose guardrail refuses the load
    ipc.mockRejectedValue("loading was refused by the balanced load guardrail");
    // WHEN the model is chosen
    const chosen = await loadChosen(false, loadModel, qwen.route);
    // THEN the window opens and carries the refusal, and no second,
    //      confirmed load was sent
    expect(chosen).toEqual({
      kind: "window",
      refusal: "loading was refused by the balanced load guardrail",
    });
    expect(ipc).toHaveBeenCalledTimes(1);
  });
});
