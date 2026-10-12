import { describe, expect, test } from "vitest";
import { applyModelLoadEvent, modelLoadEventOf, modelNameOf, type ModelLoad } from "./modelLoads";

const PATH = "/models/Vision-30B-Q4_K_M/Vision-30B-Q4_K_M.gguf";

describe("modelNameOf", () => {
  test("a model is named by its file, without folders or extension", () => {
    // GIVEN the path of a GGUF file in a folder, and the folder of an MLX model
    // WHEN each is named
    // THEN the first loses its folders and extension, the second its folders
    expect(modelNameOf(PATH)).toBe("Vision-30B-Q4_K_M");
    expect(modelNameOf("/models/Qwen3-4B-MLX-4bit")).toBe("Qwen3-4B-MLX-4bit");
  });
});

describe("applyModelLoadEvent", () => {
  const loading: ModelLoad[] = [{ engine: "llama-server", name: "Vision-30B-Q4_K_M" }];

  test("a process started on a model makes it a load", () => {
    // GIVEN no load
    // WHEN the engine says it started on a model
    const loads = applyModelLoadEvent([], {
      type: "LlmModelLoading",
      backend: "llama-server",
      model_path: PATH,
    });
    // THEN the model is being loaded
    expect(loads).toEqual(loading);
  });

  test("the same model started twice is one load", () => {
    // GIVEN a model being loaded
    // WHEN the engine says again it started on it
    const loads = applyModelLoadEvent(loading, {
      type: "LlmModelLoading",
      backend: "llama-server",
      model_path: PATH,
    });
    // THEN it is still listed once
    expect(loads).toEqual(loading);
  });

  test("a model that answers is no longer a load", () => {
    // GIVEN a model being loaded
    // WHEN the engine says it answers, by the name it gives it
    const loads = applyModelLoadEvent(loading, {
      type: "LlmModelReady",
      backend: "llama-server",
      model_id: "Vision-30B-Q4_K_M",
    });
    // THEN nothing is loading
    expect(loads).toEqual([]);
  });

  test("a model that did not come up is no longer a load", () => {
    // GIVEN two models being loaded
    const two = [...loading, { engine: "llama-server", name: "Plain-4B-Q4_K_M" }];
    // WHEN the engine says the first did not come up
    const loads = applyModelLoadEvent(two, {
      type: "LlmModelFailed",
      backend: "llama-server",
      reason: "Vision-30B-Q4_K_M: llama-server exited during startup",
    });
    // THEN only the second is still loading
    expect(loads).toEqual([{ engine: "llama-server", name: "Plain-4B-Q4_K_M" }]);
  });

  test("a router naming its configuration starts no load", () => {
    // GIVEN no load
    // WHEN a backend that is not an engine says it is loading
    const loads = applyModelLoadEvent([], {
      type: "LlmModelLoading",
      backend: "local",
      model_path: "~/.apollia/models/x.gguf",
    });
    // THEN nothing is loading: no process was started
    expect(loads).toEqual([]);
  });
});

describe("modelLoadEventOf", () => {
  test("the three events of a load are read from their payload", () => {
    // GIVEN the payloads the runtime sends
    // WHEN each is read
    // THEN it carries its fields
    expect(
      modelLoadEventOf("LlmModelLoading", {
        LlmModelLoading: { backend: "llama-server", model_path: PATH },
      }),
    ).toEqual({ type: "LlmModelLoading", backend: "llama-server", model_path: PATH });
    expect(
      modelLoadEventOf("LlmModelReady", {
        LlmModelReady: { backend: "llama-server", model_id: "Vision-30B-Q4_K_M" },
      }),
    ).toEqual({ type: "LlmModelReady", backend: "llama-server", model_id: "Vision-30B-Q4_K_M" });
    expect(
      modelLoadEventOf("LlmModelFailed", {
        LlmModelFailed: { backend: "llama-server", reason: "x: gone" },
      }),
    ).toEqual({ type: "LlmModelFailed", backend: "llama-server", reason: "x: gone" });
  });

  test("another event, or a payload that does not carry its fields, is not a load event", () => {
    // GIVEN an event of another kind, and a load event with a field missing
    // WHEN each is read
    // THEN neither is taken for a load event
    expect(modelLoadEventOf("LlmCallCompleted", { LlmCallCompleted: { backend: "x" } })).toBeNull();
    expect(modelLoadEventOf("LlmModelLoading", { LlmModelLoading: { backend: "llama-server" } })).toBeNull();
    expect(modelLoadEventOf("LlmModelReady", {})).toBeNull();
  });
});
