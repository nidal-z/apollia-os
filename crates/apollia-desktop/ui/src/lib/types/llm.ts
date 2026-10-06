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

/** What the loading window asks: the model and the settings of this load. */
export interface ModelLoadRequest {
  /** Identifier of the model in the model index. */
  modelId: string;
  /** Context window in tokens; absent for the one the model's defaults give. */
  nCtx?: number;
  /** Layers placed on the accelerator; absent for the planned count. */
  nGpuLayers?: number;
  /** Advanced load settings, by the keys of a preset's load settings. */
  advanced?: Record<string, unknown>;
  /** Write these settings to the model's default preset. */
  remember?: boolean;
  /** The user confirmed a load the guardrail refuses. */
  confirmed?: boolean;
}

/** How strictly a load is held to the memory of the machine. */
export type GuardrailLevel = "disabled" | "lenient" | "balanced" | "strict" | "ceiling";

/** What loading a model under given settings would take, as `estimate_model_load` returns it. */
export interface LoadEstimate {
  modelId: string;
  engine: "llama-server" | "vllm-mlx";
  /** The file or folder that would be loaded; the key of a pin and of a lease. */
  modelPath: string;
  /** Whether the load takes settings; `false` for an MLX model. */
  configurable: boolean;
  /** Context window the model would be loaded with. */
  nCtx: number;
  /** Layers it would place on the accelerator; absent for an MLX model. */
  nGpuLayers?: number;
  /** Layers the model has; absent when unknown. */
  totalLayers?: number;
  /** The largest window the model accepts; absent when its header does not say. */
  maxCtx?: number;
  /** The quantisation the file name declares; absent when it declares none. */
  quantization?: string;
  /** The memory it would take; absent when the model cannot be sized. */
  memory?: {
    gpuGb: number;
    totalGb: number;
    capacityGb: number;
    fit: "fits" | "might_fit" | "too_large";
    kvCacheMeasured: boolean;
  };
  /** What the load guardrail makes of it. */
  guardrail: {
    level: GuardrailLevel;
    verdict: "allowed" | "warned" | "refused";
    /** The limit the level holds to, in GB; absent when nothing is checked. */
    budgetGb?: number;
  };
}

/** Outcome of `load_model` and of `reload_last_loaded_model`. */
export interface ModelLoadOutcome {
  modelId: string;
  loaded: LoadedModel;
  /** The preset the settings were written to, when asked to remember them. */
  rememberedPresetId?: number;
}

/** The last model loaded by hand, as `get_last_loaded_model` returns it. */
export interface LastLoadedModel {
  modelId: string;
  /** The settings it was loaded with, by the keys of a preset's load settings. */
  load: Record<string, unknown>;
  /** RFC 3339. */
  loadedAt: string;
}

/** Outcome of `open_model_lease`. */
export interface ModelLease {
  modelPath: string;
  /** Leases now held on the model. */
  leases: number;
}

/** Outcome of `release_model_lease`. */
export interface ModelLeaseRelease {
  /** The model the lease was on; absent when none was open. */
  released?: string;
  /** Whether releasing it unloaded the model. */
  unloaded: boolean;
}

/** The models kept in memory, as `list_model_pins` returns them. */
export interface ModelPins {
  pins: string[];
}

/** Outcome of `set_model_pin`. */
export interface ModelPin {
  modelPath: string;
  pinned: boolean;
}
