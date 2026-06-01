import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { companionStore } from "$lib/stores/companion";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  companionStore.reset();
  vi.clearAllMocks();
});

// ── Route context propagation ─────────────────────────────────────────────────

describe("CompanionContextProvider - route context", () => {
  test("context text for a known route is stored in companionStore", async () => {
    // GIVEN the backend returns a specific text for /memory
    const memoryContext =
      "Vous êtes sur la page Mémoire. Apollia offre 3 types de mémoire.";
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_companion_context")
        return Promise.resolve(memoryContext);
      return Promise.resolve(null);
    });

    // WHEN updateContext is called for the memory route
    await companionStore.updateContext("memory");

    // THEN the store reflects the returned context
    expect(get(companionStore).currentContext).toBe(memoryContext);
  });

  test("unknown route receives a generic fallback context from the backend", async () => {
    // GIVEN the backend returns a generic fallback for an unknown route
    const fallback =
      "Je suis votre assistant Apollia. Posez-moi n'importe quelle question.";
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_companion_context") return Promise.resolve(fallback);
      return Promise.resolve(null);
    });

    // WHEN updateContext is called with an unknown route
    await companionStore.updateContext("unknown-page");

    // THEN the fallback context is stored
    expect(get(companionStore).currentContext).toBe(fallback);
  });
});

// ── Debounce behaviour ────────────────────────────────────────────────────────

/**
 * Mirrors the debounce logic in CompanionContextProvider.svelte.
 * The provider waits 500 ms after the last route change before calling
 * updateContext. This function simulates the same behaviour in a testable
 * form with a configurable delay.
 */
function createDebouncedContextUpdater(
  onUpdate: (route: string) => void,
  delayMs: number,
): (route: string) => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return (route: string) => {
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => onUpdate(route), delayMs);
  };
}

describe("CompanionContextProvider - debounce", () => {
  test("rapid route changes result in only the last route being processed", () => {
    // GIVEN a debounced updater with fake timers
    vi.useFakeTimers();
    const updates: string[] = [];
    const debouncedUpdate = createDebouncedContextUpdater(
      (r) => updates.push(r),
      500,
    );

    // WHEN two routes are dispatched in quick succession
    debouncedUpdate("agents");
    debouncedUpdate("tasks");

    // Timer has not yet fired
    expect(updates).toHaveLength(0);

    // WHEN 500 ms elapse
    vi.advanceTimersByTime(500);

    // THEN only the last route was processed
    expect(updates).toHaveLength(1);
    expect(updates[0]).toBe("tasks");

    vi.useRealTimers();
  });

  test("a single route change fires after the debounce delay", () => {
    // GIVEN
    vi.useFakeTimers();
    const updates: string[] = [];
    const debouncedUpdate = createDebouncedContextUpdater(
      (r) => updates.push(r),
      500,
    );

    // WHEN
    debouncedUpdate("dashboard");
    vi.advanceTimersByTime(499);
    expect(updates).toHaveLength(0);

    vi.advanceTimersByTime(1);
    expect(updates).toHaveLength(1);
    expect(updates[0]).toBe("dashboard");

    vi.useRealTimers();
  });
});
