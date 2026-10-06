import { beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

import type { ResolvedSetting, SettingSource } from "$lib/ipc/settings";
import {
  CHAT_BEHAVIOUR_DEFAULTS,
  CHAT_BEHAVIOUR_KEYS,
  chatBehaviours,
  chatDoubleClickEdit,
  chatDoubleClickRename,
  chatRegenerateShortcut,
  chatSingleEmpty,
  hydrateChatBehaviours,
  setChatBehaviour,
  type ChatBehaviour,
  type ChatBehaviourKey,
  type ChatBehavioursIpc,
} from "./chatBehaviours";
import {
  CHAT_TOKEN_COUNT_DEFAULT,
  CHAT_TOKEN_COUNT_STORAGE_KEY,
  chatTokenCount,
  formatTokenCount,
  parseChatTokenCount,
} from "./chatTokenCount";

/** A shell holding `file`, the keys `policy` imposes refusing a write. */
function memoryShell(
  file: Record<string, unknown> = {},
  policy: Record<string, unknown> = {},
): { shell: ChatBehavioursIpc; file: Record<string, unknown>; writes: string[] } {
  const writes: string[] = [];
  const resolve = (key: string): ResolvedSetting => {
    if (key in policy) return { key, value: policy[key], source: "policy" };
    if (key in file) return { key, value: file[key], source: "user" };
    const fallback = CHAT_BEHAVIOUR_DEFAULTS[key as keyof typeof CHAT_BEHAVIOUR_DEFAULTS];
    return { key, value: fallback, source: "default" };
  };
  return {
    file,
    writes,
    shell: {
      read: async (key) => resolve(key),
      write: async (key, value) => {
        if (key in policy) throw new Error(`${key} is imposed by the policy`);
        writes.push(key);
        file[key] = value;
        return resolve(key);
      },
    },
  };
}

function reset(): void {
  const entries = Object.entries(CHAT_BEHAVIOUR_DEFAULTS).map(([key, value]) => [
    key,
    { value, source: "default" as SettingSource },
  ]);
  chatBehaviours.set(Object.fromEntries(entries) as Record<ChatBehaviourKey, ChatBehaviour>);
}

beforeEach(reset);

describe("chat behaviours - the values in force", () => {
  test("the defaults are the ones of the specification", () => {
    // GIVEN no value written by anyone
    // WHEN the defaults are read
    // THEN one empty conversation, several models loaded together, no
    // edition on a double click, renaming on a double click, the shortcut on
    expect(CHAT_BEHAVIOUR_DEFAULTS).toEqual({
      "desktop.chat_single_empty": true,
      "llm.residency.unload_before_load": false,
      "desktop.chat_double_click_edit": false,
      "desktop.chat_double_click_rename": true,
      "desktop.chat_regenerate_shortcut": true,
    });
    expect(get(chatSingleEmpty)).toBe(true);
    expect(get(chatDoubleClickEdit)).toBe(false);
    expect(get(chatDoubleClickRename)).toBe(true);
    expect(get(chatRegenerateShortcut)).toBe(true);
  });

  test("at launch every behaviour is read from apollia.toml, so a choice survives a restart", async () => {
    // GIVEN a file a previous session wrote each behaviour the other way in
    const { shell } = memoryShell({
      "desktop.chat_single_empty": false,
      "llm.residency.unload_before_load": true,
      "desktop.chat_double_click_edit": true,
      "desktop.chat_double_click_rename": false,
      "desktop.chat_regenerate_shortcut": false,
    });

    // WHEN the store is seeded, as the application does at launch
    await hydrateChatBehaviours(shell);

    // THEN each behaviour is the file's, from the user
    const all = get(chatBehaviours);
    expect(all[CHAT_BEHAVIOUR_KEYS.singleEmpty]).toEqual({ value: false, source: "user" });
    expect(all[CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad]).toEqual({ value: true, source: "user" });
    expect(get(chatDoubleClickEdit)).toBe(true);
    expect(get(chatDoubleClickRename)).toBe(false);
    expect(get(chatRegenerateShortcut)).toBe(false);
  });

  test("a switch writes its key through the shell and keeps what the shell answers", async () => {
    // GIVEN the defaults and an empty file
    const { shell, file, writes } = memoryShell();

    // WHEN unloading before loading is turned on
    const written = await setChatBehaviour(CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad, true, shell);

    // THEN the key is in the file, the engines will read it, and the store
    // holds the value as the user's
    expect(written).toBe(true);
    expect(writes).toEqual(["llm.residency.unload_before_load"]);
    expect(file["llm.residency.unload_before_load"]).toBe(true);
    expect(get(chatBehaviours)[CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad]).toEqual({
      value: true,
      source: "user",
    });
  });

  test("a behaviour the policy imposes is locked: the switch does not move and nothing is written", async () => {
    // GIVEN a policy that imposes one model at a time
    const { shell, writes } = memoryShell({}, { "llm.residency.unload_before_load": true });
    await hydrateChatBehaviours(shell);

    // WHEN the user turns it off
    const written = await setChatBehaviour(CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad, false, shell);

    // THEN the write is refused before reaching the shell, and the value in
    // force is still the policy's
    expect(written).toBe(false);
    expect(writes).toEqual([]);
    expect(get(chatBehaviours)[CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad]).toEqual({
      value: true,
      source: "policy",
    });
  });

  test("a write the shell refuses puts the switch back", async () => {
    // GIVEN a shell that cannot write the file
    const shell: ChatBehavioursIpc = {
      read: async (key) => ({ key, value: true, source: "default" }),
      write: async () => {
        throw new Error("apollia.toml is read-only");
      },
    };

    // WHEN the double click to rename is turned off
    const written = await setChatBehaviour(CHAT_BEHAVIOUR_KEYS.doubleClickRename, false, shell);

    // THEN the write failed and the behaviour is on again
    expect(written).toBe(false);
    expect(get(chatDoubleClickRename)).toBe(true);
  });

  test("an unreachable shell leaves the defaults in force", async () => {
    // GIVEN a shell that never answers a read
    const shell: ChatBehavioursIpc = {
      read: async () => {
        throw new Error("no shell");
      },
      write: async (key, value) => ({ key, value, source: "user" }),
    };

    // WHEN the store is seeded
    await hydrateChatBehaviours(shell);

    // THEN every behaviour keeps its default
    expect(get(chatSingleEmpty)).toBe(true);
    expect(get(chatDoubleClickEdit)).toBe(false);
  });
});

describe("token count in the list - a display preference", () => {
  test("off by default, and an unknown stored value falls back to off", () => {
    // GIVEN no stored value, then each value the storage can hold
    // WHEN it is parsed
    // THEN off unless it says on
    expect(CHAT_TOKEN_COUNT_DEFAULT).toBe(false);
    expect(parseChatTokenCount(null)).toBe(false);
    expect(parseChatTokenCount("off")).toBe(false);
    expect(parseChatTokenCount("yes")).toBe(false);
    expect(parseChatTokenCount("on")).toBe(true);
  });

  test("the choice is kept beside the theme, so it survives a restart", () => {
    // GIVEN the preference off
    chatTokenCount.set(false);

    // WHEN it is turned on
    chatTokenCount.set(true);

    // THEN the storage of the webview holds it
    expect(localStorage.getItem(CHAT_TOKEN_COUNT_STORAGE_KEY)).toBe("on");
    chatTokenCount.set(false);
    expect(localStorage.getItem(CHAT_TOKEN_COUNT_STORAGE_KEY)).toBe("off");
  });

  test("a count reads exact under a thousand, then in thousands and millions", () => {
    // GIVEN counts of three sizes, and values that are no count
    // WHEN each is worded for an English reader
    // THEN the short form of the language, and zero for what is no count
    expect(formatTokenCount(840, "en")).toBe("840");
    expect(formatTokenCount(1840, "en")).toBe("1.8K");
    expect(formatTokenCount(2_400_000, "en")).toBe("2.4M");
    expect(formatTokenCount(-5, "en")).toBe("0");
    expect(formatTokenCount(Number.NaN, "en")).toBe("0");
  });
});
