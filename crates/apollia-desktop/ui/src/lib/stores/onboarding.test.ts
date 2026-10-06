import { afterEach, beforeEach, describe, test, expect, vi } from "vitest";
import { get } from "svelte/store";
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import {
  firstTaskAgentId,
  proposesConnections,
  type FirstAgent,
} from "$lib/onboarding/firstAgent";

// Vitest runs in `node`: there is no `localStorage`. A map stands for it, set
// before the store module is loaded, since the store reads what was kept when
// it is created, the way it does when the application starts.

const INSTALLED: FirstAgent = {
  id: "press-watch",
  name: "Press watch",
  way: "install",
  packageName: "press-watch-pack",
};
const CREATED: FirstAgent = { id: "notes-helper", name: "Notes helper", way: "create", packageName: null };
const EXAMPLE: FirstAgent = {
  id: "qualifier",
  name: "Request qualifier",
  way: "example",
  packageName: "qualify-requests",
};

function fakeStorage(initial: Record<string, string> = {}) {
  const entries = new Map(Object.entries(initial));
  return {
    entries,
    getItem: (key: string) => entries.get(key) ?? null,
    setItem: (key: string, value: string) => void entries.set(key, value),
    removeItem: (key: string) => void entries.delete(key),
  };
}

async function loadStore() {
  vi.resetModules();
  return import("./onboarding");
}

describe("onboarding store - the agent of the first launch", () => {
  let storage: ReturnType<typeof fakeStorage>;

  beforeEach(() => {
    storage = fakeStorage();
    vi.stubGlobal("localStorage", storage);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  test("a first launch starts with no agent", async () => {
    // GIVEN an application that never kept an agent
    const { onboardingFirstAgent } = await loadStore();

    // WHEN the store is read
    // THEN it holds none
    expect(get(onboardingFirstAgent)).toBeNull();
  });

  test("the agent obtained is kept for the next steps, whichever way it came", async () => {
    for (const agent of [INSTALLED, CREATED, EXAMPLE]) {
      // GIVEN the store of a first launch
      const { onboardingFirstAgent } = await loadStore();

      // WHEN the first-agent step keeps the agent it obtained
      onboardingFirstAgent.retain(agent);

      // THEN the steps after it read that agent
      const kept = get(onboardingFirstAgent);
      expect(kept, agent.way).toEqual(agent);
      expect(firstTaskAgentId(kept), agent.way).toBe(agent.id);
    }
  });

  test("an agent obtained later replaces the one kept", async () => {
    // GIVEN a store that keeps an installed agent
    const { onboardingFirstAgent } = await loadStore();
    onboardingFirstAgent.retain(INSTALLED);

    // WHEN the person then installs an example
    onboardingFirstAgent.retain(EXAMPLE);

    // THEN one agent is kept, the last obtained
    expect(get(onboardingFirstAgent)).toEqual(EXAMPLE);
  });

  test("the agent kept survives a restart", async () => {
    // GIVEN a first launch that kept an agent
    const first = await loadStore();
    first.onboardingFirstAgent.retain(EXAMPLE);
    expect(storage.entries.has(first.FIRST_AGENT_STORAGE_KEY)).toBe(true);

    // WHEN the application starts again and the store is created anew
    const second = await loadStore();

    // THEN the agent is still the one the next steps read
    expect(get(second.onboardingFirstAgent)).toEqual(EXAMPLE);
  });

  test("a passed step keeps no agent, and the next steps know it", async () => {
    // GIVEN a store that kept an agent on an earlier pass through the step
    const { onboardingFirstAgent, FIRST_AGENT_STORAGE_KEY } = await loadStore();
    onboardingFirstAgent.retain(INSTALLED);

    // WHEN the step is passed
    onboardingFirstAgent.clear();

    // THEN no agent is kept, in the store or across a restart
    const kept = get(onboardingFirstAgent);
    expect(kept).toBeNull();
    expect(storage.entries.has(FIRST_AGENT_STORAGE_KEY)).toBe(false);
    // AND the connection step proposes nothing, the first task opens on Apollia
    expect(proposesConnections(kept)).toBe(false);
    expect(firstTaskAgentId(kept)).toBe(APOLLIA_AGENT_ID);
    expect(get((await loadStore()).onboardingFirstAgent)).toBeNull();
  });

  test("a stored value this interface does not understand is no agent", async () => {
    // GIVEN a browser store holding something else under the key
    vi.stubGlobal(
      "localStorage",
      fakeStorage({ "apollia.onboarding.first_agent": '{"way":"borrowed"}' }),
    );

    // WHEN the store is created
    const { onboardingFirstAgent, FIRST_AGENT_STORAGE_KEY } = await loadStore();

    // THEN it holds no agent, under the key the test names
    expect(FIRST_AGENT_STORAGE_KEY).toBe("apollia.onboarding.first_agent");
    expect(get(onboardingFirstAgent)).toBeNull();
  });

  test("without a browser store the agent is still kept for this run", async () => {
    // GIVEN no `localStorage` at all
    vi.unstubAllGlobals();
    const { onboardingFirstAgent } = await loadStore();

    // WHEN an agent is kept
    onboardingFirstAgent.retain(CREATED);

    // THEN the store holds it
    expect(get(onboardingFirstAgent)).toEqual(CREATED);
  });
});
