/**
 * Derived trigger stores for Apollia Desktop.
 *
 * Re-exports the base triggers store from sse.ts and provides
 * derived stores for filtered views.
 */
import { derived } from "svelte/store";
import { triggers } from "./sse";

export { triggers } from "./sse";

/** Number of enabled triggers. */
export const enabledTriggersCount = derived(
  triggers,
  ($triggers) => $triggers.filter((t) => t.enabled).length,
);

/** Total fire count across all triggers. */
export const totalFireCount = derived(triggers, ($triggers) =>
  $triggers.reduce((sum, t) => sum + t.fire_count, 0),
);
