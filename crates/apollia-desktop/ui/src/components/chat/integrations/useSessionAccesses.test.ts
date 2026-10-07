import { describe, expect, test, vi } from "vitest";
import type { SessionSettings } from "$lib/chat/sessionSettings";
import type { SessionAccessRow } from "$lib/chat/sessionAccesses";
import { createSessionAccesses, type SessionAccessesEffects } from "./useSessionAccesses.svelte";

function runtime(over: Partial<SessionAccessesEffects> = {}, initial: SessionSettings = { preset: 4 }) {
  const stored: { settings: SessionSettings } = { settings: initial };
  const effects: SessionAccessesEffects = {
    getSettings: vi.fn(async () => stored.settings),
    setSettings: vi.fn(async (_id: string, next: SessionSettings) => (stored.settings = next)),
    getAgent: vi.fn(async () => ({ access: ["web_search", "file_read"], sources: ["memory"] })),
    listTools: vi.fn(async () => [
      { name: "web_search", description: "Search the web", kind: "native" },
      { name: "file_read", description: "Read a file", kind: "native" },
      { name: "bash_executor", description: "Run a command", kind: "native" },
    ]),
    listTargets: vi.fn(async () => [
      { kind: "agent" as const, item_id: "market-watch", name: "Market watch", description: "" },
    ]),
    ...over,
  };
  return { effects, stored };
}

function line(rows: SessionAccessRow[], id: string): SessionAccessRow {
  const found = rows.find((r) => r.id === id);
  if (!found) throw new Error(`no line for ${id}`);
  return found;
}

describe("the accesses of a conversation are read from its agent in force", () => {
  test("the tab reads the agent, the machine and the conversation", async () => {
    // GIVEN a conversation with the agent "accountant"
    const { effects } = runtime();
    const controller = createSessionAccesses(effects);

    // WHEN the tab opens on it
    await controller.load("s-1", "accountant");

    // THEN the agent in force is the one read, and its accesses are on
    expect(effects.getAgent).toHaveBeenCalledWith("accountant");
    expect(effects.getSettings).toHaveBeenCalledWith("s-1");
    expect(line(controller.rows, "web_search").state).toBe("on");
    expect(line(controller.rows, "bash_executor").state).toBe("not_granted");
    expect(line(controller.rows, "agent:market-watch").state).toBe("not_granted");
    expect(controller.loading).toBe(false);
  });

  test("an agent the runtime does not know lists what the machine offers, none granted", async () => {
    // GIVEN a runtime that holds no definition for the agent
    const { effects } = runtime({ getAgent: vi.fn(async () => null) });
    const controller = createSessionAccesses(effects);

    // WHEN the tab opens
    await controller.load("s-1", "ghost");

    // THEN every line is an access the agent does not hold
    expect(controller.rows.length).toBe(4);
    expect(controller.rows.every((r) => !r.held)).toBe(true);
  });
});

describe("a switch writes the settings of the conversation and nothing else", () => {
  test("cutting an access stores it with the conversation, its other settings kept", async () => {
    // GIVEN the tab open on a conversation that chose a preset
    const { effects, stored } = runtime();
    const controller = createSessionAccesses(effects);
    await controller.load("s-1", "accountant");

    // WHEN the web search is switched off
    await controller.switchAccess(line(controller.rows, "web_search"), false);

    // THEN the conversation stores the cut beside its preset
    expect(effects.setSettings).toHaveBeenCalledTimes(1);
    expect(stored.settings).toEqual({ preset: 4, disabled_accesses: ["web_search"] });
    expect(line(controller.rows, "web_search").state).toBe("off");
    // AND the agent was read, never written: the effects hold no way to write it
    expect(Object.keys(effects).sort()).toEqual(
      ["getAgent", "getSettings", "listTargets", "listTools", "setSettings"],
    );
  });

  test("switching on an access the agent does not hold proposes it for the conversation", async () => {
    // GIVEN the tab open on an agent without the command tool
    const { effects, stored } = runtime();
    const controller = createSessionAccesses(effects);
    await controller.load("s-1", "accountant");

    // WHEN the command tool is switched on
    await controller.switchAccess(line(controller.rows, "bash_executor"), true);

    // THEN the conversation proposes it, and the line says so
    expect(stored.settings).toEqual({ preset: 4, proposed_accesses: ["bash_executor"] });
    expect(line(controller.rows, "bash_executor").state).toBe("proposed");
    expect(line(controller.rows, "bash_executor").held).toBe(false);
  });

  test("a refused write puts the switch back and says why", async () => {
    // GIVEN a runtime that refuses the settings of a closed conversation
    const { effects } = runtime({
      setSettings: vi.fn(async () => {
        throw new Error("SESSION_CLOSED");
      }),
    });
    const controller = createSessionAccesses(effects);
    await controller.load("s-1", "accountant");

    // WHEN an access is switched off
    await controller.switchAccess(line(controller.rows, "web_search"), false);

    // THEN the line is back on and the refusal is shown
    expect(line(controller.rows, "web_search").state).toBe("on");
    expect(controller.error).toBe("SESSION_CLOSED");
    controller.dismissError();
    expect(controller.error).toBeNull();
  });

  test("two switches moved in a row are both stored", async () => {
    // GIVEN the tab open
    const { effects, stored } = runtime();
    const controller = createSessionAccesses(effects);
    await controller.load("s-1", "accountant");

    // WHEN two accesses are switched off without waiting for the first write
    const first = controller.switchAccess(line(controller.rows, "web_search"), false);
    const second = controller.switchAccess(line(controller.rows, "file_read"), false);
    await Promise.all([first, second]);

    // THEN the conversation names both
    expect(stored.settings.disabled_accesses).toEqual(["web_search", "file_read"]);
  });
});

describe("an installed agent may read the profile when the conversation says so", () => {
  test("the switch starts on the profile source of the agent, then follows the conversation", async () => {
    // GIVEN an agent whose profile source is not checked
    const { effects, stored } = runtime();
    const controller = createSessionAccesses(effects);
    await controller.load("s-1", "accountant");
    const before = controller.agentReadsProfile;

    // WHEN the conversation lets it read the profile
    await controller.setProfileAccess(true);

    // THEN the choice is stored with the conversation, the agent untouched
    expect(before).toBe(false);
    expect(controller.agentReadsProfile).toBe(true);
    expect(stored.settings).toEqual({ preset: 4, profile_access: true });
  });

  test("a conversation can close the profile an agent reads by default", async () => {
    // GIVEN an agent whose profile source is checked
    const { effects, stored } = runtime({
      getAgent: vi.fn(async () => ({ access: [], sources: ["profile"] })),
    });
    const controller = createSessionAccesses(effects);
    await controller.load("s-1", "accountant");
    const before = controller.agentReadsProfile;

    // WHEN the conversation switches the reading off
    await controller.setProfileAccess(false);

    // THEN the agent no longer reads it here
    expect(before).toBe(true);
    expect(controller.agentReadsProfile).toBe(false);
    expect(stored.settings.profile_access).toBe(false);
  });
});
