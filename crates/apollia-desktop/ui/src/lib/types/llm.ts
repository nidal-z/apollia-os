// ─── LLM backends and cost statistics ───

/** Configured LLM backend, as `list_llm_backends` returns it. */
export interface LlmBackendConfig {
  name: string;
  /** `openai` is any OpenAI-compatible server, at the address the backend designates. */
  provider: "llama-cpp" | "openai" | "ollama";
  model: string;
  config_json: Record<string, unknown>;
  enabled: boolean;
  is_default: boolean;
}

/** One local model resident in memory, as `list_loaded_models` returns it. */
export interface LoadedModel {
  /** The engine serving the model, chosen by its format. */
  engine: "llama-server" | "vllm-mlx";
  /** What the model serves; the embeddings instance sits outside `maxLoaded`. */
  kind: "chat" | "embeddings";
  /** Display name, the model file name without its extension. */
  model: string;
  /** Path the model was loaded from; the key an unload takes. */
  modelPath: string;
  port: number;
  nCtx: number;
  nGpuLayers: number;
  /** Absent when the engine default applies. */
  nParallel?: number | null;
  /** RFC 3339. */
  loadedAt: string;
  /** RFC 3339. */
  lastUsedAt: string;
  /** Requests being answered right now; an unload is refused above zero. */
  activeRequests: number;
  /** Memory the load guardrail estimated at launch, in GB; null when not estimated. */
  footprintGb?: number | null;
  /** Identifier of the model in the model index; null for a file it does not hold. */
  modelId?: string | null;
  /** Size of the weights on disk, from the model index. */
  sizeBytes?: number | null;
  /** What the model can do, from the model index. */
  capabilities?: ModelCapabilities | null;
}

/** What a model can do, as the model index computed it. */
export interface ModelCapabilities {
  tools: boolean;
  reasoning: boolean;
  vision: boolean;
}

/** Resident local models and the engine that holds them. */
export interface LoadedModelsResponse {
  /** `false` means no local engine on this host, not merely nothing loaded. */
  engineAvailable: boolean;
  /** Explicit ceiling on resident models; `null` when the load guardrail decides. */
  maxLoaded: number | null;
  /** Most recently used first. */
  models: LoadedModel[];
}

/** Outcome of `unload_loaded_model`. */
export interface UnloadedModel {
  modelPath: string;
  stopped: number;
}
