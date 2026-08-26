// ─── LLM backends and cost statistics ───

/** Backend LLM configuré - vue CRUD retournée par `list_llm_backends`. */
export interface LlmBackendConfig {
  name: string;
  provider: "llama-cpp" | "openai" | "mistral" | "anthropic" | "ollama";
  model: string;
  config_json: Record<string, unknown>;
  enabled: boolean;
  is_default: boolean;
  /** Message d'erreur du dernier ping en RAM (absent = jamais pingé ou dernier OK). */
  last_ping_error?: string | null;
  /** Horodatage RFC 3339 du dernier ping (absent = jamais pingé). */
  last_ping_at?: string | null;
}

/** Résultat d'un ping LLM. */
export interface LlmPingResult {
  backend: string;
  available: boolean;
  latency_ms: number | null;
  error: string | null;
}

/** Ligne de statistiques coût/tokens. */
export interface LlmCostStatsRow {
  backend: string;
  model: string;
  call_count: number;
  total_tokens: number;
  total_cost_usd: number;
}

/** Réponse agrégée des statistiques coût/tokens. */
export interface LlmCostStatsResponse {
  rows: LlmCostStatsRow[];
  days: number;
}
