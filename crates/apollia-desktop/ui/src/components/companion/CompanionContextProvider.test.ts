import { vi, describe, test, expect, beforeAll, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { get } from "svelte/store";
import { locale, t, waitLocale } from "svelte-i18n";
import "$lib/i18n";
import { invoke } from "@tauri-apps/api/core";
import { companionStore } from "$lib/stores/companion";

const mockedInvoke = vi.mocked(invoke);

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

beforeEach(() => {
  companionStore.reset();
  vi.clearAllMocks();
});

// ── Route context propagation ─────────────────────────────────────────────────

describe("CompanionContextProvider - route context", () => {
  test("the context of a known route is the catalogue text of that screen", async () => {
    // GIVEN the interface in English
    // WHEN updateContext is called for the memory route
    await companionStore.updateContext({ route: "memory", subscreen: null, object: null });

    // THEN the store holds the catalogue text, and the shell was not asked
    expect(get(companionStore).currentContext).toContain(get(t)("guide.context.memory.text"));
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  test("a route no screen answers to leaves the guide without a screen text", async () => {
    // GIVEN a route outside the visible ones
    // WHEN updateContext is called with it
    await companionStore.updateContext({ route: "unknown-page", subscreen: null, object: null });

    // THEN no text is invented for it
    expect(get(companionStore).currentContext).toBe("");
    expect(get(companionStore).currentRoute).toBe("unknown-page");
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
    debouncedUpdate("agents");
    vi.advanceTimersByTime(499);
    expect(updates).toHaveLength(0);

    vi.advanceTimersByTime(1);
    expect(updates).toHaveLength(1);
    expect(updates[0]).toBe("agents");

    vi.useRealTimers();
  });
});
