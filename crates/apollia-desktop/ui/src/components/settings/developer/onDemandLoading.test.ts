import { describe, test, expect, vi, beforeEach } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  EXPERIMENTAL_KEYS,
  ON_DEMAND_KEYS,
  ON_DEMAND_ROWS,
  SEPARATE_REASONING_ROW,
  idleFieldDisabled,
  onDemandVisible,
  parseIdleMinutes,
} from "./onDemandLoading";
import {
  SERVER_SETTING_DEFAULTS,
  SERVER_SETTING_KEYS,
  hydrateServerSettings,
  serverSettings,
  setServerSetting,
  type ServerSettingsIpc,
} from "$lib/stores/serverSettings";
import type { ResolvedSetting } from "$lib/ipc/settings";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

const UI_SRC = resolve(__dirname, "../../..");
const KEY_REGISTRY = resolve(__dirname, "../../../../../../apollia-core/src/config/keys.rs");

function lookup(catalogue: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalogue,
    );
}

function text(catalogue: unknown, key: string): string {
  const value = lookup(catalogue, key);
  if (typeof value !== "string") throw new Error(`missing ${key}`);
  return value;
}

/** A shell holding one `apollia.toml` in memory, policy keys refused. */
function memoryShell(policy: Record<string, unknown> = {}) {
  const file = new Map<string, unknown>();
  const ipc: ServerSettingsIpc = {
    read: vi.fn(async (key: string): Promise<ResolvedSetting> => {
      if (key in policy) return { key, value: policy[key], source: "policy" };
      if (file.has(key)) return { key, value: file.get(key), source: "user" };
      return { key, value: SERVER_SETTING_DEFAULTS[key as keyof typeof SERVER_SETTING_DEFAULTS], source: "default" };
    }),
    write: vi.fn(async (key: string, value: unknown): Promise<ResolvedSetting> => {
      if (key in policy) throw new Error("locked by policy");
      file.set(key, value);
      return { key, value, source: "user" };
    }),
  };
  return { file, ipc };
}

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.(ts|svelte)$/.test(name) && !name.endsWith(".test.ts") ? [path] : [];
  });
}

beforeEach(async () => {
  // Every test starts from the defaults, as a fresh window would.
  await hydrateServerSettings(memoryShell().ipc);
});

describe("Settings > Developer: on-demand loading and idle delay", () => {
  test("the blocks show with the developer mode on and not with it off", () => {
    // GIVEN the two positions of the developer mode
    // WHEN the visibility of the blocks is asked for each
    // THEN they exist only in developer mode, absent otherwise
    expect(onDemandVisible(true)).toBe(true);
    expect(onDemandVisible(false)).toBe(false);
  });

  test("four rows, in the mockup's order, each explained, with LM Studio's defaults", () => {
    // GIVEN the rows of the block and a fresh store
    const settings = get(serverSettings);

    // WHEN they are read in order
    const keys = ON_DEMAND_ROWS.map((row) => row.key);

    // THEN loading, one at a time, auto-unload, then its delay, each with a
    // label and an explanation in both languages; on, on, on, 60 minutes
    expect(keys).toEqual([
      "server.jit_loading",
      "server.jit_keep_last_only",
      "server.jit_auto_unload",
      "server.jit_idle_ttl_minutes",
    ]);
    expect(keys.map((key) => settings[key].value)).toEqual([true, true, true, 60]);
    for (const row of ON_DEMAND_ROWS) {
      for (const catalogue of [fr, en]) {
        expect(text(catalogue, row.label).length).toBeGreaterThan(0);
        expect(text(catalogue, row.hint).length).toBeGreaterThan(0);
      }
      expect(row.testid).toMatch(/^developer-/);
    }
    expect(text(en, ON_DEMAND_KEYS.title)).toBe("On-demand loading and idle delay");
    expect(text(fr, ON_DEMAND_KEYS.title)).not.toBe(text(en, ON_DEMAND_KEYS.title));
    expect(text(en, ON_DEMAND_KEYS.unit)).toBe("minutes");
  });

  test("each key is the registry's, and no other source file names it", () => {
    // GIVEN the key registry of the engine and every source file of the UI
    const registry = readFileSync(KEY_REGISTRY, "utf8");
    const files = sourceFiles(UI_SRC);

    // WHEN each key the store writes is looked for
    for (const key of Object.values(SERVER_SETTING_KEYS)) {
      // THEN the registry declares it, and only the shared store spells it
      expect(registry).toContain(`key("${key}"`);
      const holders = files.filter((file) => readFileSync(file, "utf8").includes(`"${key}"`));
      expect(holders.map((file) => file.slice(UI_SRC.length))).toEqual([
        "/lib/stores/serverSettings.ts",
      ]);
    }
  });

  test("a value changed here is the value the server settings read, and the reverse", async () => {
    // GIVEN one apollia.toml behind the shell, read by both screens through
    // the one store
    const { file, ipc } = memoryShell();
    await hydrateServerSettings(ipc);

    // WHEN Settings > Developer turns auto-unload off and sets 15 minutes
    await setServerSetting(SERVER_SETTING_KEYS.jitAutoUnload, false, ipc);
    await setServerSetting(SERVER_SETTING_KEYS.jitIdleTtlMinutes, 15, ipc);

    // THEN the file carries both under their one key, and a screen that reads
    // the store again sees them
    expect(file.get("server.jit_auto_unload")).toBe(false);
    expect(file.get("server.jit_idle_ttl_minutes")).toBe(15);
    await hydrateServerSettings(ipc);
    expect(get(serverSettings)["server.jit_idle_ttl_minutes"]).toEqual({ value: 15, source: "user" });

    // WHEN the other screen writes the same key
    file.set("server.jit_idle_ttl_minutes", 30);
    await hydrateServerSettings(ipc);

    // THEN this one reads it
    expect(get(serverSettings)["server.jit_idle_ttl_minutes"].value).toBe(30);
  });

  test("a value the policy imposes locks the row and is never written", async () => {
    // GIVEN a policy that imposes on-demand loading off
    const { ipc } = memoryShell({ "server.jit_loading": false });
    await hydrateServerSettings(ipc);

    // WHEN the row is turned on
    const written = await setServerSetting(SERVER_SETTING_KEYS.jitLoading, true, ipc);

    // THEN nothing is written, and the row stays off, from the policy
    expect(written).toBe(false);
    expect(ipc.write).not.toHaveBeenCalled();
    expect(get(serverSettings)["server.jit_loading"]).toEqual({ value: false, source: "policy" });
    expect(idleFieldDisabled(true, true)).toBe(true);
  });

  test("a refused write puts the control back", async () => {
    // GIVEN a shell that refuses every write
    const ipc: ServerSettingsIpc = {
      read: vi.fn(async (key: string) => ({ key, value: true, source: "default" as const })),
      write: vi.fn(async () => {
        throw new Error("unreadable file");
      }),
    };

    // WHEN one-at-a-time is turned off
    const written = await setServerSetting(SERVER_SETTING_KEYS.jitKeepLastOnly, false, ipc);

    // THEN the write is reported failed and the switch is on again
    expect(written).toBe(false);
    expect(get(serverSettings)["server.jit_keep_last_only"].value).toBe(true);
  });

  test("the delay takes whole minutes, 0 included, and waits for auto-unload", () => {
    // GIVEN what a user may type
    // WHEN each is read
    // THEN whole minutes pass, anything else is refused
    expect(parseIdleMinutes("60")).toBe(60);
    expect(parseIdleMinutes(" 0 ")).toBe(0);
    for (const bad of ["", "-5", "1.5", "abc", "1e3"]) expect(parseIdleMinutes(bad)).toBeNull();
    expect(idleFieldDisabled(false, false)).toBe(true);
    expect(idleFieldDisabled(true, false)).toBe(false);
  });

  test("these delays say they are not the delay of the models the user loads", () => {
    // GIVEN the texts of auto-unload and of its delay
    // WHEN they are read in each language
    // THEN auto-unload names the general setting by its label, and the
    // delay's own label is a different one, about models loaded on demand
    expect(text(en, "settings.developer.on_demand.auto_unload_hint")).toContain(
      '"Unload an unused model after"',
    );
    expect(text(fr, "settings.developer.on_demand.auto_unload_hint")).toMatch(/«[^»]+»/);
    expect(text(en, "settings.developer.on_demand.idle_label")).toContain("loaded on demand");
    expect(text(fr, "settings.developer.on_demand.idle_label")).not.toBe(
      text(en, "settings.developer.on_demand.idle_label"),
    );
    for (const catalogue of [fr, en]) {
      expect(text(catalogue, "settings.developer.on_demand.idle_hint")).not.toBe(
        text(catalogue, "settings.developer.on_demand.auto_unload_hint"),
      );
    }
  });
});

describe("Settings > Developer: separate the reasoning in API responses", () => {
  test("the row is in Experimental, on by default, under its one key", () => {
    // GIVEN the experimental block and a fresh store
    const settings = get(serverSettings);

    // WHEN its row is read
    const row = SEPARATE_REASONING_ROW;

    // THEN it writes server.separate_reasoning_content, on, and says what off does
    expect(row.key).toBe("server.separate_reasoning_content");
    expect(settings[row.key]).toEqual({ value: true, source: "default" });
    expect(text(en, EXPERIMENTAL_KEYS.title)).toBe("Experimental");
    expect(text(fr, EXPERIMENTAL_KEYS.title).length).toBeGreaterThan(0);
    expect(text(en, row.label)).toBe("Separate reasoning_content and content in API responses");
    expect(text(fr, row.hint)).toContain("think");
    expect(ON_DEMAND_ROWS.map((r) => r.key)).not.toContain(row.key);
  });
});
