/**
 * Persisted state for the guided tour and the getting-started band.
 *
 * Deliberately frontend-only, on the same model as `lib/stores/mode.ts`. The
 * backend used to own a tour counter whose completion condition could never be
 * reached, and nothing read the flag it wrote; there is nothing to preserve
 * there.
 *
 * Four of the five milestones are derived from live stores and are never stored
 * here. Only `followVisited` is, because consulting the activity journal is an
 * act rather than an object, so no store can report it.
 */
import { writable, type Readable } from "svelte/store";
import type { TourId, TourRunState } from "./types";

const STORAGE_KEY = "apollia.tour.state";

/** Shape written to local storage. */
export interface PersistedTourState {
  /** Progress per tour. Absent entries mean `not_started`. */
  readonly tours: Partial<Record<TourId, TourRunState>>;
  /** True once the user has opened the activity and audit surface. */
  readonly followVisited: boolean;
  /** True once the band was dismissed, manually or by its closing sequence. */
  readonly bandDismissed: boolean;
}

const EMPTY: PersistedTourState = {
  tours: {},
  followVisited: false,
  bandDismissed: false,
};

function isRunState(value: unknown): value is TourRunState {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.index === "number" &&
    (candidate.status === "not_started" ||
      candidate.status === "in_progress" ||
      candidate.status === "done" ||
      candidate.status === "skipped")
  );
}

/**
 * Parses the stored payload, falling back to the empty state on anything
 * malformed. Exported so the tolerance can be tested without a DOM.
 */
export function parsePersistedState(raw: string | null): PersistedTourState {
  if (raw === null) return EMPTY;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return EMPTY;
    const candidate = parsed as Record<string, unknown>;

    const tours: Partial<Record<TourId, TourRunState>> = {};
    const rawTours = candidate.tours;
    if (typeof rawTours === "object" && rawTours !== null) {
      for (const [key, value] of Object.entries(rawTours)) {
        if (isRunState(value)) tours[key as TourId] = value;
      }
    }

    return {
      tours,
      followVisited: candidate.followVisited === true,
      bandDismissed: candidate.bandDismissed === true,
    };
  } catch {
    return EMPTY;
  }
}

function load(): PersistedTourState {
  try {
    return parsePersistedState(localStorage.getItem(STORAGE_KEY));
  } catch {
    // Storage unavailable (private mode, quota, embedded webview). The tour
    // still runs, in memory, for the current session.
    return EMPTY;
  }
}

const store = writable<PersistedTourState>(load());

store.subscribe((value) => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // See `load`: losing persistence degrades to a session-scoped tour, which
    // is not worth surfacing to the user.
  }
});

/** Reactive view of the persisted tour state. */
export const tourState: Readable<PersistedTourState> = { subscribe: store.subscribe };

/** Returns the stored progress of a tour, defaulting to `not_started`. */
export function runStateOf(state: PersistedTourState, id: TourId): TourRunState {
  return state.tours[id] ?? { status: "not_started", index: 0 };
}

/** Writes the progress of a single tour. */
export function setRunState(id: TourId, next: TourRunState): void {
  store.update((current) => ({
    ...current,
    tours: { ...current.tours, [id]: next },
  }));
}

/** Records that the user opened the activity and audit surface. */
export function markFollowVisited(): void {
  store.update((current) =>
    current.followVisited ? current : { ...current, followVisited: true },
  );
}

/** Hides the getting-started band, manually or after its closing sequence. */
export function dismissBand(): void {
  store.update((current) =>
    current.bandDismissed ? current : { ...current, bandDismissed: true },
  );
}

/** Brings the band back, used by the Help entry point. */
export function restoreBand(): void {
  store.update((current) => ({ ...current, bandDismissed: false }));
}

/** Clears everything. Exposed for the danger zone and for tests. */
export function resetTourState(): void {
  store.set(EMPTY);
}
