import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  revealInstalledModel,
  setDefaultModel,
  setEmbeddingsDefault,
  setInstalledModelPinned,
} from "./aiModels";
import { unloadLoadedModel } from "./llm";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
});

describe("the AI screen reaches the runtime through its commands", () => {
  test("unload asks the runtime to stop the model by its path", async () => {
    // GIVEN a runtime that stops the model
    mockedInvoke.mockResolvedValue({ modelPath: "/m/qwen.gguf", stopped: 1 });

    // WHEN the Unload button of a loaded model is played
    const result = await unloadLoadedModel("/m/qwen.gguf");

    // THEN the unload command carries the path, and its answer flows back
    expect(mockedInvoke).toHaveBeenCalledWith("unload_loaded_model", { modelPath: "/m/qwen.gguf" });
    expect(result.stopped).toBe(1);
  });

  test("a refused unload rejects with the runtime's words", async () => {
    // GIVEN a runtime that refuses a model answering a request
    mockedInvoke.mockRejectedValue(new Error("model /m/qwen.gguf is in use by 1 request(s)"));

    // WHEN the unload is played
    // THEN the promise rejects with the refusal the screen phrases
    await expect(unloadLoadedModel("/m/qwen.gguf")).rejects.toThrow(/in use/);
  });

  test("use by default names the model by its index identifier", async () => {
    // GIVEN a runtime that makes the model the default
    mockedInvoke.mockResolvedValue({ backend: "local-qwen", model_source: "local", model_id: "local-qwen" });

    // WHEN Use by default is played on a row
    await setDefaultModel("qwen3-8b");

    // THEN the command carries the identifier of the index
    expect(mockedInvoke).toHaveBeenCalledWith("set_default_model", { modelId: "qwen3-8b" });
  });

  test("use for search names the registry entry of the embeddings file", async () => {
    // GIVEN a runtime that records the embeddings model
    mockedInvoke.mockResolvedValue({ model: "multilingual-e5-small", installed: true, path: "/m/e5.gguf" });

    // WHEN Use for search is confirmed
    const answer = await setEmbeddingsDefault("multilingual-e5-small");

    // THEN the command carries the entry, and the runtime's answer flows back
    expect(mockedInvoke).toHaveBeenCalledWith("set_embeddings_default", { entry: "multilingual-e5-small" });
    expect(answer.installed).toBe(true);
  });

  test("pin and reveal carry the identifier and the path", async () => {
    // GIVEN a runtime that accepts both
    mockedInvoke.mockResolvedValue(undefined);

    // WHEN a model is pinned, unpinned and shown in the file manager
    await setInstalledModelPinned("qwen3-8b", true);
    await setInstalledModelPinned("qwen3-8b", false);
    await revealInstalledModel("/m/qwen.gguf");

    // THEN each command receives what it needs
    expect(mockedInvoke).toHaveBeenNthCalledWith(1, "set_installed_model_pinned", { id: "qwen3-8b", pinned: true });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, "set_installed_model_pinned", { id: "qwen3-8b", pinned: false });
    expect(mockedInvoke).toHaveBeenNthCalledWith(3, "reveal_installed_model", { path: "/m/qwen.gguf" });
  });
});
