import { describe, test, expect } from "vitest";
import { statusConfigForHealth } from "./ConnectionStatusIndicator.svelte";
import type { McpHealth } from "$lib/types";

describe("ConnectionStatusIndicator - statusConfigForHealth", () => {
  test("verified healthy is success + connected", () => {
    // GIVEN a verified-healthy server
    const health: McpHealth = { state: "healthy", verified: true };
    // WHEN mapping to a status config
    const config = statusConfigForHealth(health);
    // THEN the dot is success and the label is "connected"
    expect(config.dot).toBe("bg-success");
    expect(config.labelKey).toBe("integrations.status.connected");
  });

  test("unverified healthy keeps success dot but a distinct label", () => {
    // GIVEN a reachable-but-unexercised server
    const health: McpHealth = { state: "healthy", verified: false };
    // WHEN mapping
    const config = statusConfigForHealth(health);
    // THEN the dot stays success but the label is "unverified"
    expect(config.dot).toBe("bg-success");
    expect(config.labelKey).toBe("integrations.status.unverified");
  });

  test("degraded is warning", () => {
    // GIVEN a degraded server (e.g. Notion 404)
    const health: McpHealth = {
      state: "degraded",
      category: "tool_failure",
      last_error: "object_not_found",
      consecutive_failures: 1,
      since: "2026-06-02T00:00:00Z",
    };
    // WHEN mapping
    const config = statusConfigForHealth(health);
    // THEN the dot is amber/warning
    expect(config.dot).toBe("bg-warning");
    expect(config.labelKey).toBe("integrations.status.degraded");
  });

  test("needs_reauth is warning with its own label", () => {
    // GIVEN a server whose token expired
    const health: McpHealth = { state: "needs_reauth", reason: "unauthorized" };
    // WHEN mapping
    const config = statusConfigForHealth(health);
    // THEN amber dot and the reauth label
    expect(config.dot).toBe("bg-warning");
    expect(config.labelKey).toBe("integrations.status.needs_reauth");
  });

  test("unavailable is destructive", () => {
    // GIVEN a server whose process exited
    const health: McpHealth = { state: "unavailable", reason: "process_exited" };
    // WHEN mapping
    const config = statusConfigForHealth(health);
    // THEN red dot and the unavailable label
    expect(config.dot).toBe("bg-destructive");
    expect(config.labelKey).toBe("integrations.status.unavailable");
  });
});
