// Typed IPC wrapper for the global observability timeline.
//
// `getGlobalTimeline` reads the merged, cross-agent event stream the runtime
// exposes for the Observability surface. Keeping the call here removes direct
// `invoke` usage from the `.svelte` file.

import { invoke } from "@tauri-apps/api/core";
import type { GlobalTimelineEvent } from "$lib/types";

/**
 * Reads the global timeline for a trailing window.
 *
 * @param windowMinutes trailing window size, in minutes.
 */
export async function getGlobalTimeline(
  windowMinutes: number,
): Promise<GlobalTimelineEvent[]> {
  return invoke<GlobalTimelineEvent[]>("get_global_timeline", {
    params: { window_minutes: windowMinutes },
  });
}
