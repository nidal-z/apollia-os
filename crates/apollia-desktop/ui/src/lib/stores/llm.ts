/**
 * Derived LLM stores for Apollia Desktop.
 *
 * Re-exports the base llmBackends store from sse.ts and provides
 * derived stores for backend counts and status filtering.
 */
import { derived } from "svelte/store";
import { llmBackends } from "./sse";

export { llmBackends } from "./sse";

/** Number of known LLM backends. */
export const llmBackendCount = derived(
  llmBackends,
  ($backends) => $backends.length,
);

/** LLM backends that are enabled. */
export const readyLlmBackends = derived(llmBackends, ($backends) =>
  $backends.filter((b) => b.enabled),
);

/** LLM backends that are disabled. */
export const errorLlmBackends = derived(llmBackends, ($backends) =>
  $backends.filter((b) => !b.enabled),
);
