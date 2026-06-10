import { describe, test, expect, beforeEach } from "vitest";

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
import { planModeDefault } from "./planModeSetting";

const STORAGE_KEY = "apollia-plan-mode-default";

beforeEach(() => {
  localStorage.clear();
  planModeDefault.set(false);
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
