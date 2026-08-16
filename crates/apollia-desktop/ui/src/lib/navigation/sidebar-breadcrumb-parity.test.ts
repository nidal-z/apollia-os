import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

// The sidebar reads `nav.sidebar.*` keys, the breadcrumb reads the `routeMeta`
// registry whose entries point at `nav.*` keys. Both surfaces are visible at
// the same time on every screen, so for every route present on both sides the
// two catalogs must resolve to the same string. Before this test existed the
// FR catalog diverged on 4 routes (agents, dashboard, llm, tasks) and the EN
// catalog on 3 (dashboard, llm, tasks).

type JsonObject = Record<string, unknown>;

function resolve(catalog: JsonObject, dottedKey: string): string | undefined {
  let node: unknown = catalog;
  for (const part of dottedKey.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as JsonObject)[part];
  }
  return typeof node === "string" ? node : undefined;
}

function readSource(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), "utf-8");
}

const routeMetaSource = readSource("./routeMeta.ts");
const sidebarSource = readSource("../components/app/Sidebar.svelte");

const breadcrumbKeys = new Map<string, string>(
  [...routeMetaSource.matchAll(/(\w+):\s*\{\s*labelKey:\s*"([^"]+)"/g)].map((m) => [m[1], m[2]]),
);
const sidebarKeys = new Map<string, string>(
  [...sidebarSource.matchAll(/route:\s*"(\w+)"\s*as Route.*?labelKey:\s*"([^"]+)"/g)].map((m) => [
    m[1],
    m[2],
  ]),
);

const commonRoutes = [...sidebarKeys.keys()].filter((route) => breadcrumbKeys.has(route)).sort();

const catalogs: Array<[string, JsonObject]> = [
  ["fr", fr as JsonObject],
  ["en", en as JsonObject],
];

describe("sidebar and breadcrumb name every common route identically", () => {
  test("the extraction still sees both surfaces", () => {
    // GIVEN the sidebar item list and the routeMeta registry
    // WHEN their route/labelKey pairs are extracted
    // THEN the crossing covers the 14 routes both surfaces declare, so a
    // refactor that blinds the extraction fails here instead of passing silently
    expect(commonRoutes).toEqual([
      "agents",
      "automations",
      "chat",
      "dashboard",
      "inbox",
      "integrations",
      "llm",
      "memory",
      "notifications",
      "observability",
      "projects",
      "settings",
      "tasks",
      "transcriptions",
    ]);
  });

  test.each(catalogs)("no divergent route label in %s", (_name, catalog) => {
    // GIVEN one catalog and the two key families the co-visible surfaces read
    // WHEN each common route is resolved through both families
    // THEN the sidebar string and the breadcrumb string are identical
    const divergent = commonRoutes
      .map((route) => ({
        route,
        sidebar: resolve(catalog, sidebarKeys.get(route) as string),
        breadcrumb: resolve(catalog, breadcrumbKeys.get(route) as string),
      }))
      .filter((entry) => entry.sidebar !== entry.breadcrumb);
    expect(divergent).toEqual([]);
  });

  test.each(catalogs)("the Operator page title of the llm route is the navigation string in %s", (_name, catalog) => {
    // GIVEN the llm screen title in Operator mode (routes/Llm.svelte reads
    // `llm.title_operator`) and the navigation string of the llm route
    // WHEN both are resolved from the same catalog
    // THEN the screen carries one name, not three
    const navigationLabel = resolve(catalog, breadcrumbKeys.get("llm") as string);
    expect(resolve(catalog, "llm.title_operator")).toBe(navigationLabel);
  });
});
