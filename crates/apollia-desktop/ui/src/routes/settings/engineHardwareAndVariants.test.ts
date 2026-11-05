import { describe, test, expect, vi, beforeAll, beforeEach } from "vitest";
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

const ipc = vi.hoisted(() => ({
  invoke: vi.fn(async (_command: string, _args?: unknown): Promise<unknown> => null),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import Engines from "./Engines.svelte";
import { variantLabel } from "../../components/settings/system/EngineList.svelte";
import { engineOptions } from "$lib/settings/engineSelection";
import { verifyEngineUpdates } from "$lib/settings/engineChannel";
import {
  incompatibleCount,
  reasonSentence,
  shownVariants,
  variantList,
} from "$lib/settings/engineVariants";
import { developerMode } from "$lib/stores/developerMode";
import { engineSettings, DEFAULT_ENGINE_SETTINGS } from "$lib/settings/engineSelection";
import { enginesStore, hardwareStore, systemInfoStore } from "$lib/stores/settings";
import type {
  EngineAvailablePackage,
  EngineEntry,
  EngineHardwareCard,
  EngineRatingReason,
  EnginesResponse,
} from "$lib/ipc/system";
import type { HardwareProfile } from "$lib/ipc/modelHub";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const copy = fr.settings.engines;

function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) => String(values[name] ?? ""));
}

/** The element carrying `data-testid="<testid>"`, up to its closing tag, or `null`. */
function blockOf(html: string, testid: string): string | null {
  const start = html.indexOf(`data-testid="${testid}"`);
  if (start === -1) return null;
  const open = html.lastIndexOf("<", start);
  const tag = html.slice(open + 1).match(/^\w+/)?.[0] ?? "div";
  return html.slice(open, html.indexOf(`</${tag}>`, start) + tag.length + 3);
}

/** The `<select>` carrying `testid`, with its options. */
function selectOf(html: string, testid: string): string {
  const start = html.indexOf(`data-testid="${testid}"`);
  return start === -1 ? "" : html.slice(start, html.indexOf("</select>", start));
}

/** Text of an HTML fragment, tags and comments dropped, spaces collapsed. */
function text(html: string | null): string {
  return (html ?? "").replace(/<!--[^>]*-->/g, "").replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
}

function loaded<T>(data: T) {
  return { data, loading: false, error: null, loaded: true };
}

function profile(kind: HardwareProfile["accelerator"]["kind"]): HardwareProfile {
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

function card(over: Partial<EngineHardwareCard>): EngineHardwareCard {
  return {
    vendor: "nvidia",
    name: "NVIDIA GeForce RTX 4090",
    vram_gb: 24,
    driver_version: "580.65.06",
    compute_capability: "8.9",
    gfx_target: null,
    kfd_accessible: null,
    timed_out: false,
    ...over,
  };
}

function pkg(variant: EngineAvailablePackage["variant"], installed = false): EngineAvailablePackage {
  return { engine: "llama-server", tag: "b7100", variant, size: 1, released: null, installed };
}

const EMBEDDED: EngineEntry = {
  engine: "llama-server",
  tag: null,
  variant: "cpu",
  path: "/opt/apollia/llama-server",
  source: "embedded",
  version: "7100 (c0ffee0)",
  in_service: true,
};

function installed(variant: EngineEntry["variant"]): EngineEntry {
  return { ...EMBEDDED, tag: "b7100", variant, source: "downloaded", in_service: false };
}

/** A computer with an NVIDIA card whose runtime read the channel. */
const NVIDIA: EnginesResponse = {
  engines: [EMBEDDED, installed("cuda-12"), installed("rocm")],
  channel: "stable",
  manifest_sequence: 12,
  updates: [],
  dictation: null,
  verifiable: true,
  hardware: { cards: [card({})], vulkan_loader: true, rocm_runtime: false },
  available: [pkg("cuda-13"), pkg("cuda-12", true), pkg("rocm", true), pkg("vulkan"), pkg("cpu")],
  ratings: [
    { variant: "cuda-13", verdict: "compatible", reason: { code: "driver_too_old", have: "580.65.06", need: "615" } },
    { variant: "cuda-12", verdict: "recommended", reason: null },
    { variant: "rocm", verdict: "incompatible", reason: { code: "no_gpu" } },
    { variant: "vulkan", verdict: "compatible", reason: null },
    { variant: "cpu", verdict: "compatible", reason: null },
  ],
};

const AMD: EnginesResponse = {
  ...NVIDIA,
  engines: [EMBEDDED],
  hardware: {
    cards: [
      card({
        vendor: "amd",
        name: "AMD Radeon RX 7900 XTX",
        driver_version: null,
        compute_capability: null,
        gfx_target: "gfx1100",
        kfd_accessible: true,
      }),
    ],
    vulkan_loader: false,
    rocm_runtime: true,
  },
};

const NO_CARD: EnginesResponse = {
  ...NVIDIA,
  engines: [EMBEDDED],
  hardware: { cards: [], vulkan_loader: false, rocm_runtime: false },
};

function page(response: EnginesResponse, kind: HardwareProfile["accelerator"]["kind"] = "cuda", os = "linux x86_64"): string {
  enginesStore.set(loaded(response));
  hardwareStore.set(loaded(profile(kind)));
  systemInfoStore.set(
    loaded({ version: "0.3.1", os, arch: "x86_64", python_path: null, data_dir: "/home/ada/.apollia" }),
  );
  return renderToString(Engines).body;
}

beforeAll(async () => {
  locale.set("fr");
  await waitLocale("fr");
});

beforeEach(() => {
  ipc.invoke.mockReset();
  ipc.invoke.mockImplementation(async () => null);
  developerMode.set(false);
  engineSettings.set(DEFAULT_ENGINE_SETTINGS);
});

describe("Engines - the hardware card", () => {
  test("an NVIDIA card shows its name, maker, driver and memory, and the Vulkan loader", () => {
    // GIVEN the engine list of a computer with an NVIDIA card
    // WHEN System > Engines renders
    const html = page(NVIDIA);

    // THEN the hardware card comes first and names the card
    expect(html.indexOf('data-testid="engine-hardware"')).toBeLessThan(html.indexOf('data-testid="engines-list"'));
    const row = text(blockOf(html, "engine-hardware-card-0"));
    expect(row).toContain("NVIDIA GeForce RTX 4090");
    expect(row).toContain("NVIDIA");
    expect(row).toContain(fill(copy.hardware_driver, { version: "580.65.06" }));
    expect(row).toContain(fill(fr.settings.hardware.gb, { n: "24.0" }));
    // AND it says a Vulkan loader is installed
    expect(text(blockOf(html, "engine-hardware-vulkan"))).toBe(copy.hardware_vulkan_yes);
    expect(blockOf(html, "engine-hardware-no-card")).toBeNull();
  });

  test("an AMD card shows its maker and memory, an unreported driver, and no Vulkan loader", () => {
    // GIVEN the engine list of a computer with an AMD card and no Vulkan loader
    // WHEN System > Engines renders
    const html = page(AMD, "generic");

    // THEN the card is named with its maker, and the missing pieces are said
    const row = text(blockOf(html, "engine-hardware-card-0"));
    expect(row).toContain("AMD Radeon RX 7900 XTX");
    expect(row).toContain(copy.hardware_driver_unknown);
    expect(text(blockOf(html, "engine-hardware-vulkan"))).toBe(copy.hardware_vulkan_no);
  });

  test("without a card a sentence says so, and an Apple chip reads as the embedded Metal engine", () => {
    // GIVEN a computer with no graphics card, then a Mac, then a probe that did not answer
    // WHEN System > Engines renders each
    const none = page(NO_CARD, "none");
    const mac = page(NO_CARD, "apple_silicon", "macos aarch64");
    const late = page({ ...NVIDIA, hardware: null });

    // THEN each says it in one sentence, with no card row
    expect(text(blockOf(none, "engine-hardware-no-card"))).toBe(copy.hardware_none);
    expect(blockOf(none, "engine-hardware-card-0")).toBeNull();
    expect(text(blockOf(mac, "engine-hardware-no-card"))).toBe(copy.hardware_apple);
    expect(text(blockOf(late, "engine-hardware-unknown"))).toBe(copy.hardware_unknown);
  });
});

describe("Engines - the variants of the channel", () => {
  test("each variant offered shows the verdict and the translated reason of the runtime", () => {
    // GIVEN the runtime rated every variant of an NVIDIA computer
    // WHEN System > Engines renders
    const html = page(NVIDIA);

    // THEN the recommended one comes first, then the compatible ones, in the runtime's order
    const order = [...html.matchAll(/<li[^>]*data-testid="engine-variant-row-([a-z0-9-]+)"/g)].map((m) => m[1]);
    expect(order).toEqual(["cuda-12", "cuda-13", "vulkan", "cpu"]);
    expect(text(blockOf(html, "engine-variant-row-cuda-12-verdict"))).toBe(copy.verdict_recommended);
    expect(text(blockOf(html, "engine-variant-row-cuda-13-verdict"))).toBe(copy.verdict_compatible);
    // AND an old driver names the driver present and the one required
    expect(text(blockOf(html, "engine-variant-row-cuda-13-reason"))).toBe(
      fill(copy.reason.driver_too_old, { have: "580.65.06", need: "615" }),
    );
    // AND an installed variant says so, one not installed does not
    expect(blockOf(html, "engine-variant-row-cuda-12-installed")).not.toBeNull();
    expect(blockOf(html, "engine-variant-row-cuda-13-installed")).toBeNull();
  });

  test("every reason code of the runtime has its sentence in both catalogues", () => {
    // GIVEN one reason of each code
    const reasons: EngineRatingReason[] = [
      { code: "driver_too_old", have: "560", need: "570" },
      { code: "arch_dropped_in_cuda13", cc: "6.1" },
      { code: "gfx_not_in_build", gfx: "gfx906" },
      { code: "no_vulkan_loader" },
      { code: "no_nvidia_driver" },
      { code: "not_built_for_platform" },
      { code: "no_gpu" },
      { code: "engine_sees_no_device" },
      { code: "no_rocm_runtime" },
    ];

    // WHEN each is worded, on a compatible and on a recommended variant
    const keys = reasons.flatMap((r) => [reasonSentence(r, "compatible"), reasonSentence(r, "recommended")]);

    // THEN every key exists in FR and EN, and the values carry the facts
    for (const { key } of keys) {
      const leaf = key.replace("settings.engines.reason.", "");
      expect((fr.settings.engines.reason as Record<string, string>)[leaf], key).toBeTruthy();
      expect((en.settings.engines.reason as Record<string, string>)[leaf], key).toBeTruthy();
    }
    expect(reasonSentence(reasons[1], "incompatible").values).toEqual({ cc: "6.1" });
    expect(reasonSentence(reasons[2], "incompatible").values).toEqual({ gfx: "gfx906" });
  });

  test("the incompatible variants are folded behind Show incompatible, as --all is in the CLI", () => {
    // GIVEN a list holding one incompatible variant
    const list = variantList(NVIDIA);
    const rows = list.kind === "rows" ? list.rows : [];

    // WHEN the page renders, then the fold is opened
    const html = page(NVIDIA);

    // THEN the incompatible variant is not shown, and the button counts it
    expect(blockOf(html, "engine-variant-row-rocm")).toBeNull();
    expect(text(blockOf(html, "engine-variant-toggle-incompatible"))).toBe(
      fill(copy.variants_show_incompatible, { count: 1 }),
    );
    expect(incompatibleCount(rows)).toBe(1);
    // AND opened, it comes last with its verdict and reason
    const all = shownVariants(rows, true);
    expect(all.map((r) => r.variant)).toEqual(["cuda-12", "cuda-13", "vulkan", "cpu", "rocm"]);
    expect(all.at(-1)?.verdict).toBe("incompatible");
    expect(shownVariants(rows, false).some((r) => r.verdict === "incompatible")).toBe(false);
    // AND a list with no incompatible variant has no button
    expect(blockOf(page({ ...NVIDIA, ratings: NVIDIA.ratings.slice(0, 2) }), "engine-variant-toggle-incompatible")).toBeNull();
  });

  test("the variant names of the runtime each have a label: cuda-12, cuda-13 and rocm", () => {
    // GIVEN an answer of the runtime that carries cuda-12, cuda-13 and rocm
    const response: EnginesResponse = {
      ...NVIDIA,
      engines: [EMBEDDED, installed("cuda-12"), installed("cuda-13"), installed("rocm")],
      ratings: NVIDIA.ratings.map((r) => ({ ...r, verdict: "compatible" as const, reason: null })),
    };

    // WHEN the page renders
    const html = page(response);

    // THEN each variant is named, in the list of the channel and in the engine list
    expect(text(blockOf(html, "engine-variant-row-cuda-12"))).toContain("llama.cpp · CUDA 12");
    expect(text(blockOf(html, "engine-variant-row-cuda-13"))).toContain("llama.cpp · CUDA 13");
    expect(text(blockOf(html, "engine-variant-row-rocm"))).toContain("llama.cpp · ROCm");
    expect(html).toContain("llama.cpp · CUDA 13</span>");
    // AND each of the six names has its label
    const cpu = copy.variant_cpu;
    expect(["cuda-12", "cuda-13", "rocm", "vulkan", "cpu", "metal"].map((v) => variantLabel(v as EngineEntry["variant"], cpu))).toEqual([
      "CUDA 12",
      "CUDA 13",
      "ROCm",
      "Vulkan",
      cpu,
      "Metal",
    ]);
  });
});

describe("Engines - three cases of the variant list", () => {
  test("a recommended variant with an old driver shows the badge and what to do about it", () => {
    // GIVEN CUDA 12 recommended on a driver below the one it is best with
    const response: EnginesResponse = {
      ...NVIDIA,
      ratings: [
        { variant: "cuda-12", verdict: "recommended", reason: { code: "driver_too_old", have: "560", need: "570" } },
      ],
      available: [pkg("cuda-12")],
    };

    // WHEN the page renders
    const html = page(response);

    // THEN the badge says Recommended and the reason below reads as an action
    expect(text(blockOf(html, "engine-variant-row-cuda-12-verdict"))).toBe(copy.verdict_recommended);
    expect(text(blockOf(html, "engine-variant-row-cuda-12-reason"))).toBe(
      fill(copy.reason.driver_too_old_action, { have: "560", need: "570" }),
    );
    expect(en.settings.engines.reason.driver_too_old_action).toBe(
      "Your driver {have} is old: update it to {need} or later for better performance.",
    );
  });

  test("ROCm under Windows, compatible without a reason, reads as experimental", () => {
    // GIVEN ROCm rated compatible with no reason, on Windows then on Linux
    const response: EnginesResponse = {
      ...AMD,
      available: [pkg("rocm"), pkg("vulkan")],
      ratings: [
        { variant: "vulkan", verdict: "recommended", reason: null },
        { variant: "rocm", verdict: "compatible", reason: null },
      ],
    };

    // WHEN the page renders on each system
    const windows = page(response, "generic", "windows x86_64");
    const linux = page(response, "generic", "linux x86_64");

    // THEN Windows labels it experimental, Linux does not
    expect(text(blockOf(windows, "engine-variant-row-rocm-experimental"))).toBe(copy.variant_experimental_windows);
    expect(en.settings.engines.variant_experimental_windows).toBe("Experimental on Windows");
    expect(blockOf(linux, "engine-variant-row-rocm-experimental")).toBeNull();
  });

  test("before the catalogue was read the page says so with a Check button, and the hardware card still shows", async () => {
    // GIVEN no manifest in cache: nothing available, nothing rated
    const unread: EnginesResponse = { ...NVIDIA, manifest_sequence: null, available: [], ratings: [] };

    // WHEN the page renders, then the check is asked
    const html = page(unread);
    ipc.invoke.mockImplementation(async (command: string) => (command === "list_engines" ? NVIDIA : null));
    await verifyEngineUpdates();

    // THEN the sentence and the button are there, and the hardware card too
    expect(text(blockOf(html, "engine-variant-unread"))).toContain(copy.variants_unread);
    expect(text(blockOf(html, "engine-variant-check"))).toBe(copy.variants_check);
    expect(blockOf(html, "engine-hardware-card-0")).not.toBeNull();
    expect(blockOf(html, "engine-variant-row-cuda-12")).toBeNull();
    // AND the check is the verified check of the channel, then the list read again
    expect(ipc.invoke.mock.calls.map((c) => c[0])).toEqual(["check_engine_updates", "list_engines"]);
    // AND a channel that offers nothing for this system says the shipped engine serves
    expect(variantList({ ...NVIDIA, available: [], ratings: [] })).toEqual({ kind: "none" });
  });
});

describe("Engines - the engine of each format", () => {
  test("the GGUF list offers Automatic, the shipped engine and the installed variants, an incompatible one marked", () => {
    // GIVEN CUDA 12 and ROCm installed, ROCm rated incompatible
    // WHEN System > Engines renders
    const html = page(NVIDIA);

    // THEN the list holds those choices, read from the engine list
    const gguf = selectOf(html, "engine-format-gguf-select");
    const values = [...gguf.matchAll(/<option value="([^"]+)"/g)].map((m) => m[1]);
    expect(values).toEqual(["auto", "embedded", "cuda-12", "rocm"]);
    expect(engineOptions(NVIDIA, "gguf").map((o) => [o.value, o.incompatible])).toEqual([
      ["embedded", false],
      ["cuda-12", false],
      ["rocm", true],
    ]);
    // AND the incompatible one is marked
    expect(gguf).toContain(fill(copy.option_incompatible, { name: "llama.cpp · ROCm · 7100 (c0ffee0)" }));
    // AND the hint says the choice applies at the next load of a model
    expect(html).toContain(copy.format_gguf_hint);
    expect(en.settings.engines.format_gguf_hint).toContain("the next time a model is loaded");
  });
});
