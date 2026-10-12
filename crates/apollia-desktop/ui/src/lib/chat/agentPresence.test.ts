import { describe, expect, test } from "vitest";
import { agentPresence } from "./agentPresence";

describe("agentPresence", () => {
  test("a free conversation looks for no agent", () => {
    // GIVEN a conversation with no agent
    // WHEN its agent is looked for
    const presence = agentPresence({ mode: "libre", agentName: null, known: [], verifiedFor: null });
    // THEN there is nothing missing
    expect(presence).toBe("present");
  });

  test("an agent the list names is present", () => {
    // GIVEN a conversation with an agent the list names
    // WHEN its agent is looked for
    const presence = agentPresence({
      mode: "agent",
      agentName: "inbox-triage",
      known: ["guide", "inbox-triage"],
      verifiedFor: null,
    });
    // THEN it is present
    expect(presence).toBe("present");
  });

  test("an agent just installed is checked, not called missing, while the list lags", () => {
    // GIVEN a conversation opened right after its agent was installed, the
    // list of the interface not naming it yet
    // WHEN its agent is looked for before a fresh list was read
    const presence = agentPresence({
      mode: "agent",
      agentName: "inbox-triage",
      known: ["guide"],
      verifiedFor: null,
    });
    // THEN it is being checked: nothing says it is gone
    expect(presence).toBe("checking");
  });

  test("an agent a fresh list still does not name is missing", () => {
    // GIVEN the same conversation once a fresh list was read for that agent
    // WHEN its agent is looked for
    const presence = agentPresence({
      mode: "agent",
      agentName: "inbox-triage",
      known: ["guide"],
      verifiedFor: "inbox-triage",
    });
    // THEN it is missing
    expect(presence).toBe("missing");
  });

  test("a fresh list read for another agent does not vouch for this one", () => {
    // GIVEN a fresh list read for the agent of an earlier conversation
    // WHEN the agent of this one is looked for
    const presence = agentPresence({
      mode: "agent",
      agentName: "inbox-triage",
      known: ["guide"],
      verifiedFor: "another-agent",
    });
    // THEN it is checked on its own
    expect(presence).toBe("checking");
  });
});
