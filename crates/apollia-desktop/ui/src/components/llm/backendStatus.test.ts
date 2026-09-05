import { describe, test, expect } from "vitest";
import {
  backendStatus,
  backendStatusBadgeVariant,
  backendStatusDotColor,
  backendStatusLabelKey,
} from "./backendStatus";
import type { LlmBackendConfig } from "$lib/types";

function backend(partial: Partial<LlmBackendConfig> = {}): LlmBackendConfig {
  return {
    name: "local",
    provider: "llama-cpp",
    model: "qwen3-4b",
    config_json: {},
    enabled: true,
    is_default: true,
    ...partial,
  };
}

describe("backendStatus", () => {
  test("a backend nobody has pinged is untested, not running", () => {
    // GIVEN an enabled backend with no ping recorded in this app run
    const b = backend();

    // WHEN its status is derived
    // THEN it is untested: the absence of an error is not a measurement
    expect(backendStatus(b)).toBe("untested");
  });

  test("a backend that answered a ping is reachable", () => {
    // GIVEN a backend whose last ping came back without an error
    const b = backend({ last_ping_at: "2026-09-05T10:00:00Z", last_ping_error: null });

    // WHEN its status is derived
    // THEN it is reachable, and that claim rests on the recorded ping
    expect(backendStatus(b)).toBe("reachable");
  });

  test("a failed ping is an error whatever the timestamp says", () => {
    // GIVEN a backend whose last ping returned an error
    const b = backend({
      last_ping_at: "2026-09-05T10:00:00Z",
      last_ping_error: "connection refused",
    });

    // WHEN its status is derived
    // THEN it is in error
    expect(backendStatus(b)).toBe("error");
  });

  test("a disabled backend is in error, pinged or not", () => {
    // GIVEN a backend the operator turned off
    const b = backend({ enabled: false });

    // WHEN its status is derived
    // THEN it is in error, as the card has always shown it
    expect(backendStatus(b)).toBe("error");
  });
});

describe("backend status display tokens", () => {
  test("each status carries its own label, tone and dot", () => {
    // GIVEN the three statuses
    // WHEN their display tokens are read
    // THEN none of them shares a label, a badge tone or a dot colour with
    // another, so an untested backend can never read as a running one
    const labels = [
      backendStatusLabelKey("error"),
      backendStatusLabelKey("reachable"),
      backendStatusLabelKey("untested"),
    ];
    const variants = [
      backendStatusBadgeVariant("error"),
      backendStatusBadgeVariant("reachable"),
      backendStatusBadgeVariant("untested"),
    ];
    const dots = [
      backendStatusDotColor("error"),
      backendStatusDotColor("reachable"),
      backendStatusDotColor("untested"),
    ];
    expect(new Set(labels).size).toBe(3);
    expect(new Set(variants).size).toBe(3);
    expect(new Set(dots).size).toBe(3);
  });

  test("the untested label is the one the catalogue holds", () => {
    // GIVEN the untested status
    // WHEN its label key is read
    // THEN it points at the dedicated entry, not at the running one
    expect(backendStatusLabelKey("untested")).toBe("llm.status_untested");
    expect(backendStatusLabelKey("reachable")).toBe("llm.running");
  });
});
