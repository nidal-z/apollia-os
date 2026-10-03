// ─── LLM backends and cost statistics ───

/** Configured LLM backend - CRUD view returned by `list_llm_backends`. */
export interface LlmBackendConfig {
  name: string;
  /** `openai` is any OpenAI-compatible server, at the address the backend designates. */
  provider: "llama-cpp" | "openai" | "ollama";
  model: string;
  config_json: Record<string, unknown>;
  enabled: boolean;
  is_default: boolean;
  /** Error message of the last in-memory ping (absent = never pinged, or last one OK). */
  last_ping_error?: string | null;
  /** RFC 3339 timestamp of the last ping (absent = never pinged). */
  last_ping_at?: string | null;
}

/** One local model resident in memory, as `list_loaded_models` returns it. */
export interface LoadedModel {
  /** The engine serving the model, chosen by its format. */
  engine: "llama-server" | "vllm-mlx";
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
}

/** Resident local models and the engine that holds them. */
export interface LoadedModelsResponse {
  /** `false` means no local engine on this host, not merely nothing loaded. */
  engineAvailable: boolean;
  maxLoaded: number;
  /** Most recently used first. */
  models: LoadedModel[];
}

/** Outcome of `unload_loaded_model`. */
export interface UnloadedModel {
  modelPath: string;
  stopped: number;
}

/** Result of an LLM ping. */
export interface LlmPingResult {
  backend: string;
  available: boolean;
  latency_ms: number | null;
  error: string | null;
}

/** One row of cost and token statistics. */
export interface LlmCostStatsRow {
  backend: string;
  model: string;
  call_count: number;
  total_tokens: number;
  total_cost_usd: number;
}

/** Aggregated response of the cost and token statistics. */
export interface LlmCostStatsResponse {
  rows: LlmCostStatsRow[];
  days: number;
}
