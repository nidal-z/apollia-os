// Typed IPC wrappers for the ORIA plan cache surface.
//
// `getPlanCacheStats` reads hit/miss counters; `clearPlanCache` purges every
// cached plan. Keeping the calls here removes direct `invoke` usage from the
// `.svelte` file.

import { invoke } from "@tauri-apps/api/core";
import type { PlanCacheStats } from "$lib/types";

/** Reads the current plan cache statistics. */
export async function getPlanCacheStats(): Promise<PlanCacheStats> {
  return invoke<PlanCacheStats>("get_plan_cache_stats");
}

/** Purges every entry from the plan cache. */
export async function clearPlanCache(): Promise<void> {
  await invoke("clear_plan_cache");
}
