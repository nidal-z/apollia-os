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

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(async () => {}),
  revealItemInDir: vi.fn(async () => {}),
}));

// The tooltip primitive needs the provider the application shell mounts; here
// it only has to hand its trigger through, so the "?" is rendered bare.
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (out: unknown, props: { children?: (out: unknown) => void }) => props.children?.(out),
}));

import System from "./System.svelte";
import ModelDefaults from "./ModelDefaults.svelte";
import GpuDevices from "../../components/settings/system/GpuDevices.svelte";
import HardwareHeaderActions from "../../components/settings/system/HardwareHeaderActions.svelte";
import GuardrailsSetting from "../../components/settings/model-defaults/GuardrailsSetting.svelte";
import { COMMUNITY_URL } from "$lib/community";
import {
  DISABLED_DEVICES_KEY,
  canRestoreDefault,
  gpuRows,
  hardwareInfoText,
  nextDisabled,
  sharedApi,
  type HardwareInfoLabels,
} from "$lib/settings/hardwareDevices";
import { initialModelDefaults } from "$lib/settings/modelDefaults";
import { hardwareStore, systemInfoStore } from "$lib/stores/settings";
import type { HardwareProfile } from "$lib/ipc/modelHub";
import fr from "$lib/i18n/fr.json";

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** Every opening tag carrying `data-testid="<testid>"`. */
function tagsOf(html: string, testid: string): string[] {
  return html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`, "g")) ?? [];
}

/** Whether an opening tag carries the `disabled` attribute. */
function isDisabled(tag: string | null): boolean {
  return tag !== null && /\sdisabled(=""|\s|\/?>)/.test(tag);
}

function loaded<T>(data: T) {
  return { data, loading: false, error: null, loaded: true };
}

/** A machine with two NVIDIA cards, the second held back. */
const TWO_CARDS: HardwareProfile = {
  total_ram_gb: 64,
  available_ram_gb: 40,
  cpu_model: "AMD Ryzen 9 7950X",
  cpu_cores: 32,
  accelerator: { kind: "cuda", device_name: "NVIDIA GeForce RTX 4090", vram_gb: 24 },
  memory_budget_gb: 24,
  total_capacity_gb: 62.4,
  devices: [
    { id: "CUDA0", name: "NVIDIA GeForce RTX 4090", api: "CUDA", vram_gb: 24, enabled: true },
    { id: "CUDA1", name: "NVIDIA GeForce RTX 3090", api: "CUDA", vram_gb: 24, enabled: false },
  ],
};

/** An Apple machine: one device, unified memory. */
const APPLE: HardwareProfile = {
  total_ram_gb: 256,
  available_ram_gb: 180,
  cpu_model: "Apple M3 Ultra",
  cpu_cores: 32,
  accelerator: { kind: "apple_silicon", chip: "M3 Ultra", generation: 3, vram_gb: 256 },
  memory_budget_gb: 192,
  total_capacity_gb: 192,
  devices: [{ id: "MTL0", name: "Apple M3 Ultra", api: "Metal", vram_gb: 222.72, enabled: true }],
};

const LABELS: HardwareInfoLabels = {
  os: "Operating system",
  processor: "Processor",
  cores: "cores",
  memory: "Memory",
  gpu: "GPU",
  noGpu: "None",
  off: "off",
  engines: "Engines",
  unknown: "unknown",
  python: "Python path",
  config: "Configuration file",
  dataDir: "Data directory",
};

beforeAll(async () => {
  locale.set("fr");
  await waitLocale("fr");
});

beforeEach(() => {
  hardwareStore.set(loaded(TWO_CARDS));
});

describe("Hardware lists the graphics devices one by one", () => {
  test("each device has its line, its facts and its switch, and the default can be restored", () => {
    // GIVEN two cards the engine listed, the second held back
    // WHEN the list renders
    const html = renderToString(GpuDevices, {
      props: { hardware: TWO_CARDS, onreload: async () => {} },
    }).body;

    // THEN there is one line per card, with its name, memory, interface and
    //      identifier, and a switch on for the first and off for the second
    const rows = tagsOf(html, "hardware-gpu-row");
    expect(rows.length).toBe(2);
    expect(rows[0]).toContain('data-device="CUDA0"');
    expect(rows[0]).toContain('data-enabled="true"');
    expect(rows[1]).toContain('data-device="CUDA1"');
    expect(rows[1]).toContain('data-enabled="false"');
    expect(html).toContain("NVIDIA GeForce RTX 3090");
    expect(html).toMatch(/24\.0 Go · CUDA · [^<]*CUDA1/);
    expect(tagOf(html, "hardware-gpu-count")).not.toBeNull();
    expect(html).toMatch(/>\s*2 GPU [^<]*, CUDA\s*</);
    const switches = tagsOf(html, "hardware-gpu-switch");
    expect(switches.map((tag) => /aria-checked="true"/.test(tag))).toEqual([true, false]);
    expect(switches.some(isDisabled)).toBe(false);
    // AND "Back to the default" has something to undo, and the window opens from here
    expect(isDisabled(tagOf(html, "hardware-gpu-restore-default"))).toBe(false);
    expect(html).toContain(fr.settings.hardware.gpu_restore_default);
    expect(tagOf(html, "hardware-open-window")).not.toBeNull();
  });

  test("a switch writes the identifiers held back, and the default is the empty list", () => {
    // GIVEN the two cards, the second held back
    const { devices } = TWO_CARDS;

    // WHEN the first is turned off, the second turned on, and nothing changes
    // THEN the setting names exactly the devices left off
    expect(DISABLED_DEVICES_KEY).toBe("llm.devices.disabled");
    expect(nextDisabled(devices, "CUDA0", false)).toEqual(["CUDA0", "CUDA1"]);
    expect(nextDisabled(devices, "CUDA1", true)).toEqual([]);
    expect(nextDisabled(devices, "CUDA1", false)).toEqual(["CUDA1"]);
    // AND with every device on there is nothing to restore
    const allOn = { ...TWO_CARDS, devices: devices.map((d) => ({ ...d, enabled: true })) };
    expect(canRestoreDefault(gpuRows(TWO_CARDS, "user"))).toBe(true);
    expect(canRestoreDefault(gpuRows(allOn, "user"))).toBe(false);
  });

  test("a list the policy fixes keeps its switches where they are and names the policy", () => {
    // GIVEN the two cards, the list of devices held back fixed by the policy
    // WHEN the list renders
    const html = renderToString(GpuDevices, {
      props: { hardware: TWO_CARDS, onreload: async () => {}, initialSource: "policy" },
    }).body;

    // THEN every switch is disabled, the policy is named, nothing can be restored
    expect(tagsOf(html, "hardware-gpu-switch").every(isDisabled)).toBe(true);
    expect(tagOf(html, "hardware-gpu-policy")).not.toBeNull();
    expect(isDisabled(tagOf(html, "hardware-gpu-restore-default"))).toBe(true);
  });

  test("an Apple machine shows its single GPU with a switch that cannot be moved, and says why", () => {
    // GIVEN an Apple machine, whose only device a settings file even names
    const named = { ...APPLE, devices: [{ ...APPLE.devices[0], enabled: false }] };

    // WHEN the list renders
    const html = renderToString(GpuDevices, {
      props: { hardware: named, onreload: async () => {} },
    }).body;
    const rows = gpuRows(named, "user");

    // THEN the device is on, its switch is shown disabled, the explanation
    //      names the single GPU and the MLX engine, and nothing can be restored
    expect(rows).toHaveLength(1);
    expect(rows[0].enabled).toBe(true);
    expect(rows[0].lock).toBe("single_unified");
    const toggle = tagOf(html, "hardware-gpu-switch");
    expect(toggle).toContain('aria-checked="true"');
    expect(isDisabled(toggle)).toBe(true);
    expect(html).toContain(fr.settings.hardware.gpu_single_note);
    expect(fr.settings.hardware.gpu_single_note).toContain("MLX");
    expect(html).toMatch(/>\s*1 GPU [^<]*, Metal\s*</);
    expect(isDisabled(tagOf(html, "hardware-gpu-restore-default"))).toBe(true);
  });

  test("a device the engine did not list is shown without a switch", () => {
    // GIVEN a card described from the hardware profile, without an identifier
    const unlisted: HardwareProfile = {
      ...TWO_CARDS,
      devices: [{ id: null, name: "NVIDIA GeForce RTX 4090", api: "CUDA", vram_gb: 24, enabled: true }],
    };

    // WHEN the list renders
    const html = renderToString(GpuDevices, {
      props: { hardware: unlisted, onreload: async () => {} },
    }).body;

    // THEN the card is named, no switch is offered, and the reason is given
    expect(html).toContain("NVIDIA GeForce RTX 4090");
    expect(tagOf(html, "hardware-gpu-switch")).toBeNull();
    expect(tagOf(html, "hardware-gpu-unlisted-note")).not.toBeNull();
    expect(sharedApi(gpuRows(unlisted, "default"))).toBe("CUDA");
  });

  test("inside its own window the list has no button to open that window", () => {
    // GIVEN the list mounted in the hardware window
    // WHEN it renders
    const html = renderToString(GpuDevices, {
      props: { hardware: TWO_CARDS, onreload: async () => {}, windowed: true },
    }).body;

    // THEN the devices are there, the button is not
    expect(tagsOf(html, "hardware-gpu-row").length).toBe(2);
    expect(tagOf(html, "hardware-open-window")).toBeNull();
  });
});

describe("Hardware recalls the load guardrails of Model defaults", () => {
  test("the page mounts the guardrails, same choices and same keys as Model defaults", () => {
    // GIVEN the hardware page and the model defaults page
    systemInfoStore.set(
      loaded({ version: "0.3.0", os: "Windows 11", arch: "x86_64", python_path: null, data_dir: "C:/a" } as never),
    );

    // WHEN both render
    const hardware = renderToString(System).body;
    const defaults = renderToString(ModelDefaults).body;

    // THEN each shows the one guardrail block, with the five same choices,
    //      "balanced" chosen
    for (const html of [hardware, defaults]) {
      expect(tagsOf(html, "model-defaults-guardrails").length).toBe(1);
      for (const level of ["disabled", "lenient", "balanced", "strict", "ceiling"]) {
        expect(tagOf(html, `model-defaults-guardrail-${level}-row`)).not.toBeNull();
      }
      expect(tagOf(html, "model-defaults-guardrail-balanced-row")).toContain('data-checked="true"');
    }
    // AND on the hardware page it comes after the devices and the monitor
    const order = ["hardware-gpu", "resource-monitor", "model-defaults-guardrails"].map((id) =>
      hardware.indexOf(`data-testid="${id}"`),
    );
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
  });

  test("guardrails the policy fixes are disabled and say 'imposed by the policy'", () => {
    // GIVEN the guardrail level fixed by the organization policy
    const state = initialModelDefaults();
    state.sources.guardrailLevel = "policy";
    state.values.guardrailLevel = "strict";

    // WHEN the block renders
    const html = renderToString(GuardrailsSetting, { props: { initial: state } }).body;

    // THEN the imposed level stays shown, no choice can be moved, and the
    //      policy is named
    expect(tagOf(html, "model-defaults-guardrail-strict-row")).toContain('data-checked="true"');
    expect(tagOf(html, "model-defaults-guardrail-selector")).toContain('aria-disabled="true"');
    expect(tagOf(html, "model-defaults-guardrails-policy")).not.toBeNull();
    expect(html).toContain(fr.settings.policy_locked);
  });
});

describe("Copy the information", () => {
  test("the text carries the version, the system, the processor, the memory, the GPUs and the engines", () => {
    // GIVEN the facts of a machine with two cards, the developer mode off
    // WHEN the text is composed
    const text = hardwareInfoText(
      {
        version: "0.3.0",
        os: "Windows 11",
        arch: "x86-64",
        hardware: TWO_CARDS,
        engines: [
          { name: "llama.cpp · CUDA", version: "10092 (3ce7da2c8)" },
          { name: "whisper.cpp", version: null },
        ],
        paths: null,
      },
      LABELS,
    );

    // THEN it is these lines, in this order, and no path is in it
    expect(text.split("\n")).toEqual([
      "Apollia OS v0.3.0",
      "Operating system: Windows 11",
      "Processor: AMD Ryzen 9 7950X, x86-64, 32 cores",
      "Memory: 64.0 GB",
      "GPU: NVIDIA GeForce RTX 4090, 24.0 GB, CUDA, CUDA0",
      "GPU: NVIDIA GeForce RTX 3090, 24.0 GB, CUDA, CUDA1, off",
      "Engines:",
      "  llama.cpp · CUDA: 10092 (3ce7da2c8)",
      "  whisper.cpp: unknown",
    ]);
  });

  test("the paths are in the text in developer mode only", () => {
    // GIVEN the same machine, with the paths the developer mode gives
    const input = {
      version: "0.3.0",
      os: "macOS 15",
      arch: "ARM64",
      hardware: APPLE,
      engines: [],
      paths: { python: "/app/python/bin/python3", config: "/Users/u/.apollia/apollia.toml", dataDir: null },
    };

    // WHEN the text is composed with and without them
    const withPaths = hardwareInfoText(input, LABELS);
    const without = hardwareInfoText({ ...input, paths: null }, LABELS);

    // THEN the two paths that are known close the text, and are absent otherwise
    expect(withPaths.split("\n").slice(-2)).toEqual([
      "Python path: /app/python/bin/python3",
      "Configuration file: /Users/u/.apollia/apollia.toml",
    ]);
    expect(without).not.toContain("/app/python");
    expect(without).not.toContain("apollia.toml");
  });

  test("a machine without a GPU says so", () => {
    // GIVEN a processor-only machine
    const bare = { ...TWO_CARDS, accelerator: { kind: "none" as const }, devices: [] };

    // WHEN the text is composed
    const text = hardwareInfoText(
      { version: null, os: null, arch: null, hardware: bare, engines: [], paths: null },
      LABELS,
    );

    // THEN the GPU line says none, and unknown facts say unknown
    expect(text).toContain("GPU: None");
    expect(text.split("\n")[0]).toBe("Apollia OS unknown");
  });
});

describe("The header of Hardware", () => {
  test("carries 'Copy the information' and a Community link to the one community address", () => {
    // GIVEN the header actions of Hardware
    // WHEN they render
    const html = renderToString(HardwareHeaderActions).body;

    // THEN the copy button is there, and the link carries the address the
    //      help resources use, the shared constant
    expect(tagOf(html, "hardware-copy-info")).not.toBeNull();
    expect(html).toContain(fr.settings.hardware.copy_info);
    const link = tagOf(html, "hardware-community-link");
    expect(link).toContain(`href="${COMMUNITY_URL}"`);
    expect(COMMUNITY_URL).toMatch(/^https:\/\/github\.com\/.+\/discussions$/);
    expect(html).toContain(fr.settings.hardware.community);
  });
});
