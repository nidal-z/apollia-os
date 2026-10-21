import { describe, test, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { createCatalogueSource, matchesCatalogueQuery } from "$lib/connections/catalogueSource";
import { isOfficial } from "$lib/connections/vendor";
import type { RegistryServerView } from "$lib/types";

function entry(over: Partial<RegistryServerView> = {}): RegistryServerView {
  return {
    name: "com.notion/mcp",
    title: "Notion",
    description: "Read and write the pages of a workspace",
    version: "1.0.0",
    repository_url: null,
    website_url: null,
    packages: null,
    trust_level: "verified_official",
    category: "productivity",
    enrichment: null,
    is_installed: false,
    remotes: [],
    ...over,
  };
}

const CURATED = [entry()];
const FULL = [entry(), entry({ name: "io.github.someone/weather", title: "Weather", trust_level: "community" })];

function callsTo(command: string): number {
  return invoke.mock.calls.filter(([name]) => name === command).length;
}

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation(async (command: string) => {
    if (command === "fetch_mcp_curated") return CURATED;
    if (command === "fetch_mcp_registry") return FULL;
    throw new Error(`unexpected command ${command}`);
  });
});

describe("opening the catalogue never asks the public catalogue", () => {
  test("the selection is read and the registry command is never invoked", async () => {
    // GIVEN the catalogue source of the Connections screen, over the real IPC
    const source = createCatalogueSource();

    // WHEN the catalogue is opened, twice
    const first = await source.selection();
    const second = await source.selection();

    // THEN the selection is shown and the registry command was called zero times
    expect(first).toEqual({ tier: "curated", selection: CURATED, publicEntries: [], failures: [] });
    expect(second.selection).toEqual(CURATED);
    expect(callsTo("fetch_mcp_registry")).toBe(0);
    expect(callsTo("fetch_mcp_curated")).toBe(1);
  });

  test("the Connections screen reaches the registry through Explore all only", () => {
    // GIVEN the source of the Connections screen and of its add window
    const route = readFileSync(new URL("../../routes/Connections.svelte", import.meta.url), "utf8");
    const windowSource = readFileSync(
      new URL("../../components/connections/catalogue/AddConnectionWindow.svelte", import.meta.url),
      "utf8",
    );

    // WHEN the calls that can reach the public catalogue are listed
    const exploreCalls = route.match(/catalogueSource\.exploreAll\(/g) ?? [];
    const openHandler = route.slice(
      route.indexOf("function handleOpenCatalogue"),
      route.indexOf("function handleConnect"),
    );

    // THEN no command is called directly, one handler explores, the opening
    //      handler does not, and only the Explore all button calls it
    expect(route).not.toMatch(/fetchMcpRegistry|fetch_mcp_registry/);
    expect(exploreCalls).toHaveLength(1);
    expect(route).toMatch(/onexploreall=\{handleExploreAll\}/);
    expect(openHandler).not.toMatch(/exploreAll|handleExploreAll/);
    expect(windowSource.match(/onclick=\{onexploreall\}/g) ?? []).toHaveLength(1);
  });
});

describe("Explore all is the one gesture that asks the public catalogue", () => {
  test("it asks once, then answers from what it read", async () => {
    // GIVEN an opened catalogue
    const source = createCatalogueSource();
    await source.selection();

    // WHEN Explore all is pressed twice
    const first = await source.exploreAll();
    const second = await source.exploreAll();

    // THEN the registry was asked exactly once and both show the public entries apart
    expect(callsTo("fetch_mcp_registry")).toBe(1);
    expect(first).toEqual({ tier: "full", selection: CURATED, publicEntries: [FULL[1]], failures: [] });
    expect(second.tier).toBe("full");
  });

  test("a failed read falls back to the selection and is tried again", async () => {
    // GIVEN a registry command that fails once
    invoke.mockImplementationOnce(async () => {
      throw new Error("offline");
    });
    const source = createCatalogueSource({
      curated: async () => CURATED,
      registry: () => invoke("fetch_mcp_registry"),
    });

    // WHEN Explore all is pressed, then pressed again
    const failed = await source.exploreAll();
    const retried = await source.exploreAll();

    // THEN the first shows the selection and names the failure, the second the full list
    expect(failed).toEqual({ tier: "curated", selection: CURATED, publicEntries: [], failures: ["public"] });
    expect(retried).toEqual({ tier: "full", selection: CURATED, publicEntries: [FULL[1]], failures: [] });
  });
});

describe("a source that does not answer is named, never shown as an empty catalogue", () => {
  test("a selection that cannot be read is a failure, not an empty selection", async () => {
    // GIVEN a selection command that fails, then answers
    const curated = vi.fn<() => Promise<RegistryServerView[]>>()
      .mockRejectedValueOnce(new Error("runtime unreachable"))
      .mockResolvedValue(CURATED);
    const source = createCatalogueSource({ curated, registry: async () => FULL });

    // WHEN the window opens, then opens again
    const failed = await source.selection();
    const retried = await source.selection();

    // THEN the first names the selection as failed, the second reads it
    expect(failed.failures).toEqual(["selection"]);
    expect(failed.selection).toEqual([]);
    expect(retried).toEqual({ tier: "curated", selection: CURATED, publicEntries: [], failures: [] });
  });

  test("a public catalogue that cannot be read is named beside the selection", async () => {
    // GIVEN a registry command that fails
    const source = createCatalogueSource({
      curated: async () => CURATED,
      registry: async () => {
        throw new Error("the registry did not answer");
      },
    });

    // WHEN Explore all is pressed
    const view = await source.exploreAll();

    // THEN the selection stays, the public catalogue is named as failed
    expect(view.selection).toEqual(CURATED);
    expect(view.failures).toEqual(["public"]);
  });
});

describe("the search narrows what is loaded", () => {
  const notion = entry();
  const weather = entry({
    name: "io.github.someone/weather",
    title: "Weather",
    description: "Weather forecasts for Z\u00fcrich",
    trust_level: "community",
    category: null,
  });
  const fromSelection = entry({
    name: "@modelcontextprotocol/server-memory",
    title: null,
    description: null,
    category: null,
    enrichment: {
      operator_label: { en: "Memory", fr: "M\u00e9moire" },
      category: "knowledge",
      icon_name: "brain",
      trust_level: "community_verified",
      auth_help_url: null,
      auth_help_text: null,
      auth_help_i18n_key: null,
      oauth_pre_registered_client_id_env: null,
    } as unknown as RegistryServerView["enrichment"],
  });

  test("an empty search keeps everything", () => {
    // GIVEN loaded entries WHEN the search is blank THEN all match
    expect([notion, weather, fromSelection].every((e) => matchesCatalogueQuery(e, "   "))).toBe(true);
  });

  test("every word must appear, case and accents ignored", () => {
    // GIVEN a selection entry and a registry entry
    const loaded = [notion, weather, fromSelection];

    // WHEN searched by label, by description without accents, by category, by two words
    const byLabel = loaded.filter((e) => matchesCatalogueQuery(e, "memory"));
    const byDescription = loaded.filter((e) => matchesCatalogueQuery(e, "FORECASTS zurich"));
    const byCategory = loaded.filter((e) => matchesCatalogueQuery(e, "productivity"));
    const nothing = loaded.filter((e) => matchesCatalogueQuery(e, "notion weather"));

    // THEN each narrows to the entries holding every word
    expect(byLabel).toEqual([fromSelection]);
    expect(byDescription).toEqual([weather]);
    expect(byCategory).toEqual([notion]);
    expect(nothing).toEqual([]);
  });

  test("a selection entry is found by its label in either language", () => {
    // GIVEN a selection entry labelled "Memory" and "M\u00e9moire"
    const loaded = [notion, weather, fromSelection];

    // WHEN searched by the French label, typed without its accent
    const found = loaded.filter((e) => matchesCatalogueQuery(e, "memoire"));

    // THEN the entry is found
    expect(found).toEqual([fromSelection]);
  });
});

describe("the official badge", () => {
  test("only verified_official is official, community_verified is community", () => {
    // GIVEN one entry per trust level
    // WHEN the badge is decided
    // THEN only verified_official is official
    expect(isOfficial(entry({ trust_level: "verified_official" }))).toBe(true);
    expect(isOfficial(entry({ trust_level: "community_verified" }))).toBe(false);
    expect(isOfficial(entry({ trust_level: "community" }))).toBe(false);
    expect(isOfficial(entry({ trust_level: "custom" }))).toBe(false);
  });
});
