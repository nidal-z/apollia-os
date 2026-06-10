import { describe, test, expect, beforeEach, vi } from "vitest";

// Mock the IPC wrapper so the store never touches the Tauri runtime in the node
// test env. `backendDefault` drives what `hydratePlanModeDefault()` reads.
let backendDefault = false;
let backendThrows = false;
vi.mock("$lib/ipc/planMode", () => ({
  getPlanModeDefault: vi.fn(async () => {
    if (backendThrows) throw new Error("runtime unavailable");
    return backendDefault;
  }),
}));

// Minimal localStorage shim - the default vitest env is `node`, but the store
// mirrors the "always plan" default into localStorage.
class MemoryStorage {
  private readonly s = new Map<string, string>();
  getItem(k: string) {
    return this.s.has(k) ? (this.s.get(k) as string) : null;
  }
  setItem(k: string, v: string) {
    this.s.set(k, v);
  }
  removeItem(k: string) {
    this.s.delete(k);
  }
  clear() {
    this.s.clear();
  }
  key(i: number) {
    return Array.from(this.s.keys())[i] ?? null;
  }
  get length() {
    return this.s.size;
  }
}
(globalThis as unknown as { localStorage: Storage }).localStorage =
  new MemoryStorage() as unknown as Storage;

import { get } from "svelte/store";
import { planModeDefault, hydratePlanModeDefault } from "./planModeSetting";

const STORAGE_KEY = "apollia-plan-mode-default";

beforeEach(() => {
  localStorage.clear();
  planModeDefault.set(false);
  backendDefault = false;
  backendThrows = false;
});

describe("planModeSetting store", () => {
  test("defaults to false when nothing is persisted", () => {
    // GIVEN no persisted value (cleared in beforeEach)
    // WHEN the store is read
    // THEN it is false
    expect(get(planModeDefault)).toBe(false);
  });

  test("mirrors the value into localStorage when set to true", () => {
    // GIVEN the default setting store
    // WHEN it is enabled
    planModeDefault.set(true);
    // THEN localStorage mirrors the value and the store reflects it
    expect(localStorage.getItem(STORAGE_KEY)).toBe("true");
    expect(get(planModeDefault)).toBe(true);
  });

  test("mirrors false back into localStorage when disabled", () => {
    // GIVEN an enabled setting
    planModeDefault.set(true);
    // WHEN it is disabled
    planModeDefault.set(false);
    // THEN localStorage mirrors the off value
    expect(localStorage.getItem(STORAGE_KEY)).toBe("false");
    expect(get(planModeDefault)).toBe(false);
  });
});

describe("hydratePlanModeDefault", () => {
  test("seeds the store from the backend when no local override exists", async () => {
    // GIVEN the backend config default is on and no local override
    backendDefault = true;
    localStorage.removeItem(STORAGE_KEY);
    // WHEN hydration runs
    await hydratePlanModeDefault();
    // THEN the store reflects the backend value
    expect(get(planModeDefault)).toBe(true);
  });

  test("keeps the local override and ignores the backend value", async () => {
    // GIVEN the user has a local override (off) and the backend default is on
    localStorage.setItem(STORAGE_KEY, "false");
    backendDefault = true;
    // WHEN hydration runs
    await hydratePlanModeDefault();
    // THEN the local override wins; the backend value is not applied
    expect(get(planModeDefault)).toBe(false);
  });

  test("leaves the store unchanged when the backend call fails", async () => {
    // GIVEN no local override and a failing backend
    localStorage.removeItem(STORAGE_KEY);
    backendThrows = true;
    planModeDefault.set(false);
    localStorage.removeItem(STORAGE_KEY);
    // WHEN hydration runs
    await hydratePlanModeDefault();
    // THEN the store keeps its current value (off)
    expect(get(planModeDefault)).toBe(false);
  });
});
