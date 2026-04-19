/**
 * LLM pricing table used by SessionMetricsPanel (US-SP42-030).
 *
 * Prices are expressed in USD per million tokens. Figures are best-effort
 * snapshots — the backend is the source of truth when `cost_usd` is reported
 * (Anthropic, OpenAI). This table is only used when the backend does not
 * return a cost, to produce a rough client-side estimate.
 */

export interface ModelPricing {
  /** Canonical model identifier (backend:model). */
  key: string;
  /** Human label for the metrics panel. */
  label: string;
  /** USD per 1M input tokens. */
  inputPerMillion: number;
  /** USD per 1M output tokens. */
  outputPerMillion: number;
}

export const PRICING_TABLE: ModelPricing[] = [
  // Anthropic Claude
  { key: "anthropic:claude-opus-4-7", label: "Claude Opus 4.7", inputPerMillion: 15, outputPerMillion: 75 },
  { key: "anthropic:claude-sonnet-4-6", label: "Claude Sonnet 4.6", inputPerMillion: 3, outputPerMillion: 15 },
  { key: "anthropic:claude-haiku-4-5", label: "Claude Haiku 4.5", inputPerMillion: 0.8, outputPerMillion: 4 },
  // OpenAI
  { key: "openai:gpt-4o", label: "GPT-4o", inputPerMillion: 2.5, outputPerMillion: 10 },
  { key: "openai:gpt-4o-mini", label: "GPT-4o mini", inputPerMillion: 0.15, outputPerMillion: 0.6 },
  // Local backends — zero-cost.
  { key: "local", label: "Local (llama.cpp)", inputPerMillion: 0, outputPerMillion: 0 },
  { key: "ollama", label: "Ollama", inputPerMillion: 0, outputPerMillion: 0 },
];

/**
 * Resolve pricing for a backend/model string returned by the runtime.
 *
 * Falls back to a zero-cost entry when no match is found — callers should
 * treat the result as an estimate, not a billable amount.
 */
export function resolvePricing(backend: string | null | undefined): ModelPricing {
  if (!backend) {
    return { key: "unknown", label: "Unknown", inputPerMillion: 0, outputPerMillion: 0 };
  }
  const match = PRICING_TABLE.find((p) => backend.toLowerCase().includes(p.key.split(":")[1] ?? p.key));
  return match ?? { key: backend, label: backend, inputPerMillion: 0, outputPerMillion: 0 };
}

/**
 * Compute an estimated cost (USD) for a given token count with a model's pricing.
 */
export function estimateCost(
  pricing: ModelPricing,
  promptTokens: number,
  completionTokens: number,
): number {
  const inputCost = (promptTokens / 1_000_000) * pricing.inputPerMillion;
  const outputCost = (completionTokens / 1_000_000) * pricing.outputPerMillion;
  return inputCost + outputCost;
}

/**
 * Approximate USD → tokens-equivalent conversion for display purposes.
 *
 * Uses the model's input rate so the number represents "tokens you could have
 * afforded at input price" — a rough comparison metric only.
 */
export function costToTokensEquivalent(costUsd: number, pricing: ModelPricing): number {
  if (pricing.inputPerMillion === 0) return 0;
  return Math.round((costUsd / pricing.inputPerMillion) * 1_000_000);
}
