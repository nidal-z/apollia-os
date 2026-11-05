import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
const pick = vi.hoisted(() => vi.fn(async (): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: pick }));

import "$lib/i18n";
import en from "$lib/i18n/en.json";
import type { EnginesResponse } from "$lib/ipc/system";
import { engineManagement } from "$lib/settings/engineManagementLive";
import EngineList from "./EngineList.svelte";

const copy = en.settings.engines;

// The dialogs run a Svelte transition, which jsdom cannot play: an animation
// that finishes at once stands in for it.
Element.prototype.animate = function (): Animation {
  const animation = {
    onfinish: null as (() => void) | null,
    cancel: () => {},
    finished: Promise.resolve(),
  };
  setTimeout(() => animation.onfinish?.(), 0);
  return animation as unknown as Animation;
};

/** Two builds of llama.cpp installed: the embedded one in service, a downloaded CUDA 12 one. */
function engines(cudaInService = false): EnginesResponse {
  return {
    engines: [
      { engine: "llama-server", variant: "cpu", source: "embedded", tag: null, path: "/app/llama-server", version: "7100 (c0ffee0)", in_service: !cudaInService },
      { engine: "llama-server", variant: "cuda-12", source: "downloaded", tag: "b7100", path: "/store/llama-server", version: "7100 (c0ffee0)", in_service: cudaInService },
    ],
    channel: "stable",
    manifest_sequence: 7,
    updates: [],
    cuda_proposable: false,
    nvidia_card: "NVIDIA GeForce RTX 4090",
    dictation: null,
    hardware: {
      cards: [{ vendor: "nvidia", name: "NVIDIA GeForce RTX 4090", vram_gb: 24, driver_version: "570.86", compute_capability: "8.9", gfx_target: null, kfd_accessible: null, timed_out: false }],
      vulkan_loader: true,
      rocm_runtime: false,
    },
    available: [
      { engine: "llama-server", tag: "b7100", variant: "cuda-12", size: 1, released: null, installed: true },
      { engine: "llama-server", tag: "b7100", variant: "vulkan", size: 1, released: null, installed: false },
      { engine: "llama-server", tag: "b7100", variant: "rocm", size: 1, released: null, installed: false },
    ],
    ratings: [
      { variant: "cuda-12", verdict: "recommended", reason: null },
      { variant: "vulkan", verdict: "compatible", reason: null },
      { variant: "rocm", verdict: "incompatible", reason: { code: "no_gpu" } },
    ],
    verifiable: true,
  };
}

let target: HTMLElement;
let list: ReturnType<typeof mount> | null = null;

async function settle(): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await Promise.resolve();
    flushSync();
  }
  // A closing dialog leaves once its transition ends, on the next task.
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function byTestid(id: string): HTMLElement | null {
  return document.querySelector(`[data-testid="${id}"]`);
}

async function click(id: string): Promise<void> {
  const element = byTestid(id);
  if (!element) throw new Error(`no element ${id}`);
  element.click();
  await settle();
}

function calls(command: string): unknown[] {
  return invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args);
}

function mountList(response: EnginesResponse): void {
  list = mount(EngineList, {
    target,
    props: {
      response,
      loading: false,
      error: null,
      developer: false,
      checking: false,
      checkError: null,
      onchannel: () => {},
      oncheck: () => {},
    },
  });
  flushSync();
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  invoke.mockReset();
  invoke.mockImplementation(async () => null);
  pick.mockReset();
  pick.mockImplementation(async () => null);
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  engineManagement.cancelInstall();
  engineManagement.cancelRemove();
  if (list) unmount(list);
  list = null;
  target.remove();
});

describe("Install a variant from its row", () => {
  test("Install starts the verified download of that variant and its progress shows on the row", async () => {
    // GIVEN the variant list, Vulkan not installed, and a store that starts downloading
    invoke.mockImplementation(async (command: unknown) => {
      if (command === "start_engine_installation") {
        return { state: "downloading", kind: "install", received: 40, total: 100 };
      }
      if (command === "get_engine_installation") return { state: "failed", kind: "install", code: "network", error: "offline", archive_requested: true };
      return null;
    });
    mountList(engines());

    // WHEN Install is pressed on the Vulkan row
    await click("engine-variant-install-vulkan");

    // THEN the download of Vulkan was asked for, and the row shows its progress
    expect(calls("start_engine_installation")).toEqual([{ engine: "llama-server", variant: "vulkan" }]);
    expect(byTestid("engine-variant-install-progress-vulkan")?.textContent).toContain(
      copy.install_progress.downloading.replace("{percent}", "40"),
    );
    // AND an installed variant has no Install button
    expect(byTestid("engine-variant-install-cuda-12")).toBeNull();

    // WHEN the store then stops the download
    await new Promise((resolve) => setTimeout(resolve, 600));
    await settle();

    // THEN the row says why, in words
    expect(byTestid("engine-variant-install-progress-vulkan-error")?.textContent).toContain(
      en.ai.engine.refusal.body.network,
    );
  });

  test("an incompatible variant asks first with its reason, and cancelling installs nothing", async () => {
    // GIVEN the list with the incompatible variants shown
    mountList(engines());
    await click("engine-variant-toggle-incompatible");

    // WHEN Install is pressed on ROCm
    await click("engine-variant-install-rocm");

    // THEN a confirmation says why it does not suit this computer, and nothing started
    expect(byTestid("engine-variant-install-dialog")?.textContent).toContain(copy.reason.no_gpu);
    expect(calls("start_engine_installation")).toEqual([]);

    // WHEN it is cancelled
    await click("engine-variant-install-dialog-cancel");

    // THEN it closes (its outro is left to the browser) and still nothing started
    expect(get(engineManagement).pendingInstall).toBeNull();
    expect(calls("start_engine_installation")).toEqual([]);
  });
});

describe("Import a folder", () => {
  test("Import a folder opens the folder picker and imports the chosen folder", async () => {
    // GIVEN a picker that answers a folder and a store that refuses it unsigned
    pick.mockImplementation(async () => "/Volumes/USB/engines");
    invoke.mockImplementation(async (command: unknown) =>
      command === "import_engine_folder"
        ? { state: "failed", kind: "import", code: "signature_missing", error: "no signature", archive_requested: false }
        : null,
    );
    mountList(engines());

    // WHEN Import a folder is pressed
    await click("engine-import-folder");

    // THEN that folder was imported and the refusal shows on the import line
    expect(calls("import_engine_folder")).toEqual([{ path: "/Volumes/USB/engines" }]);
    expect(byTestid("engine-import-folder-progress-error")?.textContent).toContain(
      en.ai.engine.refusal.body.signature_missing,
    );
  });
});

describe("Remove an installed build", () => {
  test("the build in service cannot be removed and the embedded one has no button", () => {
    // GIVEN the downloaded CUDA build in service
    mountList(engines(true));

    // WHEN the list renders
    const remove = byTestid("engine-variant-remove-llama-server-b7100-cuda-12") as HTMLButtonElement | null;

    // THEN its Remove is disabled, and only that build offers one
    expect(remove?.disabled).toBe(true);
    expect(document.querySelectorAll('[data-testid^="engine-variant-remove-llama-server"]').length).toBe(1);
  });

  test("the chosen build says the choice goes back to automatic before it goes", async () => {
    // GIVEN a store that keeps the chosen build until the removal is confirmed
    invoke.mockImplementation(async (command: unknown, args: unknown) => {
      if (command !== "remove_engine_variant") return null;
      return (args as { confirm: boolean }).confirm
        ? { outcome: "removed" }
        : { outcome: "refused", status: 409, code: "chosen", error: "chosen", after: "auto" };
    });
    mountList(engines());

    // WHEN Remove is pressed, then confirmed once
    await click("engine-variant-remove-llama-server-b7100-cuda-12");
    expect(calls("remove_engine_variant")).toEqual([]);
    await click("engine-variant-remove-dialog-confirm");

    // THEN the dialog says what the choice becomes
    expect(byTestid("engine-variant-remove-dialog-message")?.textContent).toContain(copy.remove.chosen_auto);

    // WHEN it is confirmed again
    await click("engine-variant-remove-dialog-confirm");

    // THEN the removal went with the confirmation, the dialog closed and the list is read again
    expect(calls("remove_engine_variant")).toEqual([
      { engine: "llama-server", tag: "b7100", variant: "cuda-12", confirm: false },
      { engine: "llama-server", tag: "b7100", variant: "cuda-12", confirm: true },
    ]);
    expect(get(engineManagement).removal).toBeNull();
    expect(calls("list_engines").length).toBe(1);
  });

  test("a build the store refuses as in service is said in clear, with no way to insist", async () => {
    // GIVEN a store that answers 409 in_service
    invoke.mockImplementation(async (command: unknown) =>
      command === "remove_engine_variant"
        ? { outcome: "refused", status: 409, code: "in_service", error: "in service", after: null }
        : null,
    );
    mountList(engines());

    // WHEN the removal is confirmed
    await click("engine-variant-remove-llama-server-b7100-cuda-12");
    await click("engine-variant-remove-dialog-confirm");

    // THEN the dialog says Apollia is using it, and offers only to close
    expect(byTestid("engine-variant-remove-dialog-error")?.textContent).toContain(copy.remove.refused.in_service);
    expect(byTestid("engine-variant-remove-dialog-confirm")).toBeNull();
    await click("engine-variant-remove-dialog-cancel");
    expect(get(engineManagement).removal).toBeNull();
  });
});
