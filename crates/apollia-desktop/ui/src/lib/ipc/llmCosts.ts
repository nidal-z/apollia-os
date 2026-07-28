// Typed IPC wrapper for the LLM daily cost breakdown.
//
// `getLlmDailyCosts` returns per-day, per-backend spend over a trailing window.
// Keeping the call here removes direct `invoke` usage from the `.svelte` file.

import { invoke } from "@tauri-apps/api/core";
import type { LlmDailyCostsResponse } from "$lib/types";

/**
 * Reads daily LLM costs for a trailing window.
 *
 * @param days trailing window size, in days.
 */
export async function getLlmDailyCosts(days: number): Promise<LlmDailyCostsResponse> {
  return invoke<LlmDailyCostsResponse>("get_llm_daily_costs", { days });
}
