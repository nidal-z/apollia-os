import { describe, test, expect, vi, beforeAll } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
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

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
// The background service section anchors its help on a tooltip, which needs
// the provider the application root mounts. This page is rendered alone, and
// the section has its own tests, so it stays out of the render here.
vi.mock(
  "../../components/settings/developer/backgroundService",
  async (importOriginal) => ({
    ...(await importOriginal<
      typeof import("../../components/settings/developer/backgroundService")
    >()),
    backgroundServiceVisible: () => false,
  }),
);

import General from "./General.svelte";
import Developer from "./Developer.svelte";
import { developerMode } from "$lib/stores/developerMode";
import {
  ENGINE_LOG_LEVELS,
  engineLogLevel,
  hydrateEngineLogLevel,
  setEngineLogLevel,
  type EngineLogLevelIpc,
} from "$lib/stores/engineLogLevel";
import type { ResolvedSetting } from "$lib/ipc/settings";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** The labels of the options of the engine log level list, in order. */
function levelOptions(html: string): string[] {
  const start = html.indexOf('data-testid="engine-log-level-select"');
  const end = html.indexOf("</select>", start);
  const list = html.slice(start, end);
  return [...list.matchAll(/<option[^>]*>([^<]*)<\/option>/g)].map((m) =>
    m[1].trim(),
  );
}

beforeAll(async () => {
  await waitLocale();
});

describe("General - the logs folder follows the developer mode", () => {
  test("on, the row and its button are shown", () => {
    // GIVEN the developer mode on
    developerMode.set(true);

    // WHEN General renders
    const html = renderToString(General).body;

    // THEN the logs row and its button are there
    expect(tagOf(html, "general-logs")).not.toBeNull();
    expect(tagOf(html, "general-open-logs")).not.toBeNull();
  });

  test("off, the entry is absent", () => {
    // GIVEN the developer mode off
    developerMode.set(false);

    // WHEN General renders
    const html = renderToString(General).body;

    // THEN neither the row nor its button is rendered
    expect(tagOf(html, "general-logs")).toBeNull();
    expect(tagOf(html, "general-open-logs")).toBeNull();
  });

  test("the button names the folder in both languages", () => {
    // GIVEN the two catalogues
    // WHEN the button label is looked up
    // THEN English reads "Open logs folder", and French carries its own text
    expect(en.settings.general.logs_button).toBe("Open logs folder");
    expect(fr.settings.general.logs_button.length).toBeGreaterThan(0);
    expect(fr.settings.general.logs_button).not.toBe(
      en.settings.general.logs_button,
    );
  });
});

describe("Developer - the engine log level", () => {
  test("the list offers four levels, the default named so, with its explanation", async () => {
    // GIVEN the developer mode on, in French, the level at its default
    developerMode.set(true);
    engineLogLevel.set("debug");
    locale.set("fr");
    await waitLocale("fr");

    // WHEN Developer renders
    const html = renderToString(Developer).body;

    // THEN the list reads from the quietest to the default, debug, and the
    // explanation says when it applies and what is lost below debug
    expect(tagOf(html, "developer-engine-log-level")).not.toBeNull();
    const engines = fr.settings.developer.engines;
    expect(levelOptions(html)).toEqual([
      engines.level_error,
      engines.level_warning,
      engines.level_info,
      engines.level_debug,
    ]);
    expect(html).toContain(engines.log_level_label);
    expect(html).toContain(engines.log_level_hint);
    // AND the default is the debug level, named so in both languages
    expect(en.settings.developer.engines.level_debug).toBe("Debug (default)");
    expect(en.settings.developer.engines.log_level_hint).toContain(
      "Applies the next time a model is loaded or reloaded",
    );
    expect(engines.level_debug).not.toBe(
      en.settings.developer.engines.level_debug,
    );
    locale.set("en");
    await waitLocale("en");
  });

  test("off, the engine section is absent", () => {
    // GIVEN the developer mode off
    developerMode.set(false);

    // WHEN Developer renders
    const html = renderToString(Developer).body;

    // THEN the engine log level is not shown
    expect(tagOf(html, "developer-engine-log-level")).toBeNull();
  });
});

describe("engineLogLevel store", () => {
  function ipcAnswering(
    value: unknown,
  ): EngineLogLevelIpc & { written: unknown[] } {
    const written: unknown[] = [];
    return {
      written,
      read: async (key) => ({ key, value, source: "user" }) as ResolvedSetting,
      write: async (key, v) => {
        written.push(v);
        return { key, value: v, source: "user" } as ResolvedSetting;
      },
    };
  }

  test("hydration reads [engines] log_level, an unknown value reads as debug", async () => {
    // GIVEN a shell answering info, then one answering a level no engine has
    // WHEN the store is hydrated from each
    await hydrateEngineLogLevel(ipcAnswering("info"));
    let seen = "";
    engineLogLevel.subscribe((v) => (seen = v))();
    const fromInfo = seen;
    await hydrateEngineLogLevel(ipcAnswering("trace"));
    engineLogLevel.subscribe((v) => (seen = v))();

    // THEN the first is info and the second the default
    expect(fromInfo).toBe("info");
    expect(seen).toBe("debug");
    expect(ENGINE_LOG_LEVELS).toEqual(["error", "warning", "info", "debug"]);
  });

  test("a refused write puts the previous level back", async () => {
    // GIVEN the level at warning and a shell that refuses every write
    engineLogLevel.set("warning");
    const refusing: EngineLogLevelIpc = {
      read: async () => {
        throw new Error("unreachable");
      },
      write: async () => {
        throw new Error("locked by policy");
      },
    };

    // WHEN error is chosen
    const written = await setEngineLogLevel("error", refusing);

    // THEN the write is reported failed and the list is back on warning
    let seen = "";
    engineLogLevel.subscribe((v) => (seen = v))();
    expect(written).toBe(false);
    expect(seen).toBe("warning");
  });

  test("an accepted write sends the wire name", async () => {
    // GIVEN a shell that accepts the write
    const ipc = ipcAnswering("debug");

    // WHEN info is chosen
    const written = await setEngineLogLevel("info", ipc);

    // THEN the wire name went to the shell
    expect(written).toBe(true);
    expect(ipc.written).toEqual(["info"]);
  });
});
