/**
 * Tick store guarantees.
 *
 * Even with many subscribers (each inbox card subscribes on mount), the
 * underlying setInterval must be instantiated exactly once — and cleared
 * once the last subscriber leaves.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { get } from "svelte/store";

describe("inbox tick store", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.resetModules();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("instantiates a single setInterval no matter how many subscribers", async () => {
    const setSpy = vi.spyOn(globalThis, "setInterval");
    const clearSpy = vi.spyOn(globalThis, "clearInterval");

    const { tickStore } = await import("./timer");

    // 100 subscribers — simulates 100 inbox cards.
    const unsubs: Array<() => void> = [];
    for (let i = 0; i < 100; i++) {
      unsubs.push(tickStore.subscribe(() => {}));
    }

    expect(setSpy).toHaveBeenCalledTimes(1);
    expect(typeof get(tickStore)).toBe("number");

    // Releasing all but one subscriber must not clear the interval.
    for (let i = 0; i < 99; i++) unsubs[i]();
    expect(clearSpy).not.toHaveBeenCalled();

    // Releasing the last one clears it.
    unsubs[99]();
    expect(clearSpy).toHaveBeenCalledTimes(1);
  });

  it("pushes tick updates to subscribers on the configured interval", async () => {
    const { tickStore, _tickIntervalMs } = await import("./timer");
    const received: number[] = [];
    const unsub = tickStore.subscribe((v) => received.push(v));

    const before = received.length;
    vi.advanceTimersByTime(_tickIntervalMs * 3);
    expect(received.length - before).toBeGreaterThanOrEqual(3);

    unsub();
  });
});

describe("reject reason validation", () => {
  it("blocks confirmation under 5 characters", async () => {
    const { MIN_REJECT_REASON } = await import("../../../components/inbox/types");
    const short = "abcd";
    const ok = "valid reason";
    expect(short.trim().length >= MIN_REJECT_REASON).toBe(false);
    expect(ok.trim().length >= MIN_REJECT_REASON).toBe(true);
  });
});
