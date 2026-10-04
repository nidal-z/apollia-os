import { describe, test, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  CHAT_AUTO_NAMING_KEY,
  chatAutoNaming,
  chatAutoNamingSource,
  hydrateChatAutoNaming,
  setChatAutoNaming,
  type ChatAutoNamingIpc,
} from "./chatAutoNaming";
import type { ResolvedSetting } from "$lib/ipc/settings";

/** The shell, backed by an `apollia.toml` held in memory. */
function fakeShell(file?: string, imposed?: string) {
  const state = { file };
  const resolve = (): ResolvedSetting => {
    if (imposed !== undefined) return { key: CHAT_AUTO_NAMING_KEY, value: imposed, source: "policy" };
    return state.file === undefined
      ? { key: CHAT_AUTO_NAMING_KEY, value: "auto", source: "default" }
      : { key: CHAT_AUTO_NAMING_KEY, value: state.file, source: "user" };
  };
  const ipc: ChatAutoNamingIpc = {
    read: vi.fn(async () => resolve()),
    write: vi.fn(async (_key: string, value: unknown) => {
      if (imposed !== undefined) throw new Error("locked by the organization policy");
      state.file = value as string;
      return resolve();
    }),
  };
  return { ipc, state };
}

beforeEach(() => {
  chatAutoNaming.set("auto");
  chatAutoNamingSource.set("default");
});

describe("auto-naming setting", () => {
  test("a file without the key names automatically", async () => {
    // GIVEN an apollia.toml that does not name the choice
    const { ipc } = fakeShell();

    // WHEN the setting is seeded
    await hydrateChatAutoNaming(ipc);

    // THEN it reads auto, from the default, under [desktop]
    expect(get(chatAutoNaming)).toBe("auto");
    expect(get(chatAutoNamingSource)).toBe("default");
    expect(ipc.read).toHaveBeenCalledWith("desktop.chat_auto_naming");
  });

  test("a choice is written to the file and survives a restart", async () => {
    // GIVEN the default in force
    const { ipc, state } = fakeShell();

    // WHEN never is chosen, then the application restarts and seeds again
    const written = await setChatAutoNaming("never", ipc);
    chatAutoNaming.set("auto");
    await hydrateChatAutoNaming(ipc);

    // THEN the file holds never and the setting reads it back
    expect(written).toBe(true);
    expect(state.file).toBe("never");
    expect(get(chatAutoNaming)).toBe("never");
    expect(get(chatAutoNamingSource)).toBe("user");
  });

  test("a choice the policy imposes refuses a change", async () => {
    // GIVEN a policy imposing never
    const { ipc } = fakeShell(undefined, "never");
    await hydrateChatAutoNaming(ipc);

    // WHEN always is chosen anyway
    const written = await setChatAutoNaming("always", ipc);

    // THEN the write is refused and never stays, from the policy
    expect(written).toBe(false);
    expect(get(chatAutoNaming)).toBe("never");
    expect(get(chatAutoNamingSource)).toBe("policy");
  });
});
