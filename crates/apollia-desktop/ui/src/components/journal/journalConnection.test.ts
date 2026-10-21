import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, test, vi } from "vitest";
import { render } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The theme store reads the browser storage at import; node has none.
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
import {
  requestJournalConnection,
  takeRequestedJournalConnection,
  type JournalConnectionFocus,
} from "$lib/stores/journalFocus";
import JournalConnectionFilter from "./JournalConnectionFilter.svelte";
import {
  NO_FILTER,
  activeFilterCount,
  openingFilters,
  queryOf,
  withoutConnection,
} from "./journal";
import { exportQueryOf, filterMentions } from "./journalExport";

const here = dirname(fileURLToPath(import.meta.url));
const read = (relative: string): string => readFileSync(resolve(here, relative), "utf8");

const NOW = new Date(2026, 9, 21, 12, 0, 0);
const NOTION: JournalConnectionFocus = { prefix: "mcp:notion/", label: "Notion" };

describe("journal focus - the connection another screen opens the journal on", () => {
  test("a connection asked for is taken once, then nothing is", () => {
    // GIVEN a screen that asks the journal to open on Notion
    requestJournalConnection(NOTION);

    // WHEN the journal takes the request twice
    const first = takeRequestedJournalConnection();
    const second = takeRequestedJournalConnection();

    // THEN the first arrival opens on Notion and the next one on nothing
    expect(first).toEqual(NOTION);
    expect(second).toBeNull();
  });

  test("with nothing asked, the journal opens with no filter", () => {
    // GIVEN no request
    // WHEN the opening filters are read
    const filters = openingFilters(takeRequestedJournalConnection());

    // THEN they are the filters of an untouched screen
    expect(filters).toEqual(NO_FILTER);
    expect(activeFilterCount(filters)).toBe(0);
  });

  test("opened on a connection, the journal asks the route for its tools only", () => {
    // GIVEN the journal opened on Notion
    const filters = openingFilters(NOTION);

    // WHEN the query of the list and of the export are written
    const list = queryOf(filters, false, NOW, 0);
    const exported = exportQueryOf(filters, false, NOW);

    // THEN both carry the prefix of Notion, and the filter counts as one
    expect(list.tool_prefixes).toEqual(["mcp:notion/"]);
    expect(exported.tool_prefixes).toEqual(["mcp:notion/"]);
    expect(activeFilterCount(filters)).toBe(1);
    expect(queryOf(NO_FILTER, false, NOW, 0).tool_prefixes).toBeUndefined();
  });

  test("removing the connection keeps every other filter", () => {
    // GIVEN the journal on Notion, narrowed to the failures of `watch`
    const filters = { ...openingFilters(NOTION), agent: "watch", result: "failure" as const };

    // WHEN the connection is removed
    const left = withoutConnection(filters);

    // THEN the agent and the result stay, and the route is no longer asked for a prefix
    expect(left.connection).toBeNull();
    expect(left.agent).toBe("watch");
    expect(left.result).toBe("failure");
    expect(queryOf(left, false, NOW, 0).tool_prefixes).toBeUndefined();
  });

  test("an export recalls the connection by its name", () => {
    // GIVEN the journal on Notion
    // WHEN the filters of an export are recalled
    const mentions = filterMentions(openingFilters(NOTION), (agent) => agent ?? "");

    // THEN the name of the connection comes first
    expect(mentions[0]).toEqual({ text: "Notion" });
  });
});

describe("journal screen - the connection filter", () => {
  test("the filter says the connection by its name and offers to remove it", async () => {
    // GIVEN the interface in English
    locale.set("en");
    await waitLocale("en");

    // WHEN the filter of Notion is rendered
    const html = render(JournalConnectionFilter, {
      props: { connection: NOTION, onRemove: () => {} },
    }).body;

    // THEN it names Notion and carries its remove button
    expect(html).toContain('data-testid="journal-filter-connection"');
    expect(html).toContain("Connection: Notion");
    expect(html).toContain('data-testid="journal-filter-connection-remove"');
    expect(html).toContain('aria-label="Remove the filter on Notion"');
  });

  test("the screen opens on the connection asked for, shows it, and removes it", () => {
    // GIVEN the source of the screen
    const source = read("JournalScreen.svelte");

    // WHEN its opening filters and its connection filter are read
    // THEN it takes the request on arrival, shows the filter while a connection
    // is set, and removing it drops the connection alone
    expect(source).toContain(
      "let filters = $state<JournalFilters>(openingFilters(takeRequestedJournalConnection()));",
    );
    expect(source).toContain("{#if filters.connection !== null}");
    expect(source).toContain("onRemove={() => (filters = withoutConnection(filters))}");
  });
});
