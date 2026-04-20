/**
 * Navigation module tests (US-SP42-079).
 *
 * Verifies the structural invariants the sidebar relies on:
 *   - Both mode navigations share the same `NavGroup[]` shape.
 *   - Operator nav is split into exactly 2 groups (Core, Collaboration).
 *   - All routes referenced by nav entries are non-empty strings, so a
 *     typo does not silently produce a dead nav link.
 *   - Group i18n keys follow the `nav.*` / `sidebar.group.*` conventions
 *     the sidebar uses to look up labels.
 */
import { describe, expect, it } from "vitest";
import { operatorGroups } from "./operatorNav";
import { builderGroups } from "./builderNav";

describe("operatorGroups", () => {
  it("is split into Core + Collaboration", () => {
    expect(operatorGroups.map((g) => g.labelKey)).toEqual([
      "sidebar.group.core",
      "sidebar.group.collaboration",
    ]);
  });

  it("includes projects in Core and inbox in Collaboration", () => {
    const [core, collab] = operatorGroups;
    expect(core.items.map((i) => i.route)).toContain("projects");
    expect(collab.items.map((i) => i.route)).toContain("inbox");
  });

  it("never produces a duplicate route across groups", () => {
    const routes = operatorGroups.flatMap((g) => g.items.map((i) => i.route));
    expect(new Set(routes).size).toBe(routes.length);
  });
});

describe("builderGroups", () => {
  it("preserves the 3 historical groups (operations/infrastructure/data)", () => {
    expect(builderGroups.map((g) => g.labelKey)).toEqual([
      "nav.operations",
      "nav.infrastructure",
      "nav.data",
    ]);
  });

  it("every entry declares a non-empty route and labelKey", () => {
    for (const group of builderGroups) {
      for (const item of group.items) {
        expect(item.route.length).toBeGreaterThan(0);
        expect(item.labelKey.length).toBeGreaterThan(0);
      }
    }
  });
});
