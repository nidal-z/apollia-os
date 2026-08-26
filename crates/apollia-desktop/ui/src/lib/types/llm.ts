// ─── LLM backends and cost statistics ───

/** Configured LLM backend - CRUD view returned by `list_llm_backends`. */
export interface LlmBackendConfig {
  name: string;
  provider: "llama-cpp" | "openai" | "mistral" | "anthropic" | "ollama";
  model: string;
  config_json: Record<string, unknown>;
  enabled: boolean;
  is_default: boolean;
  /** Error message of the last in-memory ping (absent = never pinged, or last one OK). */
  last_ping_error?: string | null;
  /** RFC 3339 timestamp of the last ping (absent = never pinged). */
  last_ping_at?: string | null;
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
