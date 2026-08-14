/**
 * Component tests for the execution section of AgentSettingsTab.
 *
 * Covers the Start button visibility predicate the template renders. The
 * regression under test: after Stop, the registry keeps the agent loaded with
 * `runtime_status: "stopped"`, and a predicate that reads "never loaded" as
 * the only idle shape hides Start until the app restarts.
 */
import { describe, it, expect } from "vitest";
import { showStartButton } from "./startVisibility";
import type { AgentListItem } from "$lib/types";

function agent(overrides: Partial<AgentListItem> = {}): AgentListItem {
  return {
    id: "3e9c1d2a-0000-4000-8000-000000000000",
    name: "mail-triage",
    version: "1.0.0",
    enabled: false,
    runtime_status: null,
    installed_at: "2026-08-01T10:00:00Z",
    description: null,
    tags: [],
    tools_required: [],
    tools_optional: [],
    execution_mode: null,
    install_path: "/home/user/.apollia/agents/mail-triage",
    supports_a2a: false,
    skills: [],
    agent_type: null,
    examples: [],
    ...overrides,
  } as AgentListItem;
}

describe("showStartButton", () => {
  it("offers Start on a stopped agent", () => {
    // GIVEN an installed agent the user just stopped: the registry keeps the
    // entry and list_agents reports "stopped", never null
    const stopped = agent({ runtime_status: "stopped" });
    // WHEN the Settings tab computes the Start button visibility
    // THEN Start is offered, otherwise the agent is unrestartable until the
    // app restarts
    expect(showStartButton(stopped)).toBe(true);
  });

  it("offers Start on an installed agent never loaded", () => {
    // GIVEN an installed agent that was never started this session
    const neverLoaded = agent({ runtime_status: null, id: null });
    // WHEN the Settings tab computes the Start button visibility
    // THEN Start is offered, as it already was before the fix
    expect(showStartButton(neverLoaded)).toBe(true);
  });

  it("hides Start while the agent is active or degraded", () => {
    // GIVEN a running agent, healthy or degraded
    // WHEN the Settings tab computes the Start button visibility
    // THEN Start stays hidden, Stop owns the section
    expect(showStartButton(agent({ runtime_status: "active" }))).toBe(false);
    expect(showStartButton(agent({ runtime_status: "degraded" }))).toBe(false);
  });

  it("hides Start during transitional states", () => {
    // GIVEN an agent mid-startup or mid-shutdown
    // WHEN the Settings tab computes the Start button visibility
    // THEN Start stays hidden until the state settles
    expect(showStartButton(agent({ runtime_status: "initializing" }))).toBe(false);
    expect(showStartButton(agent({ runtime_status: "stopping" }))).toBe(false);
  });

  it("hides Start when there is no install path to launch from", () => {
    // GIVEN a runtime-only agent with no install path, or one not installed
    // WHEN the Settings tab computes the Start button visibility
    // THEN Start stays hidden, handleStart would have nothing to invoke
    expect(
      showStartButton(agent({ runtime_status: "stopped", install_path: null })),
    ).toBe(false);
    expect(
      showStartButton(agent({ runtime_status: null, installed_at: null })),
    ).toBe(false);
  });
});
