import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { get } from "svelte/store";
import {
  ENDPOINTS_PAGE,
  copyIdentifier,
  detailAsk,
  ejectServed,
  endpointsDestination,
  followResidency,
  movesResidency,
  openEndpoints,
  readServedItems,
  servedIdentifier,
  servedItemSlug,
  servedModelId,
  servedModelKind,
  servedModelParallel,
  servedModelSize,
  servedModels,
  type ResidencyEvent,
  type ServedItemsIpc,
} from "./servedItems";
import { capabilitiesOf } from "../ai/explanations";
import { canUnload } from "../ai/loadedModels";
import { docsUrlFor } from "$lib/utils/docsUrl";
import { loadedModels } from "$lib/stores/modelResidency";
import type { McpExposableItem } from "$lib/ipc/mcpServer";
import type { WorkflowView } from "$lib/ipc/workflows";
import type { LoadedModel, LoadedModelsResponse } from "$lib/types";

const HERE = path.dirname(fileURLToPath(import.meta.url));

function source(relative: string): string {
  return readFileSync(path.resolve(HERE, relative), "utf8");
}

function model(over: Partial<LoadedModel> = {}): LoadedModel {
  return {
    engine: "llama-server",
    kind: "chat",
    model: "qwen3-8b-q4",
    modelPath: "/models/qwen3-8b-q4.gguf",
    port: 57606,
    nCtx: 8192,
    nGpuLayers: 99,
    nParallel: 4,
    loadedAt: "2026-10-01T08:00:00Z",
    lastUsedAt: "2026-10-01T08:05:00Z",
    activeRequests: 0,
    footprintGb: 6.2,
    modelId: "qwen/qwen3-8b",
    sizeBytes: 5_200_000_000,
    capabilities: { tools: true, vision: false, reasoning: true },
    ...over,
  } as LoadedModel;
}

function loaded(models: LoadedModel[]): LoadedModelsResponse {
  return { engineAvailable: true, models } as LoadedModelsResponse;
}

/**
 * The runtime behind the cards, held in memory: the models it keeps, the
 * unload route, and the bus a screen listens to.
 */
function fakeRuntime(initial: LoadedModel[]) {
  let resident = [...initial];
  const handlers = new Set<(event: ResidencyEvent) => void>();
  return {
    list: vi.fn(async () => loaded(resident)),
    unload: vi.fn(async (modelPath: string) => {
      const target = resident.find((m) => m.modelPath === modelPath);
      if (target && target.activeRequests > 0) throw new Error("model is in use");
      resident = resident.filter((m) => m.modelPath !== modelPath);
    }),
    /** A load or an unload made elsewhere: the chat, a client, the idle sweep. */
    change(next: LoadedModel[], eventType: string): void {
      resident = [...next];
      for (const handler of handlers) handler({ event_type: eventType });
    },
    emit(eventType: string): void {
      for (const handler of handlers) handler({ event_type: eventType });
    },
    listen(handler: (event: ResidencyEvent) => void): () => void {
      handlers.add(handler);
      return () => handlers.delete(handler);
    },
    listeners: () => handlers.size,
  };
}

function exposable(over: Partial<McpExposableItem>): McpExposableItem {
  return {
    kind: "agent",
    item_id: "qualifier-demandes",
    name: "Qualify requests",
    description: "",
    exposed: true,
    ...over,
  };
}

function workflow(names: string[]): WorkflowView {
  return {
    workflowId: "factures/relance",
    version: "1.0.0",
    description: "",
    steps: [],
    inputs: names.map((name) => ({ name, kind: "text", required: true, nullable: false })),
  } as WorkflowView;
}

describe("the card of a loaded model", () => {
  test("states the kind, identifier, capabilities, size and parallelism of a language model", () => {
    // GIVEN a language model in memory, held by the model index
    const item = model();
    // WHEN the card reads it
    // THEN it has the kind of the server, its index identifier, its
    // capabilities, the size of its weights and its parallel requests
    expect(servedModelKind(item)).toBe("llm");
    expect(servedModelId(item)).toBe("qwen/qwen3-8b");
    expect(capabilitiesOf(item.capabilities)).toEqual(["tools", "reasoning"]);
    expect(servedModelSize(item)).toBe(5_200_000_000);
    expect(servedModelParallel(item)).toBe(4);
    expect(canUnload(item)).toBe(true);
  });

  test("names an embeddings model by its kind, and a file outside the index by its name", () => {
    // GIVEN the embeddings instance, loaded from a file the index does not hold
    const item = model({
      kind: "embeddings",
      modelId: null,
      model: "nomic-embed",
      sizeBytes: null,
      nParallel: null,
    });
    // WHEN the card reads it
    // THEN the kind is embeddings, the identifier is the file name, and the
    // facts the runtime does not know are left out
    expect(servedModelKind(item)).toBe("embeddings");
    expect(servedModelId(item)).toBe("nomic-embed");
    expect(servedModelSize(item)).toBeNull();
    expect(servedModelParallel(item)).toBeNull();
  });

  test("copies the identifier, and says when the copy fails", async () => {
    // GIVEN a clipboard that takes the text, and one that refuses it
    const written: string[] = [];
    const item = model();
    // WHEN the identifier is copied to each
    const done = await copyIdentifier(servedModelId(item), async (text) => {
      written.push(text);
    });
    const refused = await copyIdentifier(servedModelId(item), async () => {
      throw new Error("denied");
    });
    // THEN the first holds the identifier and the second answers false
    expect(written).toEqual(["qwen/qwen3-8b"]);
    expect(done).toBe(true);
    expect(refused).toBe(false);
  });

  test("eject unloads the model through the runtime and the cards lose it", async () => {
    // GIVEN two models in memory behind the simulated IPC
    const kept = model({ modelPath: "/models/b.gguf", modelId: "b" });
    const runtime = fakeRuntime([model(), kept]);
    const refresh = async () => loadedModels.set(await runtime.list());
    await refresh();
    // WHEN the first is ejected
    const notice = await ejectServed(runtime.unload, refresh, model());
    // THEN the unload route took its path, the list was read again, and the
    // reader is told its memory is free
    expect(runtime.unload).toHaveBeenCalledWith("/models/qwen3-8b-q4.gguf");
    expect(servedModels(get(loadedModels)).map(servedModelId)).toEqual(["b"]);
    expect(notice).toEqual({
      key: "ai.loaded.unloaded_toast",
      values: { name: "qwen/qwen3-8b" },
      tone: "success",
    });
  });

  test("a model that answers is not ejected, and a refusal is said in its words", async () => {
    // GIVEN a model answering a request, and a runtime that refuses another
    const answering = model({ activeRequests: 1 });
    const runtime = fakeRuntime([answering]);
    const refresh = vi.fn(async () => undefined);
    // WHEN each is ejected
    const nothing = await ejectServed(runtime.unload, refresh, answering);
    const failed = await ejectServed(
      async () => {
        throw new Error("the Apollia service does not answer");
      },
      refresh,
      model(),
    );
    // THEN the first asks nothing of the runtime and the second carries the reason
    expect(runtime.unload).not.toHaveBeenCalled();
    expect(nothing).toBeNull();
    expect(failed).toEqual({
      key: "dev.developer.models.eject_failed",
      values: { name: "qwen/qwen3-8b", error: "the Apollia service does not answer" },
      tone: "error",
    });
  });

  test("the arrow opens the loading window on the model, and on nothing for a file outside the index", () => {
    // GIVEN a model of the index and a file the index does not hold
    const known = model({ engine: "vllm-mlx" });
    const unknown = model({ modelId: null });
    const onloaded = () => undefined;
    // WHEN the detail is asked for each
    // THEN the window is asked for the first, with its format, and not for the second
    expect(detailAsk(known, onloaded)).toEqual({ modelId: "qwen/qwen3-8b", format: "mlx", onloaded });
    expect(detailAsk(unknown, onloaded)).toBeNull();
  });
});

describe("one card for both screens", () => {
  test("the AI screen and the developer screen draw the same loaded model card", () => {
    // GIVEN the section of the AI screen and the section of the developer screen
    const ai = source("../ai/LoadedModelsSection.svelte");
    const developer = source("./ServerModelsSection.svelte");
    // WHEN their imports are read
    // THEN both import the one card, and neither draws its own
    expect(ai).toMatch(/import LoadedModelCard from "\.\/LoadedModelCard\.svelte"/);
    expect(developer).toMatch(/import LoadedModelCard from "\.\.\/ai\/LoadedModelCard\.svelte"/);
    expect(ai).not.toContain('data-testid="ai-loaded-card"');
    expect(developer).not.toContain('data-testid="ai-loaded-card"');
  });

  test("an exposed item is drawn on the layout of the loaded model card", () => {
    // GIVEN the card of a loaded model and the section of the exposed items
    const card = source("../ai/LoadedModelCard.svelte");
    const exposed = source("./ExposedItemsSection.svelte");
    // WHEN their imports are read
    // THEN both stand on the same layout
    expect(card).toMatch(/import ServedCard from "\.\/ServedCard\.svelte"/);
    expect(exposed).toMatch(/import ServedCard from "\.\.\/ai\/ServedCard\.svelte"/);
  });
});

describe("the exposed agents and workflows", () => {
  test("only the exposed items are listed, each under apollia/<id> with its kind and inputs", async () => {
    // GIVEN an exposed agent, an agent that is not, and an exposed workflow with two inputs
    const ipc: ServedItemsIpc = {
      list: async () => [
        exposable({}),
        exposable({ item_id: "brouillon", exposed: false }),
        exposable({ kind: "workflow", item_id: "factures/relance", name: "Reminder" }),
      ],
      workflow: vi.fn(async () => workflow(["client", "periode"])),
    };
    // WHEN the served items are read
    const items = await readServedItems(ipc);
    // THEN the two exposed ones come out, the agent taking a message and the
    // workflow the fields of its inputs
    expect(items).toEqual([
      {
        kind: "agent",
        itemId: "qualifier-demandes",
        identifier: "apollia/qualifier-demandes",
        name: "Qualify requests",
        inputs: { kind: "message" },
      },
      {
        kind: "workflow",
        itemId: "factures/relance",
        identifier: "apollia/factures/relance",
        name: "Reminder",
        inputs: { kind: "fields", names: ["client", "periode"] },
      },
    ]);
    expect(ipc.workflow).toHaveBeenCalledTimes(1);
    expect(ipc.workflow).toHaveBeenCalledWith("factures/relance");
  });

  test("a workflow whose inputs cannot be read is still listed", async () => {
    // GIVEN an exposed workflow the shell cannot describe
    const ipc: ServedItemsIpc = {
      list: async () => [exposable({ kind: "workflow", item_id: "factures/relance" })],
      workflow: async () => {
        throw new Error("unknown workflow");
      },
    };
    // WHEN the served items are read
    const items = await readServedItems(ipc);
    // THEN it is there, with inputs said unknown
    expect(items.map((i) => [i.identifier, i.inputs.kind])).toEqual([
      ["apollia/factures/relance", "unknown"],
    ]);
  });

  test("with nothing exposed the list is empty, and the section sends to the exposed flag", async () => {
    // GIVEN agents none of which is exposed
    const ipc: ServedItemsIpc = {
      list: async () => [exposable({ exposed: false })],
      workflow: async () => workflow([]),
    };
    // WHEN the served items are read, and the source of the section too
    const items = await readServedItems(ipc);
    const section = source("./ExposedItemsSection.svelte");
    // THEN nothing is listed, and the empty state opens the settings section
    // that holds the flag
    expect(items).toEqual([]);
    expect(section).toContain('data-testid="developer-exposed-empty"');
    expect(section).toMatch(
      /onclick=\{\(\) => openBridgeSettings\(openSettingsWindow\)\}\s+data-testid="developer-exposed-open-settings"/,
    );
  });

  test("the identifier and the test id of a card are stable", () => {
    // GIVEN a workflow of a package
    // WHEN its identifier and the test id of its card are built
    // THEN the identifier keeps the id and the test id part holds no slash
    expect(servedIdentifier("factures/relance")).toBe("apollia/factures/relance");
    expect(
      servedItemSlug({
        kind: "workflow",
        itemId: "factures/relance",
        identifier: "apollia/factures/relance",
        name: "Reminder",
        inputs: { kind: "unknown" },
      }),
    ).toBe("factures_relance");
  });
});

describe("supported endpoints", () => {
  test("opens the page in the documentation pane when the screen can", () => {
    // GIVEN a screen whose documentation pane opens a page
    const openPage = vi.fn();
    const openSite = vi.fn();
    // WHEN the foot is clicked
    const destination = openEndpoints(openPage, openSite, (p) => docsUrlFor("fr", p));
    // THEN the page of the compatible endpoints opens in the pane, not in the browser
    expect(destination).toEqual({ kind: "pane", page: ENDPOINTS_PAGE });
    expect(openPage).toHaveBeenCalledWith("reference/compatible-endpoints");
    expect(openSite).not.toHaveBeenCalled();
  });

  test("opens the documentation site while the pane reads no page", () => {
    // GIVEN a screen whose documentation pane opens nothing
    const openSite = vi.fn();
    // WHEN the foot is clicked, in French then in English
    const fr = openEndpoints(undefined, openSite, (p) => docsUrlFor("fr", p));
    const en = endpointsDestination(false, (p) => docsUrlFor("en", p));
    // THEN the same page opens on the site, under the locale of the reader
    expect(fr.kind).toBe("site");
    expect(openSite).toHaveBeenCalledTimes(1);
    const [url] = openSite.mock.calls[0] as [string];
    expect(url).toBe(docsUrlFor("fr", "/reference/compatible-endpoints"));
    expect(url.endsWith("/fr/reference/compatible-endpoints")).toBe(true);
    expect(en).toEqual({ kind: "site", url: docsUrlFor("en", "/reference/compatible-endpoints") });
  });

  test("the pane mounts the foot", () => {
    // GIVEN the local server pane
    const pane = source("./LocalServerPane.svelte");
    // WHEN its markup is read
    // THEN the foot is there, handed the way to the documentation pane
    expect(pane).toContain("<SupportedEndpoints {onpage} />");
  });
});

describe("the cards follow loads and unloads", () => {
  test("a model unloaded elsewhere leaves the cards without a manual reload", async () => {
    // GIVEN two models in memory and a screen following the runtime events
    const a = model();
    const b = model({ modelPath: "/models/b.gguf", modelId: "b" });
    const runtime = fakeRuntime([a, b]);
    const refresh = vi.fn(async () => loadedModels.set(await runtime.list()));
    await refresh();
    await followResidency(runtime.listen, refresh);
    // WHEN the idle sweep unloads one
    runtime.change([b], "LlmModelUnloaded");
    await vi.waitFor(() => expect(servedModels(get(loadedModels))).toHaveLength(1));
    // THEN the cards show the one that remains
    expect(servedModels(get(loadedModels)).map(servedModelId)).toEqual(["b"]);
  });

  test("a model loaded elsewhere joins the cards", async () => {
    // GIVEN no model in memory and a screen following the runtime events
    const runtime = fakeRuntime([]);
    const refresh = vi.fn(async () => loadedModels.set(await runtime.list()));
    await refresh();
    await followResidency(runtime.listen, refresh);
    // WHEN a call to the server loads one and starts answering with it
    runtime.change([model({ activeRequests: 1 })], "LlmCallStarted");
    await vi.waitFor(() => expect(servedModels(get(loadedModels))).toHaveLength(1));
    // THEN its card is there, answering
    const [card] = servedModels(get(loadedModels));
    expect(card && servedModelId(card)).toBe("qwen/qwen3-8b");
    expect(card && canUnload(card)).toBe(false);
  });

  test("an event that moves no model reads nothing, and leaving stops following", async () => {
    // GIVEN a screen following the runtime events
    const runtime = fakeRuntime([]);
    const refresh = vi.fn();
    const stop = await followResidency(runtime.listen, refresh);
    // WHEN a budget update and a task event pass, then the screen leaves
    runtime.emit("TokenBudgetUpdated");
    runtime.emit("TaskStarted");
    stop();
    runtime.emit("LlmModelUnloaded");
    // THEN nothing was read, before or after
    expect(refresh).not.toHaveBeenCalled();
    expect(runtime.listeners()).toBe(0);
  });

  test("every event of a load, an unload or a call moves the cards", () => {
    // GIVEN the events of the life of a model in memory
    const moving = [
      "LlmModelLoading",
      "LlmModelReady",
      "LlmModelReloaded",
      "LlmModelUnloaded",
      "LlmModelFailed",
      "LlmCallStarted",
      "LlmCallCompleted",
      "LlmCallFailed",
    ];
    // WHEN each is asked
    // THEN each moves the cards, and an unrelated one does not
    expect(moving.filter(movesResidency)).toEqual(moving);
    expect(movesResidency("AgentReady")).toBe(false);
  });

  test("the section listens to the runtime events on mount", () => {
    // GIVEN the section of the loaded models
    const section = source("./ServerModelsSection.svelte");
    // WHEN its script is read
    // THEN it follows the residency on the runtime event channel
    expect(section).toMatch(/followResidency\(\s*\(handler\) => listen<ResidencyEvent>\("runtime-event"/);
  });
});
