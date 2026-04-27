/**
 * Message-list virtualization helpers (perf target ≥ 55 fps).
 *
 * Wraps `@tanstack/svelte-virtual` behind a small API so `ChatConversation`
 * can opt into windowing when the message count exceeds
 * `VIRTUALIZATION_THRESHOLD`.  Below the threshold we keep the plain
 * `{#each}` render path — virtualization imposes a constant overhead per
 * item that is counter-productive for short histories.
 */

/** Flip to virtualized rendering above this count. */
export const VIRTUALIZATION_THRESHOLD = 200;

/** Default estimated row height in pixels (tuned to typical message group). */
export const DEFAULT_ROW_ESTIMATE = 120;

export function shouldVirtualize(count: number): boolean {
  return count > VIRTUALIZATION_THRESHOLD;
}

export interface VirtualizerOptions {
  /** Total number of rows. */
  count: number;
  /** Estimated row height in pixels. */
  estimateSize?: number;
  /** Extra rows to render outside the viewport for smoother scrolling. */
  overscan?: number;
}

/**
 * Lazy import of `@tanstack/svelte-virtual` to keep it out of the initial
 * bundle.  Consumers call this from an `$effect` once the row count crosses
 * the threshold.
 */
export async function loadVirtualizer() {
  const module = await import("@tanstack/svelte-virtual");
  return module;
}
