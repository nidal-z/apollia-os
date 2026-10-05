import { describe, test, expect, vi, beforeAll, beforeEach, afterEach } from "vitest";
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

// The tooltip primitive needs the provider the application shell mounts; here
// it only has to hand its trigger through, so the "?" of the guardrails is
// rendered bare.
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (out: unknown, props: { children?: (out: unknown) => void }) => props.children?.(out),
}));

import Engines from "./Engines.svelte";
import System from "./System.svelte";
import ResourceMonitor, {
  gaugeTone,
  sharePercent,
} from "../../components/settings/system/ResourceMonitor.svelte";
import { versionMatchesTag } from "../../components/settings/system/EngineList.svelte";
import { createResourceMonitor } from "$lib/settings/resourceMonitor";
import { changeEngineChannel, verifyEngineUpdates } from "$lib/settings/engineChannel";
import { developerMode } from "$lib/stores/developerMode";
import { enginesStore, systemInfoStore } from "$lib/stores/settings";
import type { EnginesResponse, ResourceUsage } from "$lib/ipc/system";
import fr from "$lib/i18n/fr.json";

const copy = fr.settings;

/** A catalogue sentence with its `{name}` placeholders filled. */
function fill(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) => values[name] ?? "");
}

const gb = (n: string): string => fill(copy.hardware.gb, { n });
const usedOf = (used: string, total: string): string =>
  fill(copy.resource_monitor.used_of, { used: gb(used), total: gb(total) });
const available = (tag: string): string => fill(copy.engines.update_available, { tag });

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

function loaded<T>(data: T) {
  return { data, loading: false, error: null, loaded: true };
}

/** The names of the commands invoked so far, in order. */
function invoked(): string[] {
  return ipc.invoke.mock.calls.map((call) => call[0]);
}

const USAGE: ResourceUsage = {
  ram_used_gb: 21.5,
  ram_total_gb: 64,
  cpu_percent: 37.4,
  vram: { kind: "dedicated", used_gb: 6, total_gb: 24 },
};

/** An embedded engine in service, and a downloaded CUDA engine the channel has outgrown. */
const ENGINES: EnginesResponse = {
  engines: [
    {
      engine: "llama-server",
      tag: null,
      variant: "cpu",
      path: "/opt/apollia/llama-server",
      source: "embedded",
      version: "7001 (0ff1ce0)",
      in_service: false,
    },
    {
      engine: "llama-server",
      tag: "b7001",
      variant: "cuda",
      path: "/home/u/.apollia/engines/llama-server/b7001/cuda",
      source: "downloaded",
      version: "7001 (0ff1ce0)",
      in_service: true,
    },
  ],
  channel: "stable",
  manifest_sequence: 12,
  updates: [
    {
      engine: "llama-server",
      variant: "cuda",
      installed_tag: "b7001",
      latest_tag: "b7100",
      update_available: true,
    },
  ],
  cuda_proposable: false,
  dictation: null,
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
  systemInfoStore.set(
    loaded({
      version: "0.3.0",
      os: "linux x86_64",
      arch: "x86_64",
      python_path: null,
      data_dir: "/home/u/.apollia",
    }),
  );
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Resource monitor - what the machine is using", () => {
  test("memory, video memory and processor are shown with their figures", () => {
    // GIVEN a measurement on a machine with a dedicated card
    // WHEN the monitor renders it
    const html = renderToString(ResourceMonitor, { props: { usage: USAGE } }).body;

    // THEN the memory in use and the video memory in use are stated against
    //      what is installed, and the processor load as a percentage
    expect(html).toContain(copy.resource_monitor.title);
    expect(tagOf(html, "resource-monitor-ram-value")).not.toBeNull();
    expect(html).toContain(usedOf("21.5", "64.0"));
    expect(tagOf(html, "resource-monitor-vram-value")).not.toBeNull();
    expect(html).toContain(usedOf("6.0", "24.0"));
    expect(tagOf(html, "resource-monitor-cpu-value")).not.toBeNull();
    expect(html).toContain(fill(copy.resource_monitor.percent, { n: "37" }));
    // AND each gauge carries the share it shows
    expect(html.match(/aria-valuenow="[0-9.]+"/g)?.length).toBe(3);
    expect(html).toContain('aria-valuenow="25"');
  });

  test("a chip that shares its memory says so instead of a second figure", () => {
    // GIVEN a measurement on an Apple chip
    const usage: ResourceUsage = { ...USAGE, vram: { kind: "unified" } };

    // WHEN the monitor renders it
    const html = renderToString(ResourceMonitor, { props: { usage } }).body;

    // THEN the video memory is said to be the system memory, with no figure
    expect(html).toContain(copy.resource_monitor.vram_unified);
    expect(tagOf(html, "resource-monitor-vram-value")).toBeNull();
  });

  test("a machine whose video memory nothing measures says so", () => {
    // GIVEN a measurement without a video memory figure
    const usage: ResourceUsage = { ...USAGE, vram: { kind: "unmeasured" } };

    // WHEN the monitor renders it
    const html = renderToString(ResourceMonitor, { props: { usage } }).body;

    // THEN it says the video memory is not measured, and invents no figure
    expect(html).toContain(copy.resource_monitor.vram_unmeasured);
    expect(tagOf(html, "resource-monitor-vram-value")).toBeNull();
  });

  test("before the first measurement it says it is measuring", () => {
    // GIVEN no measurement yet
    // WHEN the monitor renders
    const html = renderToString(ResourceMonitor, { props: { usage: null } }).body;

    // THEN it says so and shows no gauge
    expect(tagOf(html, "resource-monitor-measuring")).not.toBeNull();
    expect(tagOf(html, "resource-monitor-ram")).toBeNull();
  });

  test("a gauge is a bounded share and warns as it fills", () => {
    // GIVEN used and installed amounts, some of them degenerate
    // WHEN the share and its tone are computed
    // THEN the share stays within 0 and 100 and the tone follows it
    expect(sharePercent(6, 24)).toBe(25);
    expect(sharePercent(30, 24)).toBe(100);
    expect(sharePercent(1, 0)).toBe(0);
    expect(gaugeTone(40)).toBe("primary");
    expect(gaugeTone(80)).toBe("warning");
    expect(gaugeTone(95)).toBe("destructive");
  });
});

describe("Resource monitor - it measures only while it is watched", () => {
  test("the reads start with the subscription, repeat at the interval and stop with it", async () => {
    // GIVEN a monitor reading every two seconds, and nobody watching
    vi.useFakeTimers();
    const read = vi.fn(async () => USAGE);
    const monitor = createResourceMonitor(read, 2000);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(read).not.toHaveBeenCalled();

    // WHEN a page subscribes, stays for two intervals, then unmounts
    const seen: Array<ResourceUsage | null> = [];
    const unsubscribe = monitor.subscribe((usage) => seen.push(usage));
    await vi.advanceTimersByTimeAsync(0);
    expect(read).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(4000);
    expect(read).toHaveBeenCalledTimes(3);
    unsubscribe();

    // THEN the measurement reached the page, and nothing is read afterwards
    expect(seen.at(-1)).toEqual(USAGE);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(read).toHaveBeenCalledTimes(3);
    expect(vi.getTimerCount()).toBe(0);
  });

  test("a read still in flight when the page unmounts schedules nothing", async () => {
    // GIVEN a monitor whose read has not answered yet
    vi.useFakeTimers();
    let answer: (usage: ResourceUsage) => void = () => {};
    const read = vi.fn(() => new Promise<ResourceUsage>((resolve) => (answer = resolve)));
    const monitor = createResourceMonitor(read, 2000);
    const seen: Array<ResourceUsage | null> = [];
    const unsubscribe = monitor.subscribe((usage) => seen.push(usage));

    // WHEN the page unmounts, then the read answers
    unsubscribe();
    answer(USAGE);
    await vi.advanceTimersByTimeAsync(60_000);

    // THEN the late answer is dropped and no further read is made
    expect(seen).toEqual([null]);
    expect(read).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  test("a failed read keeps the last figures and tries again", async () => {
    // GIVEN a monitor whose second read fails
    vi.useFakeTimers();
    const read = vi
      .fn<() => Promise<ResourceUsage>>()
      .mockResolvedValueOnce(USAGE)
      .mockRejectedValueOnce(new Error("daemon restarting"))
      .mockResolvedValue({ ...USAGE, cpu_percent: 80 });
    const monitor = createResourceMonitor(read, 2000);

    // WHEN it is watched across three reads
    const unsubscribe = monitor.subscribe(() => {});
    await vi.advanceTimersByTimeAsync(2000);
    const afterFailure = get(monitor);
    await vi.advanceTimersByTimeAsync(2000);
    const afterRecovery = get(monitor);
    unsubscribe();

    // THEN the failure left the figures in place and the next read replaced them
    expect(afterFailure).toEqual(USAGE);
    expect(afterRecovery?.cpu_percent).toBe(80);
  });

  test("the Hardware page reads through the measurement command and leaves no timer once it is gone", async () => {
    // GIVEN the Hardware page
    vi.useFakeTimers();

    // WHEN it renders and is torn down, as a server render does in one pass
    const html = renderToString(System).body;
    expect(tagOf(html, "resource-monitor")).not.toBeNull();
    await vi.advanceTimersByTimeAsync(0);
    const reads = invoked().filter((c) => c === "get_resource_usage").length;
    await vi.advanceTimersByTimeAsync(60_000);

    // THEN it asked the one measurement command once, and nothing after
    expect(reads).toBe(1);
    expect(invoked().filter((c) => c === "get_resource_usage").length).toBe(1);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("Resource monitor - where it lives", () => {
  test("it is in System > Hardware for everyone, and not in System > Engines", () => {
    // GIVEN the developer mode off, then on
    for (const mode of [false, true]) {
      developerMode.set(mode);

      // WHEN each System page renders
      const hardware = renderToString(System).body;
      const engines = renderToString(Engines).body;

      // THEN the monitor sits in Hardware under its name, in both modes, and
      //      Engines carries none
      expect(tagOf(hardware, "advanced-section")).not.toBeNull();
      expect(tagOf(hardware, "resource-monitor")).not.toBeNull();
      expect(hardware).toContain(copy.resource_monitor.title);
      expect(tagOf(engines, "resource-monitor")).toBeNull();
    }
  });
});

describe("Engines - one table from the engine list", () => {
  test("developer mode off: engines, versions, the one in service and the update, no channel and no source", () => {
    // GIVEN an embedded engine, a downloaded CUDA engine in service, and a
    //      newer CUDA build on the channel
    developerMode.set(false);

    // WHEN System > Engines renders
    const html = renderToString(Engines).body;

    // THEN there is one list, one row per engine, each with its variant and
    //      the version it states
    expect(html.match(/data-testid="engines-list"/g)?.length).toBe(1);
    expect(html.match(/data-testid="engine-row"/g)?.length).toBe(2);
    expect(html).toContain(`llama.cpp · ${copy.engines.variant_cpu}`);
    expect(html).toContain("llama.cpp · CUDA");
    expect(
      html.match(/data-testid="engine-row-version"[^>]*>(?:<!--[^>]*-->|\s)*7001 \(0ff1ce0\)/g)
        ?.length,
    ).toBe(2);
    // AND the engine in service is marked, once
    expect(html.match(/data-testid="engine-row-in-service"/g)?.length).toBe(1);
    expect(html).toContain(copy.engines.in_service);
    // AND the update and the check are there for everyone
    expect(tagOf(html, "engine-row-update-available")).not.toBeNull();
    expect(html).toContain(available("b7100"));
    expect(tagOf(html, "engines-check-updates")).not.toBeNull();
    expect(html).toContain(copy.engines.check_updates);
    // AND the channel, the sources and the channel tags are not
    expect(tagOf(html, "engines-channel")).toBeNull();
    expect(tagOf(html, "engines-channel-select")).toBeNull();
    expect(tagOf(html, "engine-row-source")).toBeNull();
    expect(tagOf(html, "engine-row-tag")).toBeNull();
    expect(html).not.toContain(copy.engines.source_downloaded);
  });

  test("developer mode on: the same table gains the source and the channel tag, under the channel", () => {
    // GIVEN the same engines, developer mode on
    developerMode.set(true);

    // WHEN System > Engines renders
    const html = renderToString(Engines).body;

    // THEN it is still one list of the same rows
    expect(html.match(/data-testid="engines-list"/g)?.length).toBe(1);
    expect(html.match(/data-testid="engine-row"/g)?.length).toBe(2);
    // AND each engine says where it comes from
    expect(html.match(/data-testid="engine-row-source"/g)?.length).toBe(2);
    expect(html).toContain(copy.engines.source_embedded);
    expect(html).toContain(copy.engines.source_downloaded);
    // AND the downloaded engine shows the tag the channel published, which
    //     agrees with its banner, so no mismatch is raised
    expect(html).toContain(fill(copy.engines.channel_tag, { tag: "b7001" }));
    expect(tagOf(html, "engine-row-tag-mismatch")).toBeNull();
    // AND the channel sits above the table, with its two names, Stable chosen,
    //     under a label that is not the application's
    expect(html.indexOf('data-testid="engines-channel"')).toBeGreaterThan(-1);
    expect(html.indexOf('data-testid="engines-channel"')).toBeLessThan(
      html.indexOf('data-testid="engines-list"'),
    );
    expect(html).toContain(`>${copy.engines.channel_stable}</option>`);
    expect(html).toContain(`>${copy.engines.channel_preview}</option>`);
    expect(html).toMatch(/<option value="stable" selected[^>]*>/);
    expect(copy.engines.channel_label).not.toBe(copy.general.channel_label);
    expect(html).toContain(copy.engines.channel_label);
    expect(html).not.toContain(copy.general.channel_label);
  });

  test("an imported engine whose banner is not its tag is flagged", () => {
    // GIVEN an imported engine published as b7001 that states 6990
    developerMode.set(true);
    enginesStore.set(
      loaded({
        ...ENGINES,
        engines: [
          { ...ENGINES.engines[1], source: "imported", version: "6990 (deadbee)" },
        ],
        updates: [],
      }),
    );

    // WHEN System > Engines renders
    const html = renderToString(Engines).body;

    // THEN the row shows the banner as its version, says Imported, and names
    //      the tag and the version that disagree
    expect(html).toContain(copy.engines.source_imported);
    expect(tagOf(html, "engine-row-tag-mismatch")).not.toBeNull();
    expect(html).toContain(
      fill(copy.engines.tag_mismatch, { tag: "b7001", version: "6990 (deadbee)" }),
    );
    expect(tagOf(html, "engine-row-tag")).toBeNull();
  });

  test("an engine at the newest tag of the channel says so", () => {
    // GIVEN a channel whose newest CUDA build is the installed one
    enginesStore.set(
      loaded({
        ...ENGINES,
        updates: [{ ...ENGINES.updates[0], latest_tag: "b7001", update_available: false }],
      }),
    );

    // WHEN System > Engines renders
    const html = renderToString(Engines).body;

    // THEN the row says latest version, and no update is offered
    expect(html.match(/data-testid="engine-row-update-latest"/g)?.length).toBe(1);
    expect(tagOf(html, "engine-row-update-available")).toBeNull();
  });

  test("a banner is compared with its tag on the number, prefix and commit aside", () => {
    // GIVEN banners and tags as the engines and the channel write them
    // WHEN they are compared
    // THEN the same build agrees, another build does not
    expect(versionMatchesTag("7001 (0ff1ce0)", "b7001")).toBe(true);
    expect(versionMatchesTag("0.9.2", "v0.9.2")).toBe(true);
    expect(versionMatchesTag("v22.11.0", "v22.11.0")).toBe(true);
    expect(versionMatchesTag("7000 (0ff1ce0)", "b7001")).toBe(false);
    expect(versionMatchesTag("1.7.4", "v1.7.3")).toBe(false);
  });
});

describe("Engines - the channel is the store's, never the interface's", () => {
  test("choosing a channel writes it through the command, checks it, and refreshes the update column", async () => {
    // GIVEN a runtime that, once on preview, verifies a channel offering b7200
    const onPreview: EnginesResponse = {
      ...ENGINES,
      channel: "preview",
      manifest_sequence: 31,
      updates: [{ ...ENGINES.updates[0], latest_tag: "b7200" }],
    };
    ipc.invoke.mockImplementation(async (command: string) => {
      if (command === "set_engine_channel") return { channel: "preview", base_url: "https://m" };
      if (command === "check_engine_updates")
        return { channel: "preview", manifest_sequence: 31, updates: onPreview.updates };
      if (command === "list_engines") return onPreview;
      return null;
    });
    developerMode.set(true);

    // WHEN the preview channel is chosen
    const refused = await changeEngineChannel("preview");

    // THEN the channel went through the store command with its value, the
    //      check followed, and the list was read again, in that order
    expect(refused).toBeNull();
    expect(invoked()).toEqual(["set_engine_channel", "check_engine_updates", "list_engines"]);
    expect(ipc.invoke.mock.calls[0][1]).toEqual({ channel: "preview" });
    // AND nothing was kept on the interface side
    expect(storage.size).toBe(0);
    // AND the page shows the channel read back and the refreshed update
    const html = renderToString(Engines).body;
    expect(html).toMatch(/<option value="preview" selected[^>]*>/);
    expect(html).not.toMatch(/<option value="stable" selected/);
    expect(html).toContain(available("b7200"));
    expect(html).not.toContain(available("b7100"));
  });

  test("a channel the store refuses checks nothing and says why", async () => {
    // GIVEN a runtime that refuses the write
    ipc.invoke.mockImplementation(async (command: string) => {
      if (command === "set_engine_channel") throw new Error("settings file unreadable");
      return null;
    });

    // WHEN a channel is chosen
    const refused = await changeEngineChannel("preview");

    // THEN the refusal is returned and neither the check nor the list ran
    expect(refused).toBe("settings file unreadable");
    expect(invoked()).toEqual(["set_engine_channel"]);
    expect(get(enginesStore).data?.channel).toBe("stable");
  });

  test("a check the channel refuses still reads the list again and says why", async () => {
    // GIVEN a build without a distribution key
    ipc.invoke.mockImplementation(async (command: string) => {
      if (command === "check_engine_updates")
        throw new Error("this build carries no distribution key");
      if (command === "list_engines") return ENGINES;
      return null;
    });

    // WHEN the updates are checked
    const refused = await verifyEngineUpdates();

    // THEN the sentence of the runtime is returned and the list was re-read
    expect(refused).toBe("this build carries no distribution key");
    expect(invoked()).toEqual(["check_engine_updates", "list_engines"]);
  });
});
