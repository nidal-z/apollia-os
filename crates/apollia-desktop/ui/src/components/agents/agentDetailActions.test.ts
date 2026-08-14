import { describe, test, expect } from "vitest";
import { canStartAgent } from "./agentDetailActions";

// The dashboard sheet used to gate Start on `runtime_status === null`, so an
// agent stopped from the UI (status "stopped", non-null) lost the button until
// the app restarted. These tests pin the corrected predicate.

describe("AgentDetail - Start visibility on the dashboard sheet", () => {
  test("a stopped agent is offered Start", () => {
    // GIVEN an installed agent whose registry entry reads "stopped" after Stop
    const runtimeStatus = "stopped" as const;
    // WHEN the sheet evaluates the Start action
    const visible = canStartAgent(runtimeStatus, true, "/home/user/.apollia/agents/mailer");
    // THEN Start is offered, the agent is restartable without an app restart
    expect(visible).toBe(true);
  });

  test("a never-loaded agent keeps its Start button", () => {
    // GIVEN an installed agent with no runtime entry at all
    const runtimeStatus = null;
    // WHEN the sheet evaluates the Start action
    const visible = canStartAgent(runtimeStatus, true, "/home/user/.apollia/agents/mailer");
    // THEN the pre-existing behaviour is preserved
    expect(visible).toBe(true);
  });

  test("a running or transitioning agent is not offered Start", () => {
    // GIVEN an installed agent in each non-idle runtime state
    const nonIdle = ["active", "degraded", "initializing", "stopping"] as const;
    for (const runtimeStatus of nonIdle) {
      // WHEN the sheet evaluates the Start action
      const visible = canStartAgent(runtimeStatus, true, "/home/user/.apollia/agents/mailer");
      // THEN Start stays hidden
      expect(visible).toBe(false);
    }
  });

  test("a stopped agent without an install path cannot be started", () => {
    // GIVEN a stopped runtime-only agent (no install path to start from)
    const runtimeStatus = "stopped" as const;
    // WHEN the sheet evaluates the Start action
    const visible = canStartAgent(runtimeStatus, false, null);
    // THEN Start stays hidden, there is nothing to invoke start_agent with
    expect(visible).toBe(false);
  });
});
