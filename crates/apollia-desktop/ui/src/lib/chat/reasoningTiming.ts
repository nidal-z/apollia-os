/**
 * How long each reasoning fragment of a turn took, measured while it streams.
 *
 * A stored answer carries its reasoning text but no duration for it, so the
 * duration a block is titled with is measured live: a fragment starts when it
 * first appears in the stream and ends when it closes. The measure is kept by
 * the fragment's text, which is also what the stored answer carries, so the
 * finished turn finds the duration its live rendering measured. A conversation
 * reopened after a restart has no measure and its blocks go untitled by time.
 */

/** One reasoning fragment as the stream parser sees it. */
export interface StreamFragment {
  content: string;
  closed: boolean;
}

/** Durations kept at most, the oldest dropped first. */
export const REMEMBERED_MAX = 200;

const remembered = new Map<string, number>();

function keyOf(content: string): string {
  return content.trim();
}

/** Keep the measured duration of a closed fragment. */
export function rememberReasoningDuration(content: string, ms: number): void {
  const key = keyOf(content);
  if (!key || !Number.isFinite(ms) || ms < 0) return;
  remembered.delete(key);
  remembered.set(key, ms);
  while (remembered.size > REMEMBERED_MAX) {
    const oldest = remembered.keys().next().value;
    if (oldest === undefined) break;
    remembered.delete(oldest);
  }
}

/** The measured duration of a fragment, or `null` when none was measured. */
export function reasoningDurationFor(content: string): number | null {
  return remembered.get(keyOf(content)) ?? null;
}

/** Forget every measure. For tests. */
export function forgetReasoningDurations(): void {
  remembered.clear();
}

/**
 * A clock for one streaming turn. `observe` is handed the turn's reasoning
 * fragments, in order, each time the stream grows; it returns the duration of
 * every closed fragment (by index, `null` while open) and remembers it.
 */
export function createReasoningClock(now: () => number = () => Date.now()): {
  observe(fragments: StreamFragment[]): (number | null)[];
} {
  const starts: number[] = [];
  const ends: number[] = [];
  return {
    observe(fragments) {
      const at = now();
      return fragments.map((fragment, i) => {
        if (starts[i] === undefined) starts[i] = at;
        if (!fragment.closed) return null;
        if (ends[i] === undefined) {
          ends[i] = at;
          rememberReasoningDuration(fragment.content, ends[i] - starts[i]);
        }
        return ends[i] - starts[i];
      });
    },
  };
}
