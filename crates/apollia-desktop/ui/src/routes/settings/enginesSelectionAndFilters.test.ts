import { describe, test, expect, vi, beforeAll, beforeEach } from "vitest";
import { render as renderToString } from "svelte/server";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
const storage = vi.hoisted(() => {
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
  return memory;
});

const ipc = vi.hoisted(() => ({
  invoke: vi.fn(async (_command: string, _args?: unknown): Promise<unknown> => null),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import Engines from "./Engines.svelte";
import EngineList, {
  DEFAULT_ENGINE_FILTERS,
  engineRows,
  filterEngineRows,
  presentEngineTypes,
} from "../../components/settings/system/EngineList.svelte";
import { servedFormats } from "../../components/settings/system/EngineSelection.svelte";
import {
  DEFAULT_ENGINE_SETTINGS,
  ENGINE_AUTO_UPDATE_KEY,
  ENGINE_FORMAT_KEYS,
  chooseEngine,
  engineOptions,
  engineSelection,
  engineSettings,
  hydrateEngineSettings,
  setEngineAutoUpdate,
} from "$lib/settings/engineSelection";
import { verifyEngineUpdates } from "$lib/settings/engineChannel";
import { developerMode } from "$lib/stores/developerMode";
import { enginesStore, hardwareStore, systemInfoStore } from "$lib/stores/settings";
import type { EngineEntry, EnginesResponse } from "$lib/ipc/system";
import type { HardwareProfile } from "$lib/ipc/modelHub";
import fr from "$lib/i18n/fr.json";

const copy = fr.settings.engines;

/** A catalogue sentence with its `{name}` placeholders filled. */
function fill(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) => values[name] ?? "");
}

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** The `<select>` carrying `testid`, with its options. */
function selectOf(html: string, testid: string): string {
  const start = html.indexOf(`data-testid="${testid}"`);
  return start === -1 ? "" : html.slice(start, html.indexOf("</select>", start));
}

function loaded<T>(data: T) {
  return { data, loading: false, error: null, loaded: true };
}

function hardware(kind: HardwareProfile["accelerator"]["kind"]): HardwareProfile {
  return {
    total_ram_gb: 64,
    available_ram_gb: 40,
    cpu_model: "test",
    cpu_cores: 12,
    accelerator: { kind },
    memory_budget_gb: 48,
    total_capacity_gb: 48,
    devices: [],
  };
}

function engine(over: Partial<EngineEntry>): EngineEntry {
  return {
    engine: "llama-server",
    tag: null,
    variant: "metal",
    path: "/x",
    source: "embedded",
    version: "7001 (0ff1ce0)",
    in_service: false,
    ...over,
  };
}

/**
 * A Mac: llama.cpp shipped and in service, two CUDA builds and a Vulkan build
 * in the store, the MLX engine shipped and a newer one in the store, Node.js,
 * and the dictation engine.
 */
const ENGINES: EnginesResponse = {
  engines: [
    engine({ in_service: true }),
    engine({ engine: "vllm-mlx", version: "0.9.1" }),
    engine({ variant: "cuda", tag: "b7001", source: "downloaded", version: "7001 (0ff1ce0)" }),
    engine({ variant: "cuda", tag: "b7100", source: "downloaded", version: "7100 (c0ffee0)" }),
    engine({ variant: "vulkan", tag: "b6900", source: "imported", version: "6900 (deadbee)" }),
    engine({
      engine: "vllm-mlx",
      tag: "v0.9.2",
      source: "downloaded",
      version: "0.9.2",
      in_service: true,
    }),
    engine({ engine: "nodejs", variant: "cpu", tag: "v22.11.0", source: "downloaded", version: "v22.11.0" }),
  ],
  channel: "stable",
  manifest_sequence: 12,
  updates: [],
  cuda_proposable: false,
  dictation: { path: "/x/runner", version: "1.7.6" },
};

beforeAll(async () => {
  locale.set("fr");
  await waitLocale("fr");
});

beforeEach(() => {
  ipc.invoke.mockReset();
  ipc.invoke.mockImplementation(async () => null);
  storage.clear();
  developerMode.set(false);
  enginesStore.set(loaded(ENGINES));
  hardwareStore.set(loaded(hardware("apple_silicon")));
  engineSettings.set(DEFAULT_ENGINE_SETTINGS);
  systemInfoStore.set(
    loaded({
      version: "0.3.0",
      os: "macos aarch64",
      arch: "aarch64",
      python_path: null,
      data_dir: "/Users/u/.apollia",
    }),
  );
});

describe("Engines - one engine per format", () => {
  test("the page opens on the engine of each format: Automatic, then every engine held with its version", () => {
    // GIVEN a Mac holding several engines per format, and no choice written
    // WHEN System > Engines renders
    const html = renderToString(Engines).body;

    // THEN the engine per format comes first, before the list
    expect(html).toContain(copy.selection_title);
    expect(html.indexOf('data-testid="engines-selection"')).toBeGreaterThan(-1);
    expect(html.indexOf('data-testid="engines-selection"')).toBeLessThan(
      html.indexOf('data-testid="engines-list"'),
    );
    // AND it carries a GGUF line and an MLX line
    expect(tagOf(html, "engine-format-gguf")).not.toBeNull();
    expect(tagOf(html, "engine-format-mlx")).not.toBeNull();
    // AND the GGUF list opens on Automatic, chosen, then names the shipped
    //     engine and the newest build of each variant, each with its version
    const gguf = selectOf(html, "engine-format-gguf-select");
    const labels = Array.from(gguf.matchAll(/<option[^>]*>([^<]*)<\/option>/g), (m) => m[1]);
    expect(labels).toEqual([
      copy.option_auto,
      fill(copy.option_embedded, { name: "llama.cpp · Metal", version: "7001 (0ff1ce0)" }),
      "llama.cpp · CUDA · 7100 (c0ffee0)",
      "llama.cpp · Vulkan · 6900 (deadbee)",
    ]);
    expect(gguf).toMatch(/<option value="auto" selected[^>]*>/);
    // AND the MLX list does the same with the MLX engines
    const mlx = selectOf(html, "engine-format-mlx-select");
    expect(mlx).toContain(`>${copy.option_auto}</option>`);
    expect(mlx).toContain(fill(copy.option_embedded, { name: "vllm-mlx · Metal", version: "0.9.1" }));
    expect(mlx).toContain("vllm-mlx · Metal · 0.9.2");
    // AND each line names the engine in service
    expect(html).toContain(fill(copy.format_serving, { name: "llama.cpp · Metal" }));
    expect(html).toContain(fill(copy.format_serving, { name: "vllm-mlx · Metal" }));
  });

  test("the MLX line exists on a Mac with an Apple chip only", () => {
    // GIVEN the same engines on a computer with an NVIDIA card
    hardwareStore.set(loaded(hardware("cuda")));

    // WHEN System > Engines renders
    const html = renderToString(Engines).body;

    // THEN GGUF is offered and MLX is not
    expect(tagOf(html, "engine-format-gguf")).not.toBeNull();
    expect(tagOf(html, "engine-format-mlx")).toBeNull();
    expect(servedFormats("apple_silicon")).toEqual(["gguf", "mlx"]);
    expect(servedFormats("generic")).toEqual(["gguf"]);
    expect(servedFormats(null)).toEqual(["gguf"]);
  });

  test("the choices are the engines of the list, the newest build of a variant only", () => {
    // GIVEN the engine list
    // WHEN the choices of each format are read
    const gguf = engineOptions(ENGINES, "gguf");
    const mlx = engineOptions(ENGINES, "mlx");

    // THEN GGUF offers the shipped engine, CUDA at its newest build and Vulkan
    expect(gguf.map((o) => [o.value, o.version])).toEqual([
      ["embedded", "7001 (0ff1ce0)"],
      ["cuda", "7100 (c0ffee0)"],
      ["vulkan", "6900 (deadbee)"],
    ]);
    // AND MLX offers the shipped engine and the one of the store
    expect(mlx.map((o) => [o.value, o.version])).toEqual([
      ["embedded", "0.9.1"],
      ["metal", "0.9.2"],
    ]);
    // AND without a list there is nothing to choose
    expect(engineOptions(null, "gguf")).toEqual([]);
  });

  test("a chosen variant is shown chosen, and one since removed reads as Automatic with a warning", () => {
    // GIVEN CUDA chosen for GGUF, then a processor build this Mac does not hold
    engineSettings.set({
      ...DEFAULT_ENGINE_SETTINGS,
      gguf: { value: "cuda", source: "user" },
    });
    const kept = renderToString(Engines).body;
    engineSettings.set({
      ...DEFAULT_ENGINE_SETTINGS,
      gguf: { value: "cpu", source: "user" },
    });

    // WHEN System > Engines renders
    const removed = renderToString(Engines).body;

    // THEN the held variant is chosen and nothing warns
    expect(selectOf(kept, "engine-format-gguf-select")).toMatch(/<option value="cuda" selected/);
    expect(tagOf(kept, "engine-format-gguf-fell-back")).toBeNull();
    // AND the removed one falls back to Automatic, with the warning
    expect(selectOf(removed, "engine-format-gguf-select")).toMatch(/<option value="auto" selected/);
    expect(tagOf(removed, "engine-format-gguf-fell-back")).not.toBeNull();
    expect(removed).toContain(fill(copy.selection_fell_back, { format: "GGUF" }));
    expect(tagOf(removed, "engine-format-mlx-fell-back")).toBeNull();
    expect(engineSelection("cpu", engineOptions(ENGINES, "gguf"))).toEqual({
      value: "auto",
      fellBack: true,
    });
    expect(engineSelection("auto", [])).toEqual({ value: "auto", fellBack: false });
  });

  test("a choice is written to [engines] through the settings command, and read back from it", async () => {
    // GIVEN a shell holding the three engine settings
    const held: Record<string, unknown> = {
      [ENGINE_FORMAT_KEYS.gguf]: "vulkan",
      [ENGINE_FORMAT_KEYS.mlx]: "auto",
      [ENGINE_AUTO_UPDATE_KEY]: false,
    };
    ipc.invoke.mockImplementation(async (command: string, args?: unknown) => {
      const { key, value } = (args ?? {}) as { key: string; value?: unknown };
      if (command === "set_setting") held[key] = value;
      return { key, value: held[key], source: "user" };
    });

    // WHEN the page reads them, then the engine of MLX is chosen
    await hydrateEngineSettings();
    const read = get(engineSettings);
    const written = await chooseEngine("mlx", "embedded");

    // THEN the values in force are the shell's
    expect(read.gguf.value).toBe("vulkan");
    expect(read.autoUpdate.value).toBe(false);
    // AND the choice went through set_setting under its key, nowhere else
    expect(written).toBe(true);
    expect(ipc.invoke).toHaveBeenCalledWith("set_setting", {
      key: "engines.mlx",
      value: "embedded",
    });
    expect(get(engineSettings).mlx).toEqual({ value: "embedded", source: "user" });
    expect(storage.size).toBe(0);
  });

  test("a refused write puts the list back, and a value the organization fixes locks its control", async () => {
    // GIVEN a shell that refuses every write
    const refusing = {
      read: async (key: string) => ({ key, value: "auto", source: "policy" as const }),
      write: async () => {
        throw new Error("imposed by the policy");
      },
    };
    await hydrateEngineSettings({
      read: async (key: string) =>
        key === ENGINE_AUTO_UPDATE_KEY
          ? { key, value: true, source: "policy" as const }
          : refusing.read(key),
      write: refusing.write,
    });

    // WHEN a choice and the automatic update are written
    const chosen = await chooseEngine("gguf", "cuda", refusing);
    const toggled = await setEngineAutoUpdate(false, refusing);
    const html = renderToString(Engines).body;

    // THEN neither went through and the values are the ones in force
    expect(chosen).toBe(false);
    expect(toggled).toBe(false);
    expect(get(engineSettings).gguf.value).toBe("auto");
    expect(get(engineSettings).autoUpdate.value).toBe(true);
    // AND each control is locked under the policy note
    expect(tagOf(html, "engine-format-gguf-policy")).not.toBeNull();
    expect(tagOf(html, "engines-auto-update-policy")).not.toBeNull();
    expect(tagOf(html, "engine-format-gguf-select")).toContain("disabled");
    expect(html).toContain(fr.settings.policy_locked);
  });
});

describe("Engines - automatic update of the chosen engines", () => {
  test("it is on by default, and turning it off writes engines.auto_update", async () => {
    // GIVEN the defaults
    // WHEN System > Engines renders, then the switch is turned off
    const html = renderToString(Engines).body;
    ipc.invoke.mockImplementation(async (_c: string, args?: unknown) => {
      const { key, value } = args as { key: string; value: unknown };
      return { key, value, source: "user" };
    });
    const written = await setEngineAutoUpdate(false);

    // THEN the switch was shown on, under its name
    expect(html).toContain(copy.auto_update_label);
    expect(tagOf(html, "engines-auto-update-toggle")).toContain('aria-checked="true"');
    expect(tagOf(html, "engines-auto-update-policy")).toBeNull();
    // AND turning it off wrote the setting, and the switch follows
    expect(written).toBe(true);
    expect(ipc.invoke).toHaveBeenCalledWith("set_setting", {
      key: "engines.auto_update",
      value: false,
    });
    expect(tagOf(renderToString(Engines).body, "engines-auto-update-toggle")).toContain(
      'aria-checked="false"',
    );
  });
});

describe("Engines - check for updates", () => {
  test("the check asks the channel again, then the update of each engine is the one just verified", async () => {
    // GIVEN a list without an update, and a channel that now announces b7200
    const refreshed: EnginesResponse = {
      ...ENGINES,
      updates: [
        {
          engine: "llama-server",
          variant: "cuda",
          installed_tag: "b7100",
          latest_tag: "b7200",
          update_available: true,
        },
      ],
    };
    ipc.invoke.mockImplementation(async (command: string) =>
      command === "list_engines" ? refreshed : null,
    );
    const before = renderToString(Engines).body;

    // WHEN the check is asked
    const refusal = await verifyEngineUpdates();
    const after = renderToString(EngineList, {
      props: {
        response: get(enginesStore).data,
        loading: false,
        error: null,
        acceleratorKind: "cuda",
        developer: false,
        checking: false,
        checkError: null,
        onchannel: () => {},
        oncheck: () => {},
      },
    }).body;

    // THEN the button is there, the channel was checked before the list was
    //      read again, and the update column says what the channel answered
    expect(tagOf(before, "engines-check-updates")).not.toBeNull();
    expect(tagOf(before, "engine-row-update-available")).toBeNull();
    expect(refusal).toBeNull();
    expect(ipc.invoke.mock.calls.map((call) => call[0])).toEqual([
      "check_engine_updates",
      "list_engines",
    ]);
    expect(after).toContain(fill(copy.update_available, { tag: "b7200" }));
  });
});

describe("Engines - the list is filtered three ways", () => {
  const rows = () => engineRows(ENGINES, "apple_silicon");
  const names = (list: ReturnType<typeof rows>) =>
    list.map((row) => `${row.name}/${row.variant ?? "-"}/${row.tag ?? "-"}`);

  test("compatible only, the default, hides the builds this machine cannot run; All shows them", () => {
    // GIVEN the engines of a Mac, some built for an NVIDIA or a Vulkan card
    // WHEN the list is filtered with the defaults, then with All
    const compatible = filterEngineRows(rows(), DEFAULT_ENGINE_FILTERS);
    const all = filterEngineRows(rows(), { ...DEFAULT_ENGINE_FILTERS, compatibleOnly: false });

    // THEN the default keeps what an Apple chip runs
    expect(DEFAULT_ENGINE_FILTERS.compatibleOnly).toBe(true);
    expect(names(compatible)).toEqual([
      "llama.cpp/metal/-",
      "vllm-mlx/metal/-",
      "vllm-mlx/metal/v0.9.2",
      "Node.js/cpu/v22.11.0",
      "whisper.cpp/-/-",
    ]);
    // AND All shows every engine of the list
    expect(all.length).toBe(rows().length);
    expect(names(all)).toContain("llama.cpp/cuda/b7100");
    // AND on a computer with an NVIDIA card the CUDA and Vulkan builds are
    //     compatible and the MLX build of the store is not
    const onNvidia = filterEngineRows(engineRows(ENGINES, "cuda"), DEFAULT_ENGINE_FILTERS);
    expect(names(onNvidia)).toContain("llama.cpp/cuda/b7100");
    expect(names(onNvidia)).toContain("llama.cpp/vulkan/b6900");
    expect(names(onNvidia)).not.toContain("vllm-mlx/metal/v0.9.2");
    // AND a machine nothing is known of excludes nothing
    expect(filterEngineRows(engineRows(ENGINES), DEFAULT_ENGINE_FILTERS).length).toBe(8);
  });

  test("the search keeps the engines whose name, variant or version carries the text", () => {
    // GIVEN every engine shown
    const all = { ...DEFAULT_ENGINE_FILTERS, compatibleOnly: false };

    // WHEN a name, a variant, a version and an unknown text are searched
    const byName = filterEngineRows(rows(), { ...all, query: "  WHISPER " });
    const byVariant = filterEngineRows(rows(), { ...all, query: "cuda" });
    const byVersion = filterEngineRows(rows(), { ...all, query: "0.9.2" });
    const nothing = filterEngineRows(rows(), { ...all, query: "zzz" });

    // THEN each finds its engines, case and spaces ignored
    expect(names(byName)).toEqual(["whisper.cpp/-/-"]);
    expect(names(byVariant)).toEqual(["llama.cpp/cuda/b7001", "llama.cpp/cuda/b7100"]);
    expect(names(byVersion)).toEqual(["vllm-mlx/metal/v0.9.2"]);
    expect(nothing).toEqual([]);
  });

  test("the type keeps one kind of engine, and offers only the kinds this machine holds", () => {
    // GIVEN every engine shown
    const all = { ...DEFAULT_ENGINE_FILTERS, compatibleOnly: false };

    // WHEN each type is asked for
    const inference = filterEngineRows(rows(), { ...all, type: "inference" });
    const dictation = filterEngineRows(rows(), { ...all, type: "dictation" });
    const tools = filterEngineRows(rows(), { ...all, type: "tools" });

    // THEN each keeps its own
    expect(inference.every((row) => row.type === "inference")).toBe(true);
    expect(inference.length).toBe(6);
    expect(names(dictation)).toEqual(["whisper.cpp/-/-"]);
    expect(names(tools)).toEqual(["Node.js/cpu/v22.11.0"]);
    // AND a list without a dictation engine or Node.js offers inference alone
    expect(presentEngineTypes(rows())).toEqual(["inference", "dictation", "tools"]);
    const bare = engineRows({ ...ENGINES, engines: [ENGINES.engines[0]], dictation: null });
    expect(presentEngineTypes(bare)).toEqual(["inference"]);
  });

  test("the page shows the three filters, compatible only chosen, and says when nothing matches", () => {
    // GIVEN the engines of a Mac
    const props = {
      response: ENGINES,
      loading: false,
      error: null,
      acceleratorKind: "apple_silicon",
      developer: false,
      checking: false,
      checkError: null,
      onchannel: () => {},
      oncheck: () => {},
    };

    // WHEN the page renders, then the list under a search nothing matches
    const html = renderToString(Engines).body;
    const empty = renderToString(EngineList, {
      props: { ...props, initialFilters: { ...DEFAULT_ENGINE_FILTERS, query: "zzz" } },
    }).body;
    const every = renderToString(EngineList, {
      props: { ...props, initialFilters: { ...DEFAULT_ENGINE_FILTERS, compatibleOnly: false } },
    }).body;

    // THEN the search and the two lists are there, compatible only chosen
    expect(tagOf(html, "engines-search")).toContain(copy.search_placeholder);
    const compat = selectOf(html, "engines-filter-compat");
    expect(compat).toMatch(/<option value="compatible" selected[^>]*>/);
    expect(compat).toContain(`>${copy.filter_compatible}</option>`);
    expect(compat).toContain(`>${copy.filter_all}</option>`);
    const type = selectOf(html, "engines-filter-type");
    expect(type).toContain(`>${copy.type_all}</option>`);
    expect(type).toContain(`>${copy.type_inference}</option>`);
    expect(type).toContain(`>${copy.type_dictation}</option>`);
    // AND the rows are the compatible ones, then all of them
    expect(html.match(/data-testid="engine-row"/g)?.length).toBe(5);
    expect(html).not.toContain("llama.cpp · CUDA</span>");
    expect(every.match(/data-testid="engine-row"/g)?.length).toBe(8);
    // AND a search nothing matches says so, with no row
    expect(tagOf(empty, "engines-list-no-match")).not.toBeNull();
    expect(empty).toContain(copy.no_match);
    expect(tagOf(empty, "engine-row")).toBeNull();
  });
});
