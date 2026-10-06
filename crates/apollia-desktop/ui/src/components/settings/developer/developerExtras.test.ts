import { describe, test, expect, vi, beforeAll, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render as renderToString } from "svelte/server";
import { get, writable } from "svelte/store";
import { waitLocale } from "svelte-i18n";
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

const ipc = vi.hoisted(() => ({
  invoke: vi.fn(async (_command: string, _args?: unknown): Promise<unknown> => null),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(async () => false),
  requestPermission: vi.fn(async () => "denied"),
  sendNotification: vi.fn(),
}));

// The background service section anchors its help on a tooltip, which needs
// the provider the application root mounts. This page is rendered alone, and
// the section has its own tests, so it stays out of the render here.
vi.mock("./backgroundService", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./backgroundService")>()),
  backgroundServiceVisible: () => false,
}));

import Developer from "../../../routes/settings/Developer.svelte";
import ExecutionTrace from "../../observability/ExecutionTrace.svelte";
import Sidebar from "$lib/components/app/Sidebar.svelte";
import ResourceWidget, {
  widgetGauges,
  widgetTone,
} from "$lib/components/app/ResourceWidget.svelte";
import {
  AUTO_PURGE_ROW,
  DEVELOPER_ENGINES_KEYS,
  EXPERIMENTAL_ROWS,
  EXPERIMENTAL_WRITE_FAILED,
} from "./developerExtras";
import {
  DEVELOPER_SETTING_DEFAULTS,
  DEVELOPER_SETTING_KEYS,
  chatDebugBlocks,
  debugBlocksShown,
  developerSettings,
  hydrateDeveloperSettings,
  resourceWidgetShown,
  resourceWidgetVisible,
  setDeveloperSetting,
  type DeveloperSettingKey,
  type DeveloperSettingsIpc,
} from "$lib/stores/developerSettings";
import { developerMode } from "$lib/stores/developerMode";
import { enginesStore } from "$lib/stores/settings";
import {
  ENGINE_AUTO_UPDATE_KEY,
  DEFAULT_ENGINE_SETTINGS,
  engineSettings,
  setEngineAutoUpdate,
} from "$lib/settings/engineSelection";
import { createResourceMonitor, whileShown } from "$lib/settings/resourceMonitor";
import { settingsSubRoute } from "$lib/stores/settings";
import { openSettingsWindow, settingsWindowOpen } from "$lib/stores/settingsWindow";
import type { ResolvedSetting } from "$lib/ipc/settings";
import type { EnginesResponse, ResourceUsage } from "$lib/ipc/system";
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

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** The markup from the element `testid` up to the element `until`, or the end. */
function slice(html: string, testid: string, until?: string): string {
  const start = html.indexOf(`data-testid="${testid}"`);
  if (start < 0) return "";
  const end = until ? html.indexOf(`data-testid="${until}"`, start) : -1;
  return end < 0 ? html.slice(start) : html.slice(start, end);
}

function loaded<T>(data: T) {
  return { data, loading: false, error: null, loaded: true };
}

/** A shell holding one `apollia.toml` in memory, policy keys refused. */
function memoryShell(policy: Record<string, unknown> = {}) {
  const file = new Map<string, unknown>();
  const shell: DeveloperSettingsIpc = {
    read: vi.fn(async (key: string): Promise<ResolvedSetting> => {
      if (key in policy) return { key, value: policy[key], source: "policy" };
      if (file.has(key)) return { key, value: file.get(key), source: "user" };
      return {
        key,
        value: DEVELOPER_SETTING_DEFAULTS[key as DeveloperSettingKey],
        source: "default",
      };
    }),
    write: vi.fn(async (key: string, value: unknown): Promise<ResolvedSetting> => {
      if (key in policy) throw new Error("locked by policy");
      file.set(key, value);
      return { key, value, source: "user" };
    }),
  };
  return { shell, file };
}

function resetSettings(): void {
  developerSettings.set(
    Object.fromEntries(
      Object.values(DEVELOPER_SETTING_KEYS).map((key) => [
        key,
        { value: DEVELOPER_SETTING_DEFAULTS[key], source: "default" },
      ]),
    ) as Parameters<typeof developerSettings.set>[0],
  );
}

const USAGE: ResourceUsage = {
  ram_used_gb: 9.4,
  ram_total_gb: 32,
  cpu_percent: 18,
  vram: { kind: "dedicated", used_gb: 5.1, total_gb: 8 },
};

const ENGINES = { channel: "preview", engines: [], updates: [] } as unknown as EnginesResponse;

beforeAll(async () => {
  await waitLocale();
});

beforeEach(() => {
  developerMode.set(true);
  resetSettings();
  engineSettings.set(DEFAULT_ENGINE_SETTINGS);
  enginesStore.set(loaded(ENGINES));
  ipc.invoke.mockReset();
  ipc.invoke.mockImplementation(async () => null);
});

afterEach(() => {
  vi.useRealTimers();
  developerMode.set(false);
});

describe("Developer > Engines - the four rows, with the developer mode on", () => {
  test("on, the block holds the log level, the channel, the automatic update and the automatic purge, in that order", () => {
    // GIVEN the developer mode on, engines following the preview channel
    // WHEN Developer renders
    const html = renderToString(Developer).body;
    const block = slice(html, "developer-engines", "developer-experimental");

    // THEN the four rows are there in reading order, the channel on the value
    // of the engine store, the purge switch off by default
    const order = [
      "engine-log-level-select",
      "developer-engine-channel-select",
      "developer-engine-auto-update-switch",
      "developer-engine-auto-purge-switch",
    ].map((testid) => block.indexOf(`data-testid="${testid}"`));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    expect(block).toContain(fr.settings.engines.auto_update_label);
    expect(block).toContain(text(fr, AUTO_PURGE_ROW.label));
    expect(block).toContain(text(fr, AUTO_PURGE_ROW.hint));
    expect(tagOf(block, "developer-engine-auto-update-switch")).toMatch(/aria-checked="true"/);
    expect(tagOf(block, "developer-engine-auto-purge-switch")).toMatch(/aria-checked="false"/);
    const channel = slice(block, "developer-engine-channel-select", "developer-engine-auto-update");
    expect(channel).toMatch(/<option[^>]*value="preview"[^>]*selected/);
  });

  test("off, the block is absent", () => {
    // GIVEN the developer mode off
    developerMode.set(false);

    // WHEN Developer renders
    const html = renderToString(Developer).body;

    // THEN neither the engines nor the experimental block is rendered
    expect(tagOf(html, "developer-engines")).toBeNull();
    expect(tagOf(html, "developer-experimental")).toBeNull();
  });

  test("the automatic update is the key System > Engines writes", async () => {
    // GIVEN the store System > Engines reads, automatic update on
    ipc.invoke.mockImplementation(async (_command: string, args?: unknown) => {
      const { key, value } = args as { key: string; value: unknown };
      return { key, value, source: "user" };
    });

    // WHEN the switch of Developer turns it off, by the call its row makes
    const written = await setEngineAutoUpdate(false);
    const html = renderToString(Developer).body;

    // THEN `engines.auto_update` was written once through the shell, and
    // both screens read the value from the one store
    expect(written).toBe(true);
    expect(ipc.invoke).toHaveBeenCalledWith("set_setting", {
      key: ENGINE_AUTO_UPDATE_KEY,
      value: false,
    });
    expect(ENGINE_AUTO_UPDATE_KEY).toBe("engines.auto_update");
    expect(get(engineSettings).autoUpdate.value).toBe(false);
    expect(tagOf(html, "developer-engine-auto-update-switch")).toMatch(/aria-checked="false"/);
    const section = readFileSync(
      resolve(UI_SRC, "components/settings/developer/DeveloperEnginesSection.svelte"),
      "utf8",
    );
    expect(section).toContain("setEngineAutoUpdate");
    expect(section).toContain("changeEngineChannel");
  });

  test("turning the purge on writes engines.auto_purge, and the policy locks it", async () => {
    // GIVEN a shell with nothing written, and one where the policy imposes
    // the purge off
    const free = memoryShell();
    const locked = memoryShell({ [AUTO_PURGE_ROW.key]: false });

    // WHEN the switch is turned on in the first, then hydrated and turned on
    // in the second
    const written = await setDeveloperSetting(AUTO_PURGE_ROW.key, true, free.shell);
    const freeValue = get(developerSettings)[AUTO_PURGE_ROW.key];
    await hydrateDeveloperSettings(locked.shell);
    const refused = await setDeveloperSetting(AUTO_PURGE_ROW.key, true, locked.shell);
    const html = renderToString(Developer).body;

    // THEN the first wrote the key, the second wrote nothing and the row is
    // disabled and names the policy
    expect(written).toBe(true);
    expect(free.file.get("engines.auto_purge")).toBe(true);
    expect(freeValue).toEqual({ value: true, source: "user" });
    expect(refused).toBe(false);
    expect(locked.shell.write).not.toHaveBeenCalled();
    expect(get(developerSettings)[AUTO_PURGE_ROW.key]).toEqual({ value: false, source: "policy" });
    expect(tagOf(html, "developer-engine-auto-purge-switch")).toMatch(/disabled/);
    expect(tagOf(html, "developer-engine-auto-purge-policy")).not.toBeNull();
  });

  test("every text of the block exists in French and in English, apart", () => {
    // GIVEN the texts the block names
    const keys = [
      ...Object.values(DEVELOPER_ENGINES_KEYS),
      AUTO_PURGE_ROW.label,
      AUTO_PURGE_ROW.hint,
    ];

    // WHEN each is looked up in both catalogues
    // THEN both carry it, and the two languages differ
    for (const key of keys) {
      expect(text(fr, key).length, key).toBeGreaterThan(0);
      expect(text(en, key), key).not.toBe(text(fr, key));
    }
  });
});

describe("Developer > Experimental - each row explained", () => {
  test("the three switches come before the separation of the reasoning, each with its explanation", () => {
    // GIVEN the developer mode on and nothing written
    // WHEN Developer renders
    const html = renderToString(Developer).body;
    const block = slice(html, "developer-experimental", "developer-mcp");

    // THEN the rows are in the order of the mock-up, each with its label and
    // its explanation, at their defaults: debugging blocks on, the two
    // others off
    const order = [
      "developer-debug-blocks",
      "developer-resource-widget",
      "developer-preset-load-settings",
      "developer-separate-reasoning",
    ].map((testid) => block.indexOf(`data-testid="${testid}"`));
    expect(EXPERIMENTAL_ROWS.map((row) => row.key)).toEqual([
      "desktop.chat_debug_blocks",
      "desktop.resource_widget",
      "models.preset_load_settings",
    ]);
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    for (const row of EXPERIMENTAL_ROWS) {
      expect(block).toContain(text(fr, row.label));
      expect(text(fr, row.hint).length).toBeGreaterThan(20);
      expect(block).toContain(text(fr, row.hint));
    }
    expect(tagOf(block, "developer-debug-blocks-switch")).toMatch(/aria-checked="true"/);
    expect(tagOf(block, "developer-resource-widget-switch")).toMatch(/aria-checked="false"/);
    expect(tagOf(block, "developer-preset-load-settings-switch")).toMatch(/aria-checked="false"/);
    expect(tagOf(block, "developer-preset-load-settings-help")).not.toBeNull();
  });

  test("the defaults are the ones of the key registry, and every key is lockable", () => {
    // GIVEN the registry of the keys of apollia.toml
    const registry = readFileSync(KEY_REGISTRY, "utf8");

    // WHEN each key of the store is looked up in it
    // THEN it is registered as a lockable boolean with the default the store
    // holds
    for (const key of Object.values(DEVELOPER_SETTING_KEYS)) {
      const line = registry.split("\n").find((l) => l.includes(`key("${key}"`));
      expect(line, key).toBeDefined();
      expect(line, key).toContain("locked(");
      expect(line, key).toContain(`Boolean, Some("${DEVELOPER_SETTING_DEFAULTS[key]}")`);
    }
  });

  test("a switch is persisted through the shell and read back at the next launch", async () => {
    // GIVEN a shell with nothing written
    const { shell, file } = memoryShell();

    // WHEN each experimental switch is flipped, the store forgotten, then
    // seeded again as a launch does
    for (const row of EXPERIMENTAL_ROWS) {
      await setDeveloperSetting(row.key, !DEVELOPER_SETTING_DEFAULTS[row.key], shell);
    }
    resetSettings();
    await hydrateDeveloperSettings(shell);

    // THEN the file holds the three keys and the store reads them back
    expect(Object.fromEntries(file)).toEqual({
      "desktop.chat_debug_blocks": false,
      "desktop.resource_widget": true,
      "models.preset_load_settings": true,
    });
    for (const row of EXPERIMENTAL_ROWS) {
      expect(get(developerSettings)[row.key]).toEqual({
        value: !DEVELOPER_SETTING_DEFAULTS[row.key],
        source: "user",
      });
    }
  });

  test("a refused write puts the switch back, and a setting the policy imposes is not written", async () => {
    // GIVEN a shell that refuses every write, then one whose policy imposes
    // the debugging blocks off
    const refusing: DeveloperSettingsIpc = {
      read: vi.fn(async (key: string) => ({ key, value: true, source: "default" as const })),
      write: vi.fn(async () => {
        throw new Error("unreadable file");
      }),
    };
    const { shell } = memoryShell({ "desktop.chat_debug_blocks": false });

    // WHEN the widget is turned on against the first, and the debugging
    // blocks against the second once hydrated
    const refused = await setDeveloperSetting(DEVELOPER_SETTING_KEYS.resourceWidget, true, refusing);
    const afterRefusal = get(developerSettings)[DEVELOPER_SETTING_KEYS.resourceWidget].value;
    await hydrateDeveloperSettings(shell);
    const locked = await setDeveloperSetting(DEVELOPER_SETTING_KEYS.chatDebugBlocks, true, shell);
    const html = renderToString(Developer).body;

    // THEN the widget stayed off, the imposed value is shown locked and no
    // write was attempted for it
    expect(refused).toBe(false);
    expect(afterRefusal).toBe(false);
    expect(locked).toBe(false);
    expect(shell.write).not.toHaveBeenCalled();
    expect(tagOf(html, "developer-debug-blocks-switch")).toMatch(/aria-checked="false"/);
    expect(tagOf(html, "developer-debug-blocks-switch")).toMatch(/disabled/);
    expect(tagOf(html, "developer-debug-blocks-policy")).not.toBeNull();
  });

  test("every text of the rows exists in French and in English, apart", () => {
    // GIVEN the texts the rows name
    const keys = [
      EXPERIMENTAL_WRITE_FAILED,
      ...EXPERIMENTAL_ROWS.flatMap((row) => [
        row.label,
        row.hint,
        ...(row.help ? [row.help.text, row.help.label] : []),
      ]),
    ];

    // WHEN each is looked up in both catalogues
    // THEN both carry it, and the two languages differ
    for (const key of keys) {
      expect(text(fr, key).length, key).toBeGreaterThan(0);
      expect(text(en, key), key).not.toBe(text(fr, key));
    }
  });
});

describe("Debugging blocks in the chat - hidden without leaving the developer mode", () => {
  test("they are shown only with the mode on and the switch on", () => {
    // GIVEN the four positions of the mode and of the switch
    // WHEN each is decided
    // THEN only mode on with the switch on shows them
    expect(debugBlocksShown(true, true)).toBe(true);
    expect(debugBlocksShown(true, false)).toBe(false);
    expect(debugBlocksShown(false, true)).toBe(false);
    expect(debugBlocksShown(false, false)).toBe(false);
  });

  test("turning the switch off hides them and leaves the developer mode on", async () => {
    // GIVEN the developer mode on, the switch at its default
    const { shell } = memoryShell();
    const before = get(chatDebugBlocks);
    const traceBefore = renderToString(ExecutionTrace, { props: { taskId: "t-1" } }).body;

    // WHEN the switch is turned off
    await setDeveloperSetting(DEVELOPER_SETTING_KEYS.chatDebugBlocks, false, shell);
    const traceAfter = renderToString(ExecutionTrace, { props: { taskId: "t-1" } }).body;

    // THEN the blocks are hidden, the detailed trace falls back to the
    // default one, and the mode itself did not move
    expect(before).toBe(true);
    expect(get(chatDebugBlocks)).toBe(false);
    expect(get(developerMode)).toBe(true);
    expect(traceBefore).toContain('data-skin="builder"');
    expect(traceAfter).toContain('data-skin="operator"');
  });

  test("the raw data of a step and the detailed trace read the switch, not the mode alone", () => {
    // GIVEN the three surfaces that show a debugging block
    const surfaces = [
      "components/chat/ChatConversation.svelte",
      "components/chat/ReasoningSequence.svelte",
      "components/observability/ExecutionTrace.svelte",
    ];

    // WHEN their source is read
    // THEN each decides its skin from the derived store
    for (const file of surfaces) {
      const source = readFileSync(resolve(UI_SRC, file), "utf8");
      expect(source, file).toMatch(/\$chatDebugBlocks[^\n]*\? "(builder|operator)"/);
      expect(source, file).not.toMatch(/\$developerMode \? "builder"/);
    }
  });
});

describe("Consumption widget - at the foot of the sidebar, on the measurement of the monitor", () => {
  test("it is shown only with the developer mode on and the switch on", () => {
    // GIVEN the four positions of the mode and of the switch
    // WHEN each is decided
    // THEN only mode on with the switch on shows it, and the switch is off
    // by default
    expect(resourceWidgetShown(true, true)).toBe(true);
    expect(resourceWidgetShown(true, false)).toBe(false);
    expect(resourceWidgetShown(false, true)).toBe(false);
    expect(DEVELOPER_SETTING_DEFAULTS[DEVELOPER_SETTING_KEYS.resourceWidget]).toBe(false);
    expect(get(resourceWidgetVisible)).toBe(false);
  });

  test("shown, it subscribes to the monitor; hidden, it unsubscribes and nothing is read", async () => {
    // GIVEN the monitor of System > Hardware reading every two seconds, and
    // the sidebar watching a widget that is hidden
    vi.useFakeTimers();
    const read = vi.fn(async () => USAGE);
    const monitor = createResourceMonitor(read, 2000);
    const shown = writable(false);
    const seen: Array<ResourceUsage | null> = [];
    const leave = whileShown(shown, monitor).subscribe((usage) => seen.push(usage));
    await vi.advanceTimersByTimeAsync(10_000);
    expect(read).not.toHaveBeenCalled();

    // WHEN the widget is shown for two intervals, then hidden
    shown.set(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(read).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(4000);
    expect(read).toHaveBeenCalledTimes(3);
    const whileVisible = seen.at(-1);
    shown.set(false);
    await vi.advanceTimersByTimeAsync(60_000);

    // THEN the measurement reached the widget, the reads stopped with it, no
    // timer is left, and the figures are dropped
    expect(whileVisible).toEqual(USAGE);
    expect(read).toHaveBeenCalledTimes(3);
    expect(vi.getTimerCount()).toBe(0);
    expect(seen.at(-1)).toBeNull();

    // WHEN it is shown again, then the sidebar itself goes away
    shown.set(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(read).toHaveBeenCalledTimes(4);
    leave();
    await vi.advanceTimersByTimeAsync(60_000);

    // THEN the reads resumed, and stopped again with the sidebar
    expect(read).toHaveBeenCalledTimes(4);
    expect(vi.getTimerCount()).toBe(0);
  });

  test("the sidebar mounts it on the monitor's own command, and only when shown", async () => {
    // GIVEN the sidebar with the widget off
    vi.useFakeTimers();
    ipc.invoke.mockImplementation(async (command: string) =>
      command === "get_resource_usage" ? USAGE : null,
    );
    const hidden = renderToString(Sidebar).body;
    await vi.advanceTimersByTimeAsync(10_000);
    const readsHidden = ipc.invoke.mock.calls.filter((c) => c[0] === "get_resource_usage").length;

    // WHEN the switch is turned on and the sidebar renders again
    developerSettings.update((all) => ({
      ...all,
      [DEVELOPER_SETTING_KEYS.resourceWidget]: { value: true, source: "user" },
    }));
    const shown = renderToString(Sidebar).body;
    await vi.advanceTimersByTimeAsync(0);
    const commands = ipc.invoke.mock.calls.map((c) => c[0]);

    // THEN hidden it measured nothing; shown, the widget is in the sidebar
    // and the one measurement command of the monitor was asked, no other
    expect(tagOf(hidden, "sidebar-resource-widget")).toBeNull();
    expect(readsHidden).toBe(0);
    expect(tagOf(shown, "sidebar-resource-widget")).not.toBeNull();
    expect(commands.filter((c) => c === "get_resource_usage")).toHaveLength(1);
    expect(vi.getTimerCount()).toBe(0);
    const sidebar = readFileSync(resolve(UI_SRC, "lib/components/app/Sidebar.svelte"), "utf8");
    expect(sidebar).toContain("whileShown(resourceWidgetVisible)");
  });

  test("it shows the processor, the memory and the GPU memory, each with its gauge", () => {
    // GIVEN a measurement with dedicated video memory
    // WHEN the widget renders, full then compact
    const full = renderToString(ResourceWidget, { props: { usage: USAGE, onopen: () => {} } }).body;
    const compact = renderToString(ResourceWidget, {
      props: { usage: USAGE, compact: true, onopen: () => {} },
    }).body;

    // THEN the three figures are named and gauged, and the rail names them
    // in its label
    for (const id of ["cpu", "ram", "vram"]) {
      expect(tagOf(full, `sidebar-resource-widget-${id}`)).not.toBeNull();
    }
    expect(full).toContain(fr.nav.resource_widget.cpu);
    expect(full).toContain(fr.nav.resource_widget.ram);
    expect(full).toContain(fr.nav.resource_widget.vram);
    expect(full).toContain("18 %");
    expect(full).toContain("9.4 Go sur 32.0 Go");
    expect(full).toContain("5.1 Go sur 8.0 Go");
    expect((full.match(/role="progressbar"/g) ?? []).length).toBe(3);
    expect(tagOf(compact, "sidebar-resource-widget")).toMatch(/data-state="compact"/);
    expect(tagOf(compact, "sidebar-resource-widget")).toContain(fr.nav.resource_widget.vram);
    expect((compact.match(/role="progressbar"/g) ?? []).length).toBe(3);
  });

  test("unified or unmeasured video memory is said, never gauged", () => {
    // GIVEN a Mac, where the video memory is the memory, and a machine that
    // does not measure it
    const unified = { ...USAGE, vram: { kind: "unified" } } as ResourceUsage;
    const unmeasured = { ...USAGE, vram: { kind: "unmeasured" } } as ResourceUsage;

    // WHEN the gauges are computed and the widget renders
    const html = renderToString(ResourceWidget, { props: { usage: unified, onopen: () => {} } }).body;
    const other = renderToString(ResourceWidget, {
      props: { usage: unmeasured, onopen: () => {} },
    }).body;

    // THEN the GPU memory has no share, two gauges are drawn, and the row
    // says why
    expect(widgetGauges(unified).map((g) => g.percent)).toEqual([18, 29.375, null]);
    expect((html.match(/role="progressbar"/g) ?? []).length).toBe(2);
    expect(html).toContain(fr.nav.resource_widget.vram_unified);
    expect(other).toContain(fr.nav.resource_widget.vram_unmeasured);
    expect(widgetTone(40)).toBe("primary");
    expect(widgetTone(80)).toBe("warning");
    expect(widgetTone(95)).toBe("destructive");
  });

  test("before the first measurement it says it is measuring, and a click opens the monitor of System", () => {
    // GIVEN no measurement yet, and the settings window closed
    settingsWindowOpen.set(false);

    // WHEN the widget renders and its click handler runs, as the sidebar
    // wires it
    const html = renderToString(ResourceWidget, { props: { usage: null, onopen: () => {} } }).body;
    openSettingsWindow("hardware");

    // THEN it says so, and the window is open on System > Hardware, where
    // the resource monitor lives
    expect(tagOf(html, "sidebar-resource-widget-measuring")).not.toBeNull();
    expect(get(settingsWindowOpen)).toBe(true);
    expect(get(settingsSubRoute)).toBe("hardware");
    const sidebar = readFileSync(resolve(UI_SRC, "lib/components/app/Sidebar.svelte"), "utf8");
    expect(sidebar).toContain('openSettingsWindow("hardware")');
    settingsWindowOpen.set(false);
  });

  test("its texts exist in French and in English, apart", () => {
    // GIVEN the texts of the widget
    // WHEN both catalogues are compared
    // THEN they carry the same keys with different words
    expect(Object.keys(en.nav.resource_widget)).toEqual(Object.keys(fr.nav.resource_widget));
    for (const key of Object.keys(fr.nav.resource_widget) as Array<
      keyof typeof fr.nav.resource_widget
    >) {
      expect(en.nav.resource_widget[key], key).not.toBe(fr.nav.resource_widget[key]);
    }
  });
});
