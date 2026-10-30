/**
 * The chat targets and the models in memory are read by overlapping calls:
 * the selector, the reload line and the guide each read them again. A read
 * asked earlier that answers later must not put back what it saw.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

type Pending<T> = { resolve: (value: T) => void };
const queues: Record<string, Pending<unknown>[]> = {};

function deferred(command: string): Promise<unknown> {
  return new Promise((resolve) => {
    (queues[command] ??= []).push({ resolve });
  });
}

function answer(command: string, index: number, value: unknown): void {
  queues[command][index].resolve(value);
}

vi.mock("$lib/ipc/chatTargets", () => ({
  listChatTargets: () => deferred("list_chat_targets"),
}));
vi.mock("$lib/ipc/llm", () => ({
  listLoadedModels: () => deferred("list_loaded_models"),
}));
vi.mock("$lib/ipc/llmLoad", () => ({
  getLastLoadedModel: () => deferred("get_last_loaded_model"),
  listModelPins: () => deferred("list_model_pins"),
}));

import { chatTargets, refreshChatTargets } from "$lib/stores/chatTargets";
import { lastLoadedModel, loadedModels, refreshModelResidency } from "$lib/stores/modelResidency";

function targets(defaultModel: string | null) {
  return {
    agents: [],
    installed: [],
    servers: [],
    default_model: defaultModel === null ? null : { model_id: defaultModel },
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  for (const key of Object.keys(queues)) delete queues[key];
});

describe("refreshChatTargets", () => {
  test("keeps the answer of the later read when the earlier one answers last", async () => {
    // GIVEN two reads of the list, the first still waiting on the servers
    const first = refreshChatTargets();
    const second = refreshChatTargets();

    // WHEN the second answers, then the first
    answer("list_chat_targets", 1, targets(null));
    await settle();
    answer("list_chat_targets", 0, targets("office/qwen"));
    await Promise.all([first, second]);

    // THEN the list is the one the later read saw
    expect(get(chatTargets).defaultModel).toBeNull();
  });

  test("takes a later read that answers last", async () => {
    // GIVEN two reads of the list
    const first = refreshChatTargets();
    const second = refreshChatTargets();

    // WHEN they answer in the order they were asked
    answer("list_chat_targets", 0, targets(null));
    await settle();
    answer("list_chat_targets", 1, targets("office/qwen"));
    await Promise.all([first, second]);

    // THEN the list is the one the later read saw
    expect(get(chatTargets).defaultModel).toBe("office/qwen");
  });
});

describe("refreshModelResidency", () => {
  test("keeps the answers of the later reads when the earlier ones answer last", async () => {
    // GIVEN two reads of what is in memory, the first still waiting
    const first = refreshModelResidency();
    const second = refreshModelResidency();

    // WHEN the second answers with no model in memory and a last model, then the first
    answer("list_loaded_models", 1, { engineAvailable: true, maxLoaded: null, models: [] });
    answer("get_last_loaded_model", 1, { modelId: "phi", load: {}, loadedAt: "2026-01-01T00:00:00Z" });
    answer("list_model_pins", 1, { pins: [] });
    await settle();
    answer("list_loaded_models", 0, { engineAvailable: false, maxLoaded: null, models: [] });
    answer("get_last_loaded_model", 0, null);
    answer("list_model_pins", 0, { pins: ["/models/old.gguf"] });
    await Promise.all([first, second]);

    // THEN each store holds what the later read saw
    expect(get(loadedModels)?.engineAvailable).toBe(true);
    expect(get(lastLoadedModel)?.modelId).toBe("phi");
  });
});
