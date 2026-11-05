import { describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";
import type { EngineInstallation, EngineRemoval } from "$lib/ipc/engineInstallation";
import type { VariantRow } from "$lib/settings/engineVariants";
import {
  createEngineManagement,
  downloadPercent,
  installationOn,
  type EngineManagementDeps,
} from "$lib/settings/engineManagement";

function row(part: Partial<VariantRow> = {}): VariantRow {
  return {
    variant: "vulkan",
    tag: "b7100",
    size: 1,
    installed: false,
    verdict: "compatible",
    reason: null,
    experimental: false,
    ...part,
  };
}

/** A simulated IPC: the installation answers `states` in turn, then the last one. */
function deps(part: Partial<EngineManagementDeps> = {}, states: EngineInstallation[] = []) {
  let read = 0;
  const base: EngineManagementDeps = {
    installation: vi.fn(async () => states[Math.min(read++, states.length - 1)] ?? { state: "idle" }),
    start: vi.fn(async () => ({ state: "verifying", kind: "install" }) as EngineInstallation),
    importFolder: vi.fn(async () => ({ state: "verifying", kind: "import" }) as EngineInstallation),
    pickFolder: vi.fn(async () => "/Volumes/USB/engines"),
    remove: vi.fn(async () => ({ outcome: "removed" }) as EngineRemoval),
    reloadEngines: vi.fn(async () => undefined),
    wait: vi.fn(async () => undefined),
    onError: vi.fn(),
  };
  return { ...base, ...part };
}

const TARGET = { engine: "llama-server", tag: "b7100", variant: "cuda-12" };

describe("Install from the Engines page", () => {
  test("a variant not installed starts at once and its progress is on its own line", async () => {
    // GIVEN a page whose installation verifies, downloads, then installs
    const d = deps({}, [
      { state: "downloading", kind: "install", received: 40, total: 100 },
      { state: "installed", kind: "install" },
    ]);
    const page = createEngineManagement(d);
    const seen: (number | null)[] = [];
    page.subscribe((s) => {
      const on = installationOn(s, { kind: "variant", variant: "vulkan" });
      if (on) seen.push(downloadPercent(on));
    });

    // WHEN Install is pressed on the Vulkan row
    await page.install(row());

    // THEN the store was asked for that variant, the download showed on the
    // Vulkan line only, and the list was read again once installed
    expect(d.start).toHaveBeenCalledWith("llama-server", "vulkan");
    expect(seen).toContain(40);
    expect(installationOn(get(page), { kind: "variant", variant: "cpu" })).toBeNull();
    expect(get(page).installation.state).toBe("installed");
    expect(d.reloadEngines).toHaveBeenCalledTimes(1);
  });

  test("an incompatible variant asks first, and nothing starts until it is confirmed", async () => {
    // GIVEN the ROCm row, rated incompatible for want of a card
    const d = deps();
    const page = createEngineManagement(d);
    const rocm = row({ variant: "rocm", verdict: "incompatible", reason: { code: "no_gpu" } });

    // WHEN Install is pressed, then the confirmation is cancelled
    await page.install(rocm);
    const asked = get(page).pendingInstall;
    page.cancelInstall();

    // THEN the confirmation held the row and its reason, and nothing was sent
    expect(asked?.reason).toEqual({ code: "no_gpu" });
    expect(get(page).pendingInstall).toBeNull();
    expect(d.start).not.toHaveBeenCalled();

    // WHEN it is pressed again and confirmed
    await page.install(rocm);
    await page.confirmInstall();

    // THEN the variant is installed anyway
    expect(d.start).toHaveBeenCalledWith("llama-server", "rocm");
    expect(get(page).pendingInstall).toBeNull();
  });

  test("a refusal of the store stays on the line, and no second installation starts during one", async () => {
    // GIVEN a page whose installation is downloading
    const d = deps(
      {
        start: vi.fn(async () => ({ state: "failed", kind: "install", code: "network", error: "unreachable", archive_requested: false }) as EngineInstallation),
      },
    );
    const page = createEngineManagement(d);

    // WHEN the store refuses the CPU variant
    await page.install(row({ variant: "cpu" }));

    // THEN the refusal is on the CPU line with its code, and nothing was read again
    const on = installationOn(get(page), { kind: "variant", variant: "cpu" });
    expect(on).toMatchObject({ state: "failed", code: "network" });
    expect(d.reloadEngines).not.toHaveBeenCalled();
  });

  test("an installed variant has nothing to install", async () => {
    // GIVEN the row of an installed variant
    const d = deps();
    const page = createEngineManagement(d);

    // WHEN its install is asked for
    await page.install(row({ installed: true }));

    // THEN nothing is sent
    expect(d.start).not.toHaveBeenCalled();
  });
});

describe("Import a folder from the Engines page", () => {
  test("the chosen folder is imported and followed on the import line", async () => {
    // GIVEN a picker that answers a folder and an import that ends installed
    const d = deps({}, [{ state: "installed", kind: "import" }]);
    const page = createEngineManagement(d);

    // WHEN Import a folder is pressed
    await page.importFromFolder();

    // THEN that folder was imported, the line of the import shows its end,
    // no variant line does, and the list was read again
    expect(d.importFolder).toHaveBeenCalledWith("/Volumes/USB/engines");
    expect(installationOn(get(page), { kind: "import" })).toEqual({ state: "installed", kind: "import" });
    expect(installationOn(get(page), { kind: "variant", variant: "vulkan" })).toBeNull();
    expect(d.reloadEngines).toHaveBeenCalledTimes(1);
  });

  test("a closed picker imports nothing", async () => {
    // GIVEN a picker the person closes
    const d = deps({ pickFolder: vi.fn(async () => null) });
    const page = createEngineManagement(d);

    // WHEN Import a folder is pressed
    await page.importFromFolder();

    // THEN nothing is sent and no line shows anything
    expect(d.importFolder).not.toHaveBeenCalled();
    expect(installationOn(get(page), { kind: "import" })).toBeNull();
  });
});

describe("Remove from the Engines page", () => {
  test("a build is removed after one confirmation and the list is read again", async () => {
    // GIVEN the confirmation of an installed build that nothing names
    const d = deps();
    const page = createEngineManagement(d);
    page.askRemove(TARGET);

    // WHEN it is confirmed
    const removed = await page.confirmRemove();

    // THEN it was removed without the confirmation a chosen build needs
    expect(d.remove).toHaveBeenCalledWith("llama-server", "b7100", "cuda-12", false);
    expect(removed).toBe(true);
    expect(get(page).removal).toBeNull();
    expect(d.reloadEngines).toHaveBeenCalledTimes(1);
  });

  test("the chosen build says what the choice becomes, then goes on a second confirmation", async () => {
    // GIVEN a build the GGUF choice names, refused unconfirmed with `after`
    const remove = vi
      .fn<EngineManagementDeps["remove"]>()
      .mockResolvedValueOnce({ outcome: "refused", status: 409, code: "chosen", error: "chosen", after: "auto" })
      .mockResolvedValueOnce({ outcome: "removed" });
    const page = createEngineManagement(deps({ remove }));
    page.askRemove(TARGET);

    // WHEN it is confirmed once
    const first = await page.confirmRemove();

    // THEN it is kept, and the dialog says the choice goes back to automatic
    expect(first).toBe(false);
    expect(get(page).removal?.step).toEqual({ phase: "chosen", after: "auto" });

    // WHEN it is confirmed again
    const second = await page.confirmRemove();

    // THEN the removal is sent with the confirmation and done
    expect(remove.mock.calls[1]).toEqual(["llama-server", "b7100", "cuda-12", true]);
    expect(second).toBe(true);
  });

  test("an older tag of the chosen variant is named as the one taking the place", async () => {
    // GIVEN a chosen build with an older build of the same variant installed
    const remove = vi.fn<EngineManagementDeps["remove"]>(async () => ({
      outcome: "refused",
      status: 409,
      code: "chosen",
      error: "the build b6000 takes its place",
      after: "b6000",
    }));
    const page = createEngineManagement(deps({ remove }));
    page.askRemove(TARGET);

    // WHEN it is confirmed once
    await page.confirmRemove();

    // THEN the dialog names that tag
    expect(get(page).removal?.step).toEqual({ phase: "chosen", after: "b6000" });
  });

  test.each([
    ["in_service", 409],
    ["embedded", 409],
    ["locked", 403],
  ] as const)("a build refused as %s is said, and the dialog sends nothing more", async (code, status) => {
    // GIVEN a removal the store refuses with `code`
    const remove = vi.fn<EngineManagementDeps["remove"]>(async () => ({
      outcome: "refused",
      status,
      code,
      error: "kept",
      after: null,
    }));
    const page = createEngineManagement(deps({ remove }));
    page.askRemove(TARGET);

    // WHEN it is confirmed, then confirmed again
    await page.confirmRemove();
    const again = await page.confirmRemove();

    // THEN the dialog holds the refusal with its code and nothing is sent twice
    expect(get(page).removal?.step).toEqual({ phase: "refused", code, detail: "kept" });
    expect(again).toBe(false);
    expect(remove).toHaveBeenCalledTimes(1);
  });

  test("a build in service stays refused even after the chosen step confirmed it", async () => {
    // GIVEN a chosen build that a process starts using in the meantime
    const remove = vi
      .fn<EngineManagementDeps["remove"]>()
      .mockResolvedValueOnce({ outcome: "refused", status: 409, code: "chosen", error: "chosen", after: "auto" })
      .mockResolvedValueOnce({ outcome: "refused", status: 409, code: "in_service", error: "running", after: null });
    const page = createEngineManagement(deps({ remove }));
    page.askRemove(TARGET);

    // WHEN it is confirmed twice
    await page.confirmRemove();
    await page.confirmRemove();

    // THEN the second answer is said as in service
    expect(get(page).removal?.step).toEqual({ phase: "refused", code: "in_service", detail: "running" });
  });

  test("cancelling sends nothing", () => {
    // GIVEN the confirmation open
    const d = deps();
    const page = createEngineManagement(d);
    page.askRemove(TARGET);

    // WHEN it is cancelled
    page.cancelRemove();

    // THEN nothing was removed and the dialog is closed
    expect(d.remove).not.toHaveBeenCalled();
    expect(get(page).removal).toBeNull();
  });
});
