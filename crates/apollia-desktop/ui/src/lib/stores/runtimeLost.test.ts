import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { get } from "svelte/store";

const handlers = new Map<string, () => void>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: () => void) => {
    handlers.set(name, handler);
    return () => handlers.delete(name);
  }),
}));
vi.mock("./sse", () => ({ refreshAll: vi.fn(async () => {}) }));

import { runtimeHealth, startRuntimeHealthMonitor } from "./runtimeHealth";

/**
 * The attached desktop asks the runtime whether it is alive and emits
 * `runtime:lost` on the first unanswered probe. The banner has to follow at
 * once, not after the fifteen-second heartbeat grace window.
 */
describe("runtime:lost", () => {
  let stop: () => void = () => {};

  beforeEach(() => {
    vi.useFakeTimers();
    handlers.clear();
  });

  afterEach(() => {
    stop();
    vi.useRealTimers();
  });

  test("marks the runtime disconnected without waiting for the watchdog", async () => {
    // GIVEN a monitor that has seen a heartbeat
    stop = startRuntimeHealthMonitor();
    await vi.waitFor(() => expect(handlers.has("runtime:lost")).toBe(true));
    handlers.get("runtime:heartbeat")?.();
    expect(get(runtimeHealth).status).toBe("connected");

    // WHEN the backend reports the runtime lost, one second later
    vi.advanceTimersByTime(1_000);
    handlers.get("runtime:lost")?.();

    // THEN the store leaves the connected state immediately
    expect(get(runtimeHealth).status).not.toBe("connected");
  });

  test("a heartbeat after the loss reconnects", async () => {
    // GIVEN a runtime reported lost
    stop = startRuntimeHealthMonitor();
    await vi.waitFor(() => expect(handlers.has("runtime:lost")).toBe(true));
    handlers.get("runtime:lost")?.();
    expect(get(runtimeHealth).status).not.toBe("connected");

    // WHEN the probe is answered again
    handlers.get("runtime:heartbeat")?.();

    // THEN the store is connected
    expect(get(runtimeHealth).status).toBe("connected");
  });
});
