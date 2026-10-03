import { describe, test, expect } from "vitest";
import type { LoadedModel } from "$lib/types";
import { canUnload, showLoadedModels, unloadErrorKey } from "./loadedModels";

function model(activeRequests: number): LoadedModel {
  return {
    engine: "llama-server",
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
    const noEngine = { engineAvailable: false, maxLoaded: 0, models: [] };

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
    expect(key).toBe("llm.loaded.in_use_error");
    expect(unloadErrorKey("model /models/a.gguf is not loaded")).toBeNull();
  });
});
