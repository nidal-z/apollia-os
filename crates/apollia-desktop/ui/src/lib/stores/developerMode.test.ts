import { describe, test, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  DEVELOPER_MODE_KEY,
  developerMode,
  developerModeSource,
  hydrateDeveloperMode,
  setDeveloperMode,
  type DeveloperModeIpc,
} from "./developerMode";
import { uiMode } from "./mode";
import type { ResolvedSetting } from "$lib/ipc/settings";

const LEGACY_KEY = "apollia-ui-mode";

/** A browser storage held in memory. */
function memoryStorage(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    removeItem: (k: string) => void map.delete(k),
    has: (k: string) => map.has(k),
  };
}

/**
 * The shell, backed by an `apollia.toml` held in memory: `file` is what the
 * file carries for the developer mode, `undefined` when the key is absent.
 */
function fakeShell(file?: boolean, refuseWrites = false) {
  const state = { file };
  const resolve = (): ResolvedSetting =>
    state.file === undefined
      ? { key: DEVELOPER_MODE_KEY, value: false, source: "default" }
      : { key: DEVELOPER_MODE_KEY, value: state.file, source: "user" };
  const ipc: DeveloperModeIpc = {
    read: vi.fn(async () => resolve()),
    write: vi.fn(async (_key: string, value: unknown) => {
      if (refuseWrites) throw new Error("setting is locked by the organization policy");
      state.file = value === true;
      return resolve();
    }),
  };
  return { ipc, state };
}

beforeEach(() => {
  developerMode.set(false);
  developerModeSource.set("default");
});

describe("developerMode, read from apollia.toml", () => {
  test("without the key, the developer mode is off", async () => {
    // GIVEN an apollia.toml with no [desktop] developer_mode, and no old choice
    const { ipc } = fakeShell(undefined);

    // WHEN the store is seeded at launch
    await hydrateDeveloperMode(ipc, memoryStorage());

    // THEN the developer mode is off, from the default
    expect(get(developerMode)).toBe(false);
    expect(get(developerModeSource)).toBe("default");
    expect(ipc.read).toHaveBeenCalledWith(DEVELOPER_MODE_KEY);
  });

  test("a key written on reads back on", async () => {
    // GIVEN an apollia.toml that carries developer_mode = true
    const { ipc } = fakeShell(true);

    // WHEN the store is seeded at launch
    await hydrateDeveloperMode(ipc, memoryStorage());

    // THEN the developer mode is on, from the user's file
    expect(get(developerMode)).toBe(true);
    expect(get(developerModeSource)).toBe("user");
  });

  test("turning it on writes the key through the shell", async () => {
    // GIVEN the developer mode off
    const { ipc, state } = fakeShell(undefined);

    // WHEN the switch is turned on
    const written = await setDeveloperMode(true, ipc);

    // THEN the file carries the key on, and the store follows
    expect(written).toBe(true);
    expect(state.file).toBe(true);
    expect(get(developerMode)).toBe(true);
  });

  test("a refused write puts the switch back", async () => {
    // GIVEN a shell that refuses every write (a value the policy imposes)
    const { ipc } = fakeShell(undefined, true);

    // WHEN the switch is turned on
    const written = await setDeveloperMode(true, ipc);

    // THEN the write is reported refused and the mode stays off
    expect(written).toBe(false);
    expect(get(developerMode)).toBe(false);
  });
});

describe("developerMode, one-time migration of the former interface mode", () => {
  test("a former Builder choice turns the developer mode on, then is forgotten", async () => {
    // GIVEN the old browser key set to "builder" and no key in apollia.toml
    const storage = memoryStorage({ [LEGACY_KEY]: "builder" });
    const { ipc, state } = fakeShell(undefined);

    // WHEN the application opens for the first time
    await hydrateDeveloperMode(ipc, storage);

    // THEN the developer mode is on, written to the file, and the old key is gone
    expect(get(developerMode)).toBe(true);
    expect(state.file).toBe(true);
    expect(storage.has(LEGACY_KEY)).toBe(false);
  });

  test("a former Operator choice leaves the developer mode off, then is forgotten", async () => {
    // GIVEN the old browser key set to "operator"
    const storage = memoryStorage({ [LEGACY_KEY]: "operator" });
    const { ipc, state } = fakeShell(undefined);

    // WHEN the application opens for the first time
    await hydrateDeveloperMode(ipc, storage);

    // THEN the developer mode is off, nothing is written, and the old key is gone
    expect(get(developerMode)).toBe(false);
    expect(state.file).toBeUndefined();
    expect(ipc.write).not.toHaveBeenCalled();
    expect(storage.has(LEGACY_KEY)).toBe(false);
  });

  test("no former choice leaves the developer mode off and writes nothing", async () => {
    // GIVEN no old browser key
    const storage = memoryStorage();
    const { ipc, state } = fakeShell(undefined);

    // WHEN the application opens
    await hydrateDeveloperMode(ipc, storage);

    // THEN the developer mode is off and the file is untouched
    expect(get(developerMode)).toBe(false);
    expect(state.file).toBeUndefined();
    expect(ipc.write).not.toHaveBeenCalled();
  });

  test("a refused migration keeps the old key for the next launch", async () => {
    // GIVEN the old key set to "builder" and a shell that refuses the write
    const storage = memoryStorage({ [LEGACY_KEY]: "builder" });
    const { ipc } = fakeShell(undefined, true);

    // WHEN the application opens
    await hydrateDeveloperMode(ipc, storage);

    // THEN the mode reads from the file, and the old key stays
    expect(get(developerMode)).toBe(false);
    expect(storage.has(LEGACY_KEY)).toBe(true);
  });
});

describe("uiMode, adapter of the developer mode", () => {
  test("reads builder when the developer mode is on, operator otherwise", () => {
    // GIVEN the developer mode off, then on
    // WHEN the adapter is read
    // THEN it names the matching former mode
    expect(get(uiMode)).toBe("operator");
    developerMode.set(true);
    expect(get(uiMode)).toBe("builder");
  });

  test("writing builder turns the developer mode on", () => {
    // GIVEN the developer mode off
    // WHEN a reader writes "builder" through the adapter
    uiMode.set("builder");

    // THEN the developer mode is on at once
    expect(get(developerMode)).toBe(true);
  });
});
