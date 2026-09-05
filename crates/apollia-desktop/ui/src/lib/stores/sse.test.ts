import { vi, describe, test, expect, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(false),
  requestPermission: vi.fn().mockResolvedValue("denied"),
  sendNotification: vi.fn(),
}));

import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import {
  connectionStatus,
  createSSEConnection,
  refreshAll,
  statusFromRefresh,
} from "./sse";

const mockedInvoke = vi.mocked(invoke);

/** Let every pending microtask settle (the hydration sweep and its `.then`). */
async function flush(): Promise<void> {
  for (let i = 0; i < 10; i += 1) {
    await Promise.resolve();
  }
}

beforeEach(() => {
  vi.clearAllMocks();
  connectionStatus.set("connecting");
});

afterEach(() => {
  vi.useRealTimers();
});

describe("refreshAll", () => {
  test("counts the round-trips that actually answered", async () => {
    // GIVEN a runtime that answers every list command
    mockedInvoke.mockResolvedValue([]);

    // WHEN the hydration sweep runs
    const outcome = await refreshAll();

    // THEN every attempt is counted as a success
    expect(outcome.attempted).toBeGreaterThan(0);
    expect(outcome.succeeded).toBe(outcome.attempted);
  });

  test("reports zero successes when the runtime answers nothing", async () => {
    // GIVEN a runtime that refuses every list command
    mockedInvoke.mockRejectedValue(new Error("runtime unavailable"));

    // WHEN the hydration sweep runs
    const outcome = await refreshAll();

    // THEN the sweep still resolves, but nothing was measured
    expect(outcome.attempted).toBeGreaterThan(0);
    expect(outcome.succeeded).toBe(0);
  });
});

describe("statusFromRefresh", () => {
  test("one answered round-trip is enough to call the bridge connected", () => {
    // GIVEN a sweep where at least one command answered
    const outcome = { attempted: 6, succeeded: 1 };

    // WHEN the status is derived
    // THEN the bridge is connected
    expect(statusFromRefresh(outcome)).toBe("connected");
  });

  test("a sweep that answered nothing is not a connection", () => {
    // GIVEN a sweep where every command failed
    const outcome = { attempted: 6, succeeded: 0 };

    // WHEN the status is derived
    // THEN the store stays on the pre-connection state
    expect(statusFromRefresh(outcome)).toBe("connecting");
  });
});

describe("createSSEConnection", () => {
  test("does not announce a connection when the runtime answered nothing", async () => {
    // GIVEN a runtime that refuses every IPC call
    mockedInvoke.mockRejectedValue(new Error("runtime unavailable"));

    // WHEN the bridge is started and its hydration sweep completes
    const stop = createSSEConnection();
    await flush();
    const status = get(connectionStatus);
    stop();

    // THEN the status never claims "connected"
    expect(status).not.toBe("connected");
  });

  test("announces a connection once a round-trip answered", async () => {
    // GIVEN a runtime that answers
    mockedInvoke.mockResolvedValue([]);

    // WHEN the bridge is started and its hydration sweep completes
    const stop = createSSEConnection();
    await flush();
    const status = get(connectionStatus);
    stop();

    // THEN the status is connected, measured rather than assumed
    expect(status).toBe("connected");
  });
});
