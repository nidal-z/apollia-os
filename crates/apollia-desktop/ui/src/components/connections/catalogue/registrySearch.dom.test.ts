import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import type { RegistryServerView } from "$lib/types";
import Connections from "../../../routes/Connections.svelte";

// The dialogs of the page run Svelte transitions, which jsdom cannot play: an
// animation that finishes at once stands in for them.
Element.prototype.animate = function (): Animation {
  const animation = {
    onfinish: null as (() => void) | null,
    cancel: () => {},
    finished: Promise.resolve(),
  };
  setTimeout(() => animation.onfinish?.(), 0);
  return animation as unknown as Animation;
};

// The Connections page is mounted whole, on a simulated clock: what is held is
// when the registry's search is asked from a typed text, and which of its
// answers the add window shows when they come back late.

function entry(name: string, title: string): RegistryServerView {
  return {
    name,
    title,
    description: null,
    version: "1.0.0",
    repository_url: null,
    website_url: null,
    packages: null,
    trust_level: "community",
    category: null,
    enrichment: null,
    is_installed: false,
    remotes: [],
  };
}

const FARM = entry("io.example/farm-data", "Farm data");
const FAR_AWAY = entry("io.github.someone/far-away", "Far away");
const FABRIC = entry("io.example/fabric", "Fabric");

interface Pending {
  term: string;
  answer: (servers: RegistryServerView[]) => void;
}

let searches: Pending[] = [];
let full: RegistryServerView[] = [];
let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

async function settle(rounds = 20): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await vi.advanceTimersByTimeAsync(0);
    flushSync();
  }
}

function byTestid(id: string): HTMLElement | null {
  return document.querySelector(`[data-testid="${CSS.escape(id)}"]`);
}

async function click(id: string): Promise<void> {
  const element = byTestid(id);
  if (!element) throw new Error(`no element ${id}`);
  element.click();
  await settle();
}

async function type(text: string): Promise<void> {
  const field = byTestid("catalogue-search");
  const input = field instanceof HTMLInputElement ? field : field?.querySelector("input");
  if (!input) throw new Error("no search field");
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  await settle(2);
}

function searchTerms(): string[] {
  return searches.map((s) => s.term);
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  vi.useFakeTimers();
  searches = [];
  full = [];
  invoke.mockReset();
  invoke.mockImplementation(async (command: unknown, args: unknown) => {
    const name = String(command);
    if (name === "fetch_mcp_registry") {
      const search = (args as { search?: string } | undefined)?.search;
      if (search === undefined) return full;
      return new Promise<RegistryServerView[]>((answer) => searches.push({ term: search, answer }));
    }
    if (/(^|_)list(_|$)/.test(name) || name.startsWith("fetch_")) return [];
    return null;
  });
  target = document.createElement("div");
  document.body.appendChild(target);
  page = mount(Connections, { target, props: {} });
  await settle();
  await click("connections-open-catalogue");
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
  vi.useRealTimers();
});

describe("a typed text asks the registry's search once the typing rests", () => {
  test("a quick typing asks once, for the last text, after the pause", async () => {
    // GIVEN the add window, open on the selection

    // WHEN three letters are typed a tenth of a second apart
    await type("f");
    await vi.advanceTimersByTimeAsync(100);
    await type("fa");
    await vi.advanceTimersByTimeAsync(100);
    await type("far");
    await vi.advanceTimersByTimeAsync(349);

    // THEN nothing is asked before the pause is over, then one search for the last text
    expect(searchTerms()).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(searchTerms()).toEqual(["far"]);
  });
});

describe("an answer that comes back late is shown only for what the window shows by then", () => {
  test("an answer for a text since shortened is not shown", async () => {
    // GIVEN a search asked for a text, still unanswered
    await type("fab");
    await vi.advanceTimersByTimeAsync(350);
    expect(searchTerms()).toEqual(["fab"]);

    // WHEN the text is shortened to one letter, then the answer comes back
    await type("f");
    searches[0].answer([FABRIC]);
    await settle();

    // THEN what was found for the older text is not shown under the newer one
    expect(byTestid(`catalogue-entry-${FABRIC.name}`)).toBeNull();
  });

  test("an answer that comes back after Explore all keeps the whole catalogue", async () => {
    // GIVEN a search asked before Explore all, still unanswered, and a
    //       public catalogue holding an entry the text matches
    full = [FARM];
    await type("far");
    await vi.advanceTimersByTimeAsync(350);
    expect(searchTerms()).toEqual(["far"]);

    // WHEN Explore all answers, then the search does
    await click("catalogue-explore-all");
    expect(byTestid(`catalogue-entry-${FARM.name}`)).not.toBeNull();
    searches[0].answer([FAR_AWAY]);
    await settle();

    // THEN the window shows the whole catalogue and what the search found,
    //      and the search was not asked again
    expect(byTestid(`catalogue-entry-${FARM.name}`)).not.toBeNull();
    expect(byTestid(`catalogue-entry-${FAR_AWAY.name}`)).not.toBeNull();
    expect(byTestid("catalogue-source-all")?.getAttribute("aria-pressed")).toBe("true");
    expect(searchTerms()).toEqual(["far"]);
  });
});
