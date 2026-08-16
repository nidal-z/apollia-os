import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { buildBreadcrumbSegments, settingsSubRouteLabelKey } from "$lib/navigation/breadcrumbSegments";
import { SETTINGS_SUB_ROUTES } from "$lib/stores/settings";

/** Resolve a dotted key ("settings.nav.danger") inside a parsed catalogue. */
function resolveKey(catalogue: Record<string, unknown>, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalogue,
    );
}

function loadCatalogue(locale: "fr" | "en"): Record<string, unknown> {
  const url = new URL(`../i18n/${locale}.json`, import.meta.url);
  return JSON.parse(readFileSync(url, "utf-8")) as Record<string, unknown>;
}

describe("breadcrumbSegments - settings sub-page trail", () => {
  test.each(SETTINGS_SUB_ROUTES)(
    "settings trail ends with the label of the open sub-page (%s)",
    (sub) => {
      // GIVEN the settings route with the sub-route store on the given sub-page
      // WHEN the breadcrumb segments are built
      const segments = buildBreadcrumbSegments("settings", sub);

      // THEN the trail is "settings" then the sub-page, in that order
      expect(segments.map((s) => s.labelKey)).toEqual([
        "nav.settings",
        settingsSubRouteLabelKey(sub),
      ]);
    },
  );

  test.each(SETTINGS_SUB_ROUTES)(
    "the sub-page leaf label key exists in both catalogues (%s)",
    (sub) => {
      // GIVEN the label key the breadcrumb leaf renders for this sub-page
      const key = settingsSubRouteLabelKey(sub);

      // WHEN the key is resolved in each catalogue
      const fr = resolveKey(loadCatalogue("fr"), key);
      const en = resolveKey(loadCatalogue("en"), key);

      // THEN both catalogues carry a non-empty string for it
      expect(typeof fr, `${key} missing in fr.json`).toBe("string");
      expect(typeof en, `${key} missing in en.json`).toBe("string");
      expect((fr as string).length).toBeGreaterThan(0);
      expect((en as string).length).toBeGreaterThan(0);
    },
  );

  test("the sub-page leaf is not a navigation target, its parent routes to settings", () => {
    // GIVEN the settings route on the permissions sub-page
    // WHEN the breadcrumb segments are built
    const segments = buildBreadcrumbSegments("settings", "permissions");

    // THEN the parent segment navigates to settings and the leaf navigates nowhere
    expect(segments[0]?.route).toBe("settings");
    expect(segments[1]?.route).toBeNull();
  });
});

describe("breadcrumbSegments - non-settings routes", () => {
  test("a flat route keeps a single segment", () => {
    // GIVEN a flat route while the settings store points at some sub-page
    // WHEN the breadcrumb segments are built
    const segments = buildBreadcrumbSegments("agents", "danger");

    // THEN the trail is the route alone, with no settings leaf appended
    expect(segments.map((s) => s.labelKey)).toEqual(["nav.agents"]);
    expect(segments[0]?.route).toBe("agents");
  });

  test("a nested design route keeps its registry trail untouched", () => {
    // GIVEN a route that already has a parent in the registry
    // WHEN the breadcrumb segments are built
    const segments = buildBreadcrumbSegments("design-motion", "appearance");

    // THEN the trail is the registry parent chain, with no settings leaf appended
    expect(segments.map((s) => s.labelKey)).toEqual([
      "topbar.route.design",
      "topbar.route.design_motion",
    ]);
  });
});
