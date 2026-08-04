import { describe, test, expect, vi, beforeEach } from "vitest";
import { writable } from "svelte/store";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const currentSession = writable<{ id: string } | null>(null);
vi.mock("$lib/stores/chat", () => ({ currentSession }));

import {
  refreshActiveSessionMetrics,
  refreshSessionMetrics,
} from "./chatMetrics";

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue({ session_id: "sess-1" });
});

describe("refreshActiveSessionMetrics", () => {
  test("refreshes the currently viewed session, bypassing the throttle", async () => {
    // GIVEN a session open in the chat view, freshly refreshed (throttled)
    currentSession.set({ id: "sess-1" });
    await refreshSessionMetrics("sess-1", true);
    invokeMock.mockClear();

    // WHEN a ContextCompacted-style refresh fires within the throttle window
    await refreshActiveSessionMetrics();

    // THEN chat_session_metrics is invoked again for that session (forced)
    expect(invokeMock).toHaveBeenCalledWith("chat_session_metrics", {
      sessionId: "sess-1",
    });
  });

  test("is a no-op when no session is open", async () => {
    // GIVEN no conversation on screen
    currentSession.set(null);

    // WHEN the refresh fires
    await refreshActiveSessionMetrics();

    // THEN no IPC round-trip happens
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
