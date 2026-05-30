import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  disposeSessionMetricsListener,
  handleSessionMetricsUpdated,
  sessionMetricsFor,
  sessionMetricsSlots,
} from "./sessionMetricsV2";
import type { SessionMetrics, SessionMetricsUpdatedEvent } from "$lib/types";

const EMPTY_METRICS: SessionMetrics = {
  tokens_in: 0,
  tokens_out: 0,
  tokens_cached: 0,
  tokens_meta: 0,
  context_window_used: 0,
  context_window_max: 0,
  token_budget: 0,
  tool_timings: [],
  summarization_events: [],
};

function makeEvent(
  sessionId: string,
  overrides: Partial<SessionMetrics> = {},
): SessionMetricsUpdatedEvent {
  return {
    session_id: sessionId,
    metrics: { ...EMPTY_METRICS, ...overrides },
    alert: "ok",
  };
}

describe("sessionMetricsV2 store", () => {
  beforeEach(() => {
    disposeSessionMetricsListener();
  });

  it("returns an empty slot for unknown sessions", () => {
    const slot = sessionMetricsFor("unknown");
    expect(get(slot).metrics.tokens_in).toBe(0);
    expect(get(slot).alert).toBe("ok");
  });

  it("handleSessionMetricsUpdated indexes by session_id", () => {
    handleSessionMetricsUpdated(
      makeEvent("s1", { tokens_in: 100, tokens_out: 50 }),
    );
    handleSessionMetricsUpdated(
      makeEvent("s2", { tokens_in: 999 }),
    );

    const s1 = sessionMetricsFor("s1");
    const s2 = sessionMetricsFor("s2");

    expect(get(s1).metrics.tokens_in).toBe(100);
    expect(get(s1).metrics.tokens_out).toBe(50);
    expect(get(s2).metrics.tokens_in).toBe(999);
  });

  it("overwrites previous snapshot for the same session_id", () => {
    handleSessionMetricsUpdated(makeEvent("s", { tokens_in: 10 }));
    handleSessionMetricsUpdated(makeEvent("s", { tokens_in: 42 }));
    expect(get(sessionMetricsFor("s")).metrics.tokens_in).toBe(42);
  });

  it("surfaces alert level on the slot", () => {
    handleSessionMetricsUpdated({
      session_id: "s",
      metrics: { ...EMPTY_METRICS, tokens_in: 900, token_budget: 1000 },
      alert: "warning",
    });
    expect(get(sessionMetricsFor("s")).alert).toBe("warning");

    handleSessionMetricsUpdated({
      session_id: "s",
      metrics: { ...EMPTY_METRICS, tokens_in: 1100, token_budget: 1000 },
      alert: "block",
    });
    expect(get(sessionMetricsFor("s")).alert).toBe("block");
  });

  it("slots store exposes all known session_ids", () => {
    handleSessionMetricsUpdated(makeEvent("a"));
    handleSessionMetricsUpdated(makeEvent("b"));
    const snap = get(sessionMetricsSlots);
    expect(Object.keys(snap).sort((a, b) => a.localeCompare(b))).toEqual(["a", "b"]);
  });
});
