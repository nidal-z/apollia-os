import { describe, it, expect } from "vitest";
import { webhookEndpoint } from "./webhookEndpoint";

describe("webhookEndpoint", () => {
  it("carries the resolved port of the native API", () => {
    // GIVEN a native API moved to port 8123 by `[api].port`
    const apiPort = 8123;

    // WHEN the endpoint of a webhook trigger is built
    const url = webhookEndpoint(apiPort, "deploy-hook");

    // THEN it points at that port, not at the default
    expect(url).toBe("http://localhost:8123/api/v1/webhooks/deploy-hook");
  });
});
