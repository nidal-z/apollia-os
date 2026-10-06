import { describe, test, expect } from "vitest";
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import {
  connectionsPackage,
  firstTaskAgentId,
  parseFirstAgent,
  proposesConnections,
  type FirstAgent,
} from "./firstAgent";

const INSTALLED: FirstAgent = {
  id: "press-watch",
  name: "Press watch",
  way: "install",
  packageName: "press-watch-pack",
};

describe("first agent - what the steps after it read", () => {
  test("without an agent, no connection is proposed", () => {
    // GIVEN a first launch whose first-agent step was passed
    const agent = null;

    // WHEN the connection step asks what it has to propose
    // THEN it has nothing, and no package to read required connections from
    expect(proposesConnections(agent)).toBe(false);
    expect(connectionsPackage(agent)).toBeNull();
  });

  test("without an agent, the first conversation is with Apollia", () => {
    // GIVEN a first launch whose first-agent step was passed
    // WHEN the first-task step asks which agent it opens on
    // THEN it is Apollia, the agent every installation has
    expect(firstTaskAgentId(null)).toBe(APOLLIA_AGENT_ID);
    expect(APOLLIA_AGENT_ID).toBe("apollia:chat");
  });

  test("an installed agent brings its package to the connection step and itself to the first task", () => {
    // GIVEN an agent installed from a package
    // WHEN the two steps read it
    // THEN the connections are those of its package, the conversation is with it
    expect(proposesConnections(INSTALLED)).toBe(true);
    expect(connectionsPackage(INSTALLED)).toBe("press-watch-pack");
    expect(firstTaskAgentId(INSTALLED)).toBe("press-watch");
  });

  test("an example is read the same way as any installed package", () => {
    // GIVEN an agent that came with an example
    const example: FirstAgent = { ...INSTALLED, way: "example", packageName: "qualify-requests" };

    // WHEN the two steps read it
    // THEN nothing sets it apart from a package opened from a file
    expect(proposesConnections(example)).toBe(true);
    expect(connectionsPackage(example)).toBe("qualify-requests");
    expect(firstTaskAgentId(example)).toBe("press-watch");
  });

  test("an agent created in the step requires no connection and takes the first task", () => {
    // GIVEN an agent created by describing the need, which no package brought
    const created: FirstAgent = { id: "notes-helper", name: "Notes helper", way: "create", packageName: null };

    // WHEN the two steps read it
    // THEN no connection is proposed, and the conversation is with it
    expect(proposesConnections(created)).toBe(false);
    expect(firstTaskAgentId(created)).toBe("notes-helper");
  });
});

describe("first agent - read back from its stored text", () => {
  test("what was stored comes back unchanged", () => {
    // GIVEN an agent written as text
    const raw = JSON.stringify(INSTALLED);

    // WHEN it is read back
    // THEN it is the same agent
    expect(parseFirstAgent(raw)).toEqual(INSTALLED);
  });

  test("anything that is not an agent reads as none", () => {
    // GIVEN texts that are no agent: nothing, broken text, another shape, an
    // unknown way, an empty identifier
    const texts = [
      null,
      "",
      "{not json",
      "42",
      JSON.stringify({ name: "No id", way: "install", packageName: null }),
      JSON.stringify({ ...INSTALLED, way: "borrowed" }),
      JSON.stringify({ ...INSTALLED, id: "" }),
      JSON.stringify({ ...INSTALLED, packageName: 7 }),
    ];

    // WHEN each is read back
    // THEN each is an onboarding without an agent, never an error
    for (const text of texts) {
      expect(parseFirstAgent(text), String(text)).toBeNull();
    }
  });

  test("an agent stored without a name is shown by its identifier", () => {
    // GIVEN an agent whose name was stored empty
    const raw = JSON.stringify({ ...INSTALLED, name: "" });

    // WHEN it is read back
    // THEN its identifier stands for its name
    expect(parseFirstAgent(raw)?.name).toBe("press-watch");
  });
});
