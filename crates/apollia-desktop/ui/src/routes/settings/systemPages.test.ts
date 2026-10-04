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

import Engines from "./Engines.svelte";
import System from "./System.svelte";
import { engineRows, variantLabel } from "../../components/settings/system/EngineList.svelte";
import {
  archLabel,
  gpuBackend,
  memoryKind,
  vramGb,
} from "../../components/settings/system/HardwareCard.svelte";
import { developerMode } from "$lib/stores/developerMode";
import {
  cliStatusStore,
  configStore,
  enginesStore,
  hardwareStore,
  pythonInterpreterStore,
  securityPostureStore,
  systemInfoStore,
} from "$lib/stores/settings";
import type { EnginesResponse } from "$lib/ipc/system";
import fr from "$lib/i18n/fr.json";

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

function loaded<T>(data: T) {
  return { data, loading: false, error: null, loaded: true };
}

const ENGINES: EnginesResponse = {
  engines: [
    {
      engine: "llama-server",
      tag: null,
      variant: "metal",
      path: "/app/Contents/Resources/runners/llama-server",
      source: "embedded",
      version: "7001 (0ff1ce0)",
    },
    {
      engine: "vllm-mlx",
      tag: null,
      variant: "metal",
      path: "/app/Contents/Resources/python/engines/mlx",
      source: "embedded",
      version: null,
    },
  ],
  channel: "stable",
  manifest_sequence: null,
  updates: [],
  cuda_proposable: false,
  dictation: { path: "/app/Contents/Resources/runners/apollia-runner-metal", version: "1.7.6" },
};

function seedHardwarePage(): void {
  systemInfoStore.set(
    loaded({
      version: "0.3.0",
      os: "macos aarch64",
      arch: "aarch64",
      python_path: "/usr/bin/python3",
      data_dir: "/Users/u/.apollia",
    }),
  );
  securityPostureStore.set(
    loaded({
      platform: "macos aarch64",
      tool_sandbox: "dev_no_sandbox",
      agent_execution: "in_process_trusted",
      rlimits_active: true,
      unshare_available: false,
    }),
  );
  cliStatusStore.set(
    loaded({
      bundled: true,
      bundled_path: "/app/bin/apollia-os",
      installed: true,
      symlink_path: "/usr/local/bin/apollia-os",
      version: "0.3.0",
      needs_privilege: false,
    }),
  );
  configStore.set(
    loaded({ config_path: "/Users/u/.apollia/apollia.toml", config_exists: true, sections: [] }),
  );
  hardwareStore.set(
    loaded({
      total_ram_gb: 256,
      available_ram_gb: 180,
      cpu_model: "Apple M3 Ultra",
      cpu_cores: 32,
      accelerator: { kind: "apple_silicon", chip: "M3 Ultra", generation: 3, vram_gb: 222.72 },
      memory_budget_gb: 167,
      total_capacity_gb: 222.7,
    }),
  );
  pythonInterpreterStore.set(
    loaded({ chosen: "/opt/homebrew/bin/python3.13", bundled: "/app/python/bin/python3", required_minor: 13 }),
  );
}

beforeAll(async () => {
  locale.set("fr");
  await waitLocale("fr");
});

beforeEach(() => {
  seedHardwarePage();
  enginesStore.set(loaded(ENGINES));
});

describe("System is two sections, Engines and Hardware", () => {
  test("Engines lists each engine with the version it stated, and the dictation engine", () => {
    // GIVEN the engine list the runtime answered
    // WHEN System > Engines renders
    const html = renderToString(Engines).body;

    // THEN the section and its list are there, one row per engine and one for
    //      dictation, each version as the engine stated it, and an engine that
    //      stated none says so rather than showing a compiled-in tag
    expect(tagOf(html, "engines-section")).not.toBeNull();
    expect(tagOf(html, "engines-list")).not.toBeNull();
    expect(html.match(/data-testid="engine-row"/g)?.length).toBe(3);
    expect(html).toContain("llama.cpp · Metal");
    expect(html).toContain("7001 (0ff1ce0)");
    expect(html).toContain("whisper.cpp");
    expect(html).toContain("1.7.6");
    expect(tagOf(html, "engine-row-version-unknown")).not.toBeNull();
    expect(html).not.toContain("b10092");
    // AND no hardware block sits in Engines
    expect(tagOf(html, "hardware-processor")).toBeNull();
  });

  test("Hardware shows the processor, the memory and the GPU from the single probe", () => {
    // GIVEN the hardware profile the inference probed, on an Apple chip
    developerMode.set(false);

    // WHEN System > Hardware renders
    const html = renderToString(System).body;

    // THEN the processor carries Compatible, its name and architecture, the
    //      memory its kind, RAM and VRAM, and the GPU is named
    expect(tagOf(html, "advanced-section")).not.toBeNull();
    expect(tagOf(html, "hardware-processor-compatible")).not.toBeNull();
    expect(html).toContain("Apple M3 Ultra");
    expect(html).toContain("ARM64");
    expect(html).toContain(fr.settings.hardware.memory_unified);
    expect(html).toContain("256 Go");
    expect(html).toContain("223 Go");
    expect(tagOf(html, "hardware-gpu-name")).not.toBeNull();
    // AND no engine list sits in Hardware
    expect(tagOf(html, "engines-list")).toBeNull();
  });
});

describe("Hardware - the system information follows the developer mode", () => {
  test("off, version, operating system and posture are shown, the developer facts are not", () => {
    // GIVEN the developer mode off
    developerMode.set(false);

    // WHEN System > Hardware renders
    const html = renderToString(System).body;

    // THEN version, OS and the security posture are there
    expect(tagOf(html, "system-info-section")).not.toBeNull();
    expect(html).toContain("0.3.0");
    expect(html).toContain("macos aarch64");
    expect(tagOf(html, "security-posture-section")).not.toBeNull();
    // AND Python, the configuration path, the CLI and the automatic install are absent
    expect(tagOf(html, "system-info-copy-python")).toBeNull();
    expect(tagOf(html, "system-info-copy-config")).toBeNull();
    expect(tagOf(html, "cli-section")).toBeNull();
    expect(tagOf(html, "agent-install-section")).toBeNull();
  });

  test("on, Python, the configuration path, the CLI and the automatic install join them", () => {
    // GIVEN the developer mode on
    developerMode.set(true);

    // WHEN System > Hardware renders
    const html = renderToString(System).body;

    // THEN the always-visible facts stay, and the developer ones are added
    expect(tagOf(html, "system-info-section")).not.toBeNull();
    expect(tagOf(html, "security-posture-section")).not.toBeNull();
    expect(tagOf(html, "system-info-copy-python")).not.toBeNull();
    expect(tagOf(html, "system-info-copy-config")).not.toBeNull();
    expect(tagOf(html, "cli-section")).not.toBeNull();
    expect(tagOf(html, "agent-install-section")).not.toBeNull();
  });
});

describe("Hardware - the Python interpreter choice is a developer setting", () => {
  test("on, the field, Save and Back to automatic are there", () => {
    // GIVEN the developer mode on and a chosen interpreter
    developerMode.set(true);

    // WHEN System > Hardware renders
    const html = renderToString(System).body;

    // THEN the interpreter section holds its field and both actions
    expect(tagOf(html, "python-interpreter-section")).not.toBeNull();
    expect(tagOf(html, "python-interpreter-input")).not.toBeNull();
    expect(tagOf(html, "python-interpreter-save-btn")).not.toBeNull();
    expect(tagOf(html, "python-interpreter-reset-btn")).not.toBeNull();
    expect(html).toContain(fr.settings.system.python_interpreter_use_bundled);
  });

  test("off, none of it is rendered", () => {
    // GIVEN the developer mode off and the same chosen interpreter
    developerMode.set(false);

    // WHEN System > Hardware renders
    const html = renderToString(System).body;

    // THEN the section and its controls are absent
    expect(tagOf(html, "python-interpreter-section")).toBeNull();
    expect(tagOf(html, "python-interpreter-input")).toBeNull();
    expect(tagOf(html, "python-interpreter-save-btn")).toBeNull();
    expect(tagOf(html, "python-interpreter-reset-btn")).toBeNull();
  });
});

describe("the pure helpers of the two sections", () => {
  test("engine rows carry the stated version and nothing else", () => {
    // GIVEN the runtime's answer, then no answer yet
    // WHEN rows are built
    const rows = engineRows(ENGINES);

    // THEN the versions are the stated ones, in order, dictation last
    expect(rows.map((r) => [r.name, r.version])).toEqual([
      ["llama.cpp", "7001 (0ff1ce0)"],
      ["vllm-mlx", null],
      ["whisper.cpp", "1.7.6"],
    ]);
    expect(engineRows(null)).toEqual([]);
    expect(variantLabel("cpu", "Processeur")).toBe("Processeur");
    expect(variantLabel(null, "Processeur")).toBeNull();
  });

  test("the hardware is described from the accelerator and the architecture", () => {
    // GIVEN the accelerator kinds the probe reports
    // WHEN they are described
    // THEN unified memory is Apple's, dedicated a card's with memory, and nothing is invented
    expect(memoryKind({ kind: "apple_silicon", vram_gb: 64 })).toBe("unified");
    expect(memoryKind({ kind: "cuda", vram_gb: 24 })).toBe("dedicated");
    expect(memoryKind({ kind: "generic", vram_gb: 0 })).toBeNull();
    expect(memoryKind({ kind: "none" })).toBeNull();
    expect(vramGb({ kind: "none" })).toBeNull();
    expect(vramGb({ kind: "cuda", vram_gb: 24 })).toBe(24);
    expect(gpuBackend({ kind: "cuda" })).toBe("CUDA");
    expect(gpuBackend({ kind: "none" })).toBeNull();
    expect(archLabel("aarch64")).toBe("ARM64");
    expect(archLabel("x86_64")).toBe("x86-64");
    expect(archLabel(null)).toBeNull();
  });
});
