/**
 * Where memory lives for a user without the developer mode: an agent's memory
 * on its sheet, what the agents retained about the user in Settings > Profile,
 * clearing every memory in the danger zone of General. The full Memory screen
 * (namespaces, types, search, import, export, purge) is a developer screen.
 *
 * Components cannot be mounted under this Vitest configuration (node
 * environment, no rendering library), so the routing helpers are driven
 * directly and the mounts are read from the sources the bundle is built from.
 */
import { describe, expect, test, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { DEFAULT_ROUTE, isDeveloperRoute, reachableRoute } from "./developerRoutes";

const SRC = resolve(__dirname, "../..");

function source(rel: string): string {
  return readFileSync(resolve(SRC, rel), "utf8");
}

describe("memory distribution - each function at its destination", () => {
  test("the full Memory screen is a developer screen, out of reach with the mode off", () => {
    // GIVEN the memory route
    const route = "memory" as const;

    // WHEN the router resolves it with the developer mode off, then on
    const off = reachableRoute(route, false);
    const on = reachableRoute(route, true);

    // THEN it is a developer route, replaced by the default one with the mode
    // off and rendered with the mode on
    expect(isDeveloperRoute(route)).toBe(true);
    expect(off).toBe(DEFAULT_ROUTE);
    expect(on).toBe("memory");
  });

  test("import, export and purge stay on the Memory screen alone", () => {
    // GIVEN the Memory screen and the agent sheet
    const screen = source("routes/Memory.svelte");
    const sheet = source("components/agents/sheet/AgentSheet.svelte");

    // WHEN the data actions component is looked up
    // THEN only the developer screen mounts it
    expect(screen).toContain("<NamespaceDataActions");
    expect(sheet).not.toContain("NamespaceDataActions");
  });

  test("an agent's memory is read, searched, pruned and cleared on its sheet", () => {
    // GIVEN the agent sheet and its memory section
    const sheet = source("components/agents/sheet/AgentSheet.svelte");
    const memory = source("components/agents/sheet/SheetMemory.svelte");

    // WHEN the section and its calls are located
    // THEN the sheet mounts it outside any developer branch, and it lists,
    // searches, deletes one entry and clears the namespace
    expect(sheet).toMatch(/\n  <SheetMemory namespace=\{memoryNamespace\} \/>/);
    for (const call of ["listMemoryEntries(", "searchMemory(", "deleteMemoryEntry(", "clearMemory("]) {
      expect(memory).toContain(call);
    }
    expect(memory).not.toContain("developerMode");
  });

  test("what the agents retained about the user is read and deleted in Settings > Profile", () => {
    // GIVEN the Profile section and its facts row
    const profile = source("routes/settings/Profile.svelte");
    const row = source("components/settings/profile/ProfileFactRow.svelte");

    // WHEN the facts card and the deletion are located
    // THEN the card is mounted for every user and each row forgets its fact
    expect(profile).toContain("<ProfileMemoryFactsCard");
    expect(profile).toContain("facts={detail.facts}");
    expect(profile).not.toContain("{#if $developerMode}");
    expect(row).toContain('data-testid="profile-fact-delete"');
    expect(row).toContain("deleteProfileFact(profileId, key)");
  });

  test("clearing every memory sits in the danger zone of General", () => {
    // GIVEN the General section and the danger zone
    const general = source("routes/settings/General.svelte");
    const danger = source("components/settings/DangerZone.svelte");

    // WHEN the clear-memories action is located
    // THEN General mounts the zone, which carries the action behind its
    // confirmation word
    expect(general).toContain("<DangerZone />");
    expect(danger).toMatch(/id: "clear-memories", key: "clear_memories", icon: Brain, confirmWord: "DELETE"/);
  });
});
