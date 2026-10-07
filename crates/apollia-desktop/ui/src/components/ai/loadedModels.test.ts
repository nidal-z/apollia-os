import { describe, test, expect } from "vitest";
import type { LoadedModel } from "$lib/types";
import { canUnload, isKeptInMemory, showLoadedModels, unloadErrorKey } from "./loadedModels";

function model(activeRequests: number): LoadedModel {
  return {
    engine: "llama-server",
    kind: "chat",
    model: "a",
    modelPath: "/models/a.gguf",
    port: 41001,
    nCtx: 4096,
    nGpuLayers: 0,
    loadedAt: "2026-10-01T09:00:00+00:00",
    lastUsedAt: "2026-10-01T09:30:00+00:00",
    activeRequests,
  };
}

describe("loadedModels", () => {
  test("a model answering a request cannot be unloaded", () => {
    // GIVEN a resident model serving one response
    const busy = model(1);

    // WHEN the block asks whether it can offer the unload
    const allowed = canUnload(busy);

    // THEN it cannot
    expect(allowed).toBe(false);
  });

  test("an idle model can be unloaded", () => {
    // GIVEN a resident model serving nothing
    const idle = model(0);

    // WHEN the block asks whether it can offer the unload
    const allowed = canUnload(idle);

    // THEN it can
    expect(allowed).toBe(true);
  });

  test("the block stays hidden without a local engine or before the first answer", () => {
    // GIVEN no answer yet, then an answer from a host without a local engine
    const none = null;
    const noEngine = { engineAvailable: false, maxLoaded: null, models: [] };

    // WHEN the block decides whether to show
    const shown = [showLoadedModels(none), showLoadedModels(noEngine)];

    // THEN it shows in neither case
    expect(shown).toEqual([false, false]);
  });

  test("an engine with nothing loaded still shows the block", () => {
    // GIVEN a local engine holding no model
    const empty = { engineAvailable: true, maxLoaded: 1, models: [] };

    // WHEN the block decides whether to show
    const shown = showLoadedModels(empty);

    // THEN it shows, to say nothing is loaded
    expect(shown).toBe(true);
  });

  test("the runtime's in-use refusal maps to the block's own wording", () => {
    // GIVEN the message the runtime rejects an unload with while a response runs
    const message =
      "model /models/a.gguf is in use: 1 response(s) in progress, retry once they finish";

    // WHEN the block looks for a key of its own
    const key = unloadErrorKey(message);

    // THEN it finds the in-use wording, and nothing for another failure
    expect(key).toBe("ai.loaded.in_use_error");
    expect(unloadErrorKey("model /models/a.gguf is not loaded")).toBeNull();
  });

  test("a loaded model whose path is pinned is marked as kept in memory", () => {
    // GIVEN two loaded models, and the pins of the machine naming the first
    const kept = model(0);
    const other = { ...model(0), modelPath: "/models/b.gguf" };
    const pins = ["/models/a.gguf", "/models/gone.gguf"];

    // WHEN the section asks which one carries the mark
    const marks = [isKeptInMemory(pins, kept), isKeptInMemory(pins, other)];

    // THEN only the pinned one does
    expect(marks).toEqual([true, false]);
  });

  test("without any pin no loaded model is marked", () => {
    // GIVEN a loaded model and no pin
    // WHEN the section asks whether it carries the mark
    const mark = isKeptInMemory([], model(0));

    // THEN it does not
    expect(mark).toBe(false);
  });
});
