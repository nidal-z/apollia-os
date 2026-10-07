import { describe, test, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The layout and theme stores read the browser storage at import; node has none.
vi.hoisted(() => {
  const map = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as Storage;
});

import "$lib/i18n";
import Sidebar, {
  PRIMARY_NAV,
  SECONDARY_NAV,
  visibleDeveloperNav,
  visiblePrimaryNav,
  visibleSidebarRoutes,
} from "./Sidebar.svelte";
import { developerMode } from "$lib/stores/developerMode";
import { currentRoute } from "$lib/stores/navigation";
import { invoke } from "@tauri-apps/api/core";
import type { PendingDecision, PendingKind } from "$lib/ipc/pendingDecisions";
import { refreshPendingDecisions } from "$lib/stores/pendingDecisions";
import { pendingApprovals } from "$lib/stores/hitl";
import { pendingChatApprovals } from "$lib/stores/chat";
import { DEVELOPER_ROUTES } from "$lib/navigation/developerRoutes";
import glossary from "$lib/i18n/glossary.json";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

// The entry lists are locked through the module helpers, and the placement of
// the developer group through Svelte's server renderer, the only renderer the
// node environment of this suite offers.

function routes(developerMode: boolean): string[] {
  return visiblePrimaryNav(developerMode).map((entry) => entry.route);
}

function developerRoutes(developerMode: boolean): string[] {
  return visibleDeveloperNav(developerMode).map((entry) => entry.route);
}

async function renderSidebar(on: boolean, language: "en" | "fr" = "en"): Promise<string> {
  locale.set(language);
  await waitLocale(language);
  developerMode.set(on);
  // The whole menu is what these cases read; the chat, where it stands back
  // as a rail, has its own case.
  currentRoute.set("agents");
  return renderToString(Sidebar).body;
}

/** The routes of the entries drawn, in the order the markup draws them. */
function drawnRoutes(html: string): string[] {
  return [...html.matchAll(/data-testid="nav-([\w-]+)"/g)].map((match) => match[1]);
}

/** The rank each drawn entry declares, by route. */
function drawnRanks(html: string): Record<string, string> {
  const ranks: Record<string, string> = {};
  for (const match of html.matchAll(/data-rank="(\w+)" data-testid="nav-([\w-]+)"/g)) {
    ranks[match[2]] = match[1];
  }
  return ranks;
}

type Catalogue = Record<string, unknown>;

function resolve(catalogue: Catalogue, dottedKey: string): string | undefined {
  let node: unknown = catalogue;
  for (const part of dottedKey.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Catalogue)[part];
  }
  return typeof node === "string" ? node : undefined;
}

const PRIMARY = ["chat", "agents", "integrations", "llm", "inbox"];
const SECONDARY = ["library", "automations", "authorizations", "journal", "settings"];
const DEVELOPER_GROUP = ["transcriptions", "observability", "memory"];

// The screen of the glossary each of the ten entries carries the name of.
const OFFICIAL_SCREEN: Record<string, keyof (typeof glossary)["fr"]["screens"]> = {
  chat: "chat",
  agents: "agents",
  integrations: "connections",
  llm: "ai",
  inbox: "review",
  library: "library",
  automations: "automations",
  authorizations: "permissions",
  journal: "activity_log",
  settings: "settings",
};

describe("Sidebar - two ranks, five entries each", () => {
  test("with the developer mode off, the sidebar lists exactly the ten entries, in order", async () => {
    // GIVEN the developer mode off
    // WHEN the sidebar computes its entries, then draws them
    const listed = visibleSidebarRoutes(false);
    const html = await renderSidebar(false);

    // THEN the five everyday screens come first, the five others second,
    // and nothing else is listed or drawn
    expect(listed).toEqual([...PRIMARY, ...SECONDARY]);
    expect(drawnRoutes(html)).toEqual([...PRIMARY, ...SECONDARY]);
    expect(html).not.toContain('data-testid="nav-developer"');
    expect(html).not.toContain('data-testid="nav-policy"');
  });

  test("with the developer mode on, the developer screen closes the first rank and the group follows", async () => {
    // GIVEN the developer mode on
    // WHEN the sidebar computes its entries, then draws them
    const listed = visibleSidebarRoutes(true);
    const html = await renderSidebar(true);

    // THEN the first rank holds six entries, the developer screen after the
    // review screen; the second rank is unchanged; the group comes last
    const expected = [...PRIMARY, "developer", ...SECONDARY, ...DEVELOPER_GROUP];
    expect(routes(true)).toEqual([...PRIMARY, "developer"]);
    expect(listed).toEqual(expected);
    expect(drawnRoutes(html)).toEqual(expected);
  });

  test("each drawn entry declares its rank, the second quieter than the first", async () => {
    // GIVEN the developer mode on, where the three ranks are drawn
    // WHEN the sidebar is rendered
    const html = await renderSidebar(true);
    const ranks = drawnRanks(html);

    // THEN the entries of each rank say which one they belong to, and the
    // ranks sit in their own containers, the first before the second
    for (const route of [...PRIMARY, "developer"]) expect(ranks[route]).toBe("primary");
    for (const route of SECONDARY) expect(ranks[route]).toBe("secondary");
    for (const route of DEVELOPER_GROUP) expect(ranks[route]).toBe("developer");
    const first = html.indexOf('data-testid="sidebar-primary-rank"');
    const second = html.indexOf('data-testid="sidebar-secondary-rank"');
    expect(first).toBeGreaterThan(-1);
    expect(second).toBeGreaterThan(first);
    expect(html.indexOf('data-testid="sidebar-developer-group"')).toBeGreaterThan(second);
  });

  test("the organisation policy is no entry of the sidebar, in either mode", () => {
    // GIVEN the developer mode off, then on
    // WHEN the routes of the sidebar are listed
    const off = visibleSidebarRoutes(false);
    const on = visibleSidebarRoutes(true);

    // THEN the policy is among neither, and no route is listed twice
    expect(off).not.toContain("policy");
    expect(on).not.toContain("policy");
    expect(new Set(on).size).toBe(on.length);
  });

  test.each([
    ["fr", fr as Catalogue],
    ["en", en as Catalogue],
  ] as const)("the ten entries carry the official screen names of the glossary in %s", async (language, catalogue) => {
    // GIVEN the official screen names of the glossary for one language
    const official = glossary[language].screens;
    const entries = [...PRIMARY_NAV.filter((entry) => !entry.developer), ...SECONDARY_NAV];

    // WHEN the label of each of the ten entries is resolved in that language
    const labels = Object.fromEntries(
      entries.map((entry) => [entry.route, resolve(catalogue, entry.labelKey)]),
    );
    const html = await renderSidebar(false, language);

    // THEN every label is the word of the glossary, and it is the word drawn
    expect(Object.keys(labels)).toEqual(Object.keys(OFFICIAL_SCREEN));
    for (const [route, screen] of Object.entries(OFFICIAL_SCREEN)) {
      expect(labels[route]).toBe(official[screen]);
      expect(html).toContain(`>${official[screen]}</span>`);
    }
  });

  test.each([
    ["fr", fr as Catalogue],
    ["en", en as Catalogue],
  ] as const)("with the developer mode on, the developer entry is named in %s", async (language, catalogue) => {
    // GIVEN the name of the developer entry in one language, an ordinary
    // catalogue string rather than a word of the glossary
    const name = resolve(catalogue, "nav.sidebar.developer");
    expect(name?.trim()).toBeTruthy();
    expect(Object.values(glossary[language].screens)).not.toContain(name);

    // WHEN the sidebar is rendered in that language, mode on then off
    const on = await renderSidebar(true, language);
    const off = await renderSidebar(false, language);

    // THEN the developer entry carries its name with the mode on, and is not
    // drawn at all with the mode off
    expect(on).toContain(`>${name}</span>`);
    expect(off).not.toContain(`>${name}</span>`);
  });
});

describe("Sidebar - primary entries and the developer mode", () => {
  test("with the developer mode off, the AI screen is listed and no technical screen is", () => {
    // GIVEN the developer mode off
    const developerMode = false;

    // WHEN the sidebar computes its entries
    const shown = [...routes(developerMode), ...developerRoutes(developerMode)];

    // THEN the AI screen is there for every user, Transcriptions and
    // Observability are not
    expect(shown).toContain("llm");
    expect(shown).not.toContain("transcriptions");
    expect(shown).not.toContain("observability");
  });

  test("Observability and Transcriptions never sit among the primary entries", () => {
    // GIVEN the developer mode on, where every primary entry is shown
    const developerMode = true;

    // WHEN the sidebar computes its primary entries
    const shown = routes(developerMode);

    // THEN the two technical screens are not part of them
    expect(shown).toContain("llm");
    expect(shown).not.toContain("observability");
    expect(shown).not.toContain("transcriptions");
  });

  test("with the developer mode on, the developer group carries Transcriptions, Observability, then Memory", () => {
    // GIVEN the developer mode on
    const developerMode = true;

    // WHEN the sidebar computes its developer group
    const group = developerRoutes(developerMode);

    // THEN it holds the three technical screens, in that order, and exactly the
    // routes the router keeps out of reach with the mode off
    expect(group).toEqual(["transcriptions", "observability", "memory"]);
    expect(group).toEqual([...DEVELOPER_ROUTES]);
  });

  test("the Memory screen is never a primary entry, mode off or on", () => {
    // GIVEN the developer mode off, then on
    // WHEN the sidebar computes its primary entries
    const off = routes(false);
    const on = routes(true);

    // THEN the Memory screen is among neither: it only lives in the developer
    // group, and with the mode off it is nowhere in the sidebar
    expect(off).not.toContain("memory");
    expect(on).not.toContain("memory");
    expect(developerRoutes(false)).not.toContain("memory");
  });

  test("the developer group holds no developer server entry", () => {
    // GIVEN the developer mode on
    // WHEN the developer group is computed
    const group = developerRoutes(true);

    // THEN the Developer screen is not one of its entries: it belongs to the
    // primary rank
    expect(group).not.toContain("developer");
  });

  test("the developer mode only adds entries, it never removes one", () => {
    // GIVEN the entries shown with the mode off
    const off = routes(false);

    // WHEN the mode is turned on
    const on = routes(true);

    // THEN every entry of the default sidebar is still there
    expect(off.every((route) => on.includes(route))).toBe(true);
  });
});

describe("Sidebar - the rendered developer group", () => {
  test("with the developer mode off, the group and its three entries are absent", async () => {
    // GIVEN the developer mode off
    // WHEN the sidebar is rendered
    const html = await renderSidebar(false);

    // THEN neither the group nor its entries are in the markup
    expect(html).toContain('data-testid="nav-settings"');
    expect(html).not.toContain('data-testid="sidebar-developer-group"');
    expect(html).not.toContain('data-testid="nav-observability"');
    expect(html).not.toContain('data-testid="nav-transcriptions"');
    expect(html).not.toContain('data-testid="nav-memory"');
  });

  test("with the developer mode on, the group follows the secondary entries", async () => {
    // GIVEN the developer mode on
    // WHEN the sidebar is rendered
    const html = await renderSidebar(true);

    // THEN the group comes after Connections and Settings, and holds
    // Transcriptions, Observability, then Memory
    const settings = html.indexOf('data-testid="nav-settings"');
    const group = html.indexOf('data-testid="sidebar-developer-group"');
    const transcriptions = html.indexOf('data-testid="nav-transcriptions"');
    const observability = html.indexOf('data-testid="nav-observability"');
    expect(settings).toBeGreaterThan(-1);
    expect(group).toBeGreaterThan(settings);
    expect(transcriptions).toBeGreaterThan(group);
    expect(observability).toBeGreaterThan(transcriptions);
    expect(html.indexOf('data-testid="nav-memory"')).toBeGreaterThan(observability);
    expect(html).toContain('aria-labelledby="sidebar-developer-group-title"');
    expect(html).toContain(">Developer mode</span>");
  });
});

function waiting(kind: PendingKind): PendingDecision {
  return {
    id: `${kind}-1`,
    kind,
    agentId: "agent",
    summary: "Something is asked.",
    access: null,
    family: null,
    answers: kind === "question" ? ["answer"] : ["once", "refuse"],
    questions: null,
    allowOther: false,
    origin: {},
    askedAt: null,
  };
}

/** Publishes `items` as the list of decisions the runtime holds. */
async function runtimeLists(items: PendingDecision[]): Promise<void> {
  vi.mocked(invoke).mockImplementationOnce(async () => ({ items }));
  await refreshPendingDecisions();
}

/** The count drawn beside the review entry, or null when none is drawn. */
function reviewCount(html: string): string | null {
  return /data-testid="nav-inbox-count">([^<]*)</.exec(html)?.[1] ?? null;
}

describe("Sidebar - the count of what waits for the user", () => {
  test("a list with one item of each kind shows 4", async () => {
    // GIVEN a consent, a paused task, a batch item and a question waiting
    await runtimeLists([
      waiting("consent"),
      waiting("task_pause"),
      waiting("batch_item"),
      waiting("question"),
    ]);

    // WHEN the sidebar is drawn
    const html = await renderSidebar(false);

    // THEN the review entry counts the four of them
    expect(reviewCount(html)).toBe("4");
  });

  test("the count is the length of the list, whatever the event stores hold", async () => {
    // GIVEN an empty list, while the stores the live events feed still hold
    // two paused tasks and a consent
    await runtimeLists([]);
    pendingApprovals.set([{}, {}] as never);
    pendingChatApprovals.set([{}] as never);

    // WHEN the sidebar is drawn
    const html = await renderSidebar(false);
    pendingApprovals.set([]);
    pendingChatApprovals.set([]);

    // THEN no count is drawn: nothing is added to the list
    expect(reviewCount(html)).toBeNull();
  });

  test("the sidebar source holds one formula for that count", () => {
    // GIVEN the source of the sidebar
    const source = readFileSync(
      fileURLToPath(new URL("./Sidebar.svelte", import.meta.url)),
      "utf8",
    );

    // WHEN looking for the counters it used to add up
    // THEN none is read any more, and the shared count is
    for (const former of ["pendingCount", "pendingChatApprovalCount", "pendingUserInputCount"]) {
      expect(source).not.toContain(former);
    }
    expect(source).toContain("$toReviewCount");
  });
});

describe("Sidebar - the menu on the chat screen", () => {
  test("on the chat, the menu stands back as a rail and keeps every entry", async () => {
    // GIVEN the whole menu, drawn on another screen
    const whole = await renderSidebar(false);

    // WHEN the chat is the screen shown
    currentRoute.set("chat");
    const rail = renderToString(Sidebar).body;

    // THEN the same entries are drawn, in the narrow column of the rail
    expect(drawnRoutes(rail)).toEqual(drawnRoutes(whole));
    expect(whole).not.toContain("w-16 flex-shrink-0");
    expect(rail).toContain("w-16 flex-shrink-0");
  });
});
