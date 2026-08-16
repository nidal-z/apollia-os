import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";

/**
 * Every member of the `Route` union designates a rendered surface.
 *
 * The registry once promised nineteen surfaces and the render switch held
 * eighteen: `"onboarding"` was a `Route` member and a `routeMeta` entry while
 * onboarding is a modal (`OnboardingModal`), not a route. A guide deep-link
 * button resolving `/onboarding` then landed on the fallback branch instead of
 * a surface of its own. This suite crosses the union declaration against the
 * render branches of `Main.svelte` so the gap cannot reopen silently.
 */

const here = path.dirname(fileURLToPath(import.meta.url));
const uiSrc = path.resolve(here, "..", "..");

function routeUnionMembers(): string[] {
  const source = readFileSync(path.join(uiSrc, "lib", "stores", "navigation.ts"), "utf8");
  const union = source.match(/export type Route =([^;]+);/);
  if (!union) throw new Error("Route union not found in lib/stores/navigation.ts");
  return [...union[1].matchAll(/"([\w-]+)"/g)].map((m) => m[1]);
}

function mainSvelteRenderBranches(): string[] {
  const source = readFileSync(
    path.join(uiSrc, "lib", "components", "app", "Main.svelte"),
    "utf8",
  );
  return [...source.matchAll(/\$currentRoute === "([\w-]+)"/g)].map((m) => m[1]);
}

describe("route registry completeness", () => {
  it("crosses the Route union against Main.svelte: zero members without a render branch", () => {
    // GIVEN the Route union members declared in lib/stores/navigation.ts
    const members = routeUnionMembers();
    expect(members.length).toBeGreaterThan(0);

    // WHEN crossed against the render branches of Main.svelte
    const branches = new Set(mainSvelteRenderBranches());
    const withoutBranch = members.filter((member) => !branches.has(member));

    // THEN no member is left without a rendered surface
    expect(withoutBranch).toEqual([]);
  });

  it("crosses the Route union against the routeMeta registry: same members, both ways", () => {
    // GIVEN the Route union and the routeMeta registry keys
    const members = routeUnionMembers();
    const source = readFileSync(path.join(here, "routeMeta.ts"), "utf8");
    const body = source.match(/export const routeMeta[^=]+= \{([\s\S]+?)\n\};/);
    if (!body) throw new Error("routeMeta literal not found in routeMeta.ts");
    const keys = [...body[1].matchAll(/^ {2}"?([\w-]+)"?: \{/gm)].map((m) => m[1]);

    // WHEN compared as sets
    const membersSet = new Set(members);
    const keysSet = new Set(keys);

    // THEN the registry declares exactly the union, no more, no less
    expect(keys.filter((k) => !membersSet.has(k))).toEqual([]);
    expect(members.filter((m) => !keysSet.has(m))).toEqual([]);
  });
});
