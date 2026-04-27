/**
 * Generic guided-tour framework.
 *
 * Provides a lightweight registry + controller so feature surfaces can declare
 * named tours (welcome chat, first automation, …) and drive them through the
 * existing `TourSpotlight` + `TourStepCard` primitives. Completion is persisted
 * per-tour in `localStorage` using the same pattern as
 * {@link ../tour/FirstConnectionTour.ts}.
 *
 * Story:.
 */
import { writable, type Writable } from "svelte/store";

/** Descriptor for a single step in a generic tour. */
export interface TourStep {
  /** Stable step identifier, unique within a tour. */
  id: string;
  /** CSS selector of the element to highlight, or `null` for a centred card. */
  targetSelector: string | null;
  /** i18n key for the step title. */
  titleKey: string;
  /** i18n key for the step body. */
  bodyKey: string;
  /** Optional placement hint consumed by the card positioning helper. */
  placement?: "top" | "bottom" | "left" | "right";
}

/** A named ordered sequence of steps. */
export interface TourDefinition {
  /** Stable tour identifier (used in the localStorage key). */
  id: string;
  /** Ordered list of steps. */
  steps: TourStep[];
}

/** Controller returned by {@link triggerTour} to drive the tour from a Svelte component. */
export interface TourController {
  /** Tour identifier. */
  id: string;
  /** Resolved step definitions. */
  steps: TourStep[];
  /** Current zero-based step index (reactive). */
  current: Writable<number>;
  /** Advance to the next step or finish if we're on the last one. */
  next(): void;
  /** Move to the previous step (no-op at index 0). */
  prev(): void;
  /** Skip the tour and mark it seen. */
  skip(): void;
  /** Finish the tour and mark it seen. */
  finish(): void;
  /** Subscribe to end-of-tour events (skip or finish). Returns an unsubscribe fn. */
  onEnd(listener: (reason: "finish" | "skip") => void): () => void;
}

/** Prefix shared with FirstConnectionTour so both use the same storage namespace. */
const STORAGE_PREFIX = "apollia.user.onboarding_tours.";

/** Module-level registry of tour definitions, keyed by id. */
const registry = new Map<string, TourDefinition>();

/** Build the localStorage key used to persist completion state. */
function storageKey(id: string): string {
  return `${STORAGE_PREFIX}${id}.done`;
}

/** Register (or replace) a tour definition. */
export function registerTour(def: TourDefinition): void {
  registry.set(def.id, def);
}

/** Retrieve a registered tour, or `undefined` if unknown. */
export function getTour(id: string): TourDefinition | undefined {
  return registry.get(id);
}

/** True if the tour has already been completed or skipped. */
export function isTourSeen(id: string): boolean {
  try {
    return localStorage.getItem(storageKey(id)) === "true";
  } catch {
    // SSR or storage-disabled contexts: treat as unseen.
    return false;
  }
}

/** Mark the tour as seen so it does not re-launch. */
export function markTourSeen(id: string): void {
  try {
    localStorage.setItem(storageKey(id), "true");
  } catch {
    // No-op when storage is unavailable.
  }
}

/** Reset the seen flag — exposed for settings/debug affordances. */
export function resetTour(id: string): void {
  try {
    localStorage.removeItem(storageKey(id));
  } catch {
    // No-op.
  }
}

/**
 * Instantiate a controller for a registered tour.
 *
 * Throws if the tour id is unknown — callers are expected to register their
 * tours at module-load time (see `tourRegistry.ts`).
 */
export function triggerTour(id: string): TourController {
  const def = registry.get(id);
  if (def === undefined) {
    throw new Error(`[GuidedTour] unknown tour id: ${id}`);
  }

  const current = writable(0);
  const endListeners = new Set<(reason: "finish" | "skip") => void>();
  let ended = false;

  function endWith(reason: "finish" | "skip"): void {
    if (ended) return;
    ended = true;
    markTourSeen(id);
    for (const l of endListeners) l(reason);
  }

  return {
    id: def.id,
    steps: def.steps,
    current,
    next(): void {
      current.update((i) => {
        if (i >= def.steps.length - 1) {
          endWith("finish");
          return i;
        }
        return i + 1;
      });
    },
    prev(): void {
      current.update((i) => (i > 0 ? i - 1 : 0));
    },
    skip(): void {
      endWith("skip");
    },
    finish(): void {
      endWith("finish");
    },
    onEnd(listener): () => void {
      endListeners.add(listener);
      return () => endListeners.delete(listener);
    },
  };
}
