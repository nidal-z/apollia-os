/**
 * The local models an engine is loading right now.
 *
 * Starting an engine process on a model takes seconds, tens of them for a
 * large one. The runtime says when a process is started, when it answers and
 * when it did not come up; this store keeps the models between the first and
 * one of the other two, so a surface can say "this model is loading" instead
 * of looking idle or broken.
 */
import { writable } from "svelte/store";

/** A model an engine is loading. */
export interface ModelLoad {
  /** Wire name of the engine: `llama-server` or `vllm-mlx`. */
  engine: string;
  /** Name of the model: its file without folders or extension. */
  name: string;
}

/** The engines that load a model in a process of their own. */
const ENGINES = new Set(["llama-server", "vllm-mlx"]);

/** The name the runtime gives a model once loaded, from the path it loads. */
export function modelNameOf(modelPath: string): string {
  const last = modelPath.split(/[\\/]/).filter(Boolean).pop() ?? modelPath;
  return last.replace(/\.gguf$/i, "");
}

/** One of the three events the runtime publishes around a load. */
export type ModelLoadEvent =
  | { type: "LlmModelLoading"; backend: string; model_path: string }
  | { type: "LlmModelReady"; backend: string; model_id: string }
  | { type: "LlmModelFailed"; backend: string; reason: string };

/**
 * The loads once `event` is heard. An event of a backend that is not an engine
 * (a router naming its configuration) changes nothing.
 */
export function applyModelLoadEvent(loads: readonly ModelLoad[], event: ModelLoadEvent): ModelLoad[] {
  if (!ENGINES.has(event.backend)) return [...loads];
  const others = (name: string) =>
    loads.filter((load) => !(load.engine === event.backend && load.name === name));
  switch (event.type) {
    case "LlmModelLoading": {
      const name = modelNameOf(event.model_path);
      return [...others(name), { engine: event.backend, name }];
    }
    case "LlmModelReady":
      return others(event.model_id);
    case "LlmModelFailed":
      // The reason opens with the name of the model, then a colon.
      return loads.filter(
        (load) => !(load.engine === event.backend && event.reason.startsWith(`${load.name}: `)),
      );
  }
}

/** The models being loaded, in the order their load started. */
export const modelLoads = writable<ModelLoad[]>([]);

/** Reads the fields of a runtime event into a load event, `null` when it is not one. */
export function modelLoadEventOf(
  eventType: string,
  payload: Record<string, unknown>,
): ModelLoadEvent | null {
  const inner = payload[eventType];
  if (typeof inner !== "object" || inner === null) return null;
  const fields = inner as Record<string, unknown>;
  const text = (key: string) => (typeof fields[key] === "string" ? (fields[key] as string) : null);
  const backend = text("backend");
  if (backend === null) return null;
  if (eventType === "LlmModelLoading") {
    const model_path = text("model_path");
    return model_path === null ? null : { type: eventType, backend, model_path };
  }
  if (eventType === "LlmModelReady") {
    const model_id = text("model_id");
    return model_id === null ? null : { type: eventType, backend, model_id };
  }
  if (eventType === "LlmModelFailed") {
    const reason = text("reason");
    return reason === null ? null : { type: eventType, backend, reason };
  }
  return null;
}
