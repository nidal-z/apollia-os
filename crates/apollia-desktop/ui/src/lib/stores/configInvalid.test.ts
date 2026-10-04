/**
 * The banner of an `apollia.toml` refused at startup: set by the read once the
 * runtime answers or by the startup event, cleared once a valid file applies,
 * and its action opening the editor at the faulty line, turning the developer
 * mode on first when it is off.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  checkConfigAtStartup,
  configEditorFocusLine,
  configInvalidAtStart,
  consumeFocusLine,
  firstFaultLine,
  onConfigEvent,
  openEditorAtFault,
  resetConfigInvalidForTests,
} from "$lib/stores/configInvalid";
import type { ConfigFileView } from "$lib/ipc/configFile";

const SRC = resolve(__dirname, "../..");

function refused(): ConfigFileView {
  return {
    path: "/home/u/.apollia/apollia.toml",
    exists: true,
    text: '[api]\nport = "x"\n',
    sha256: "sha",
    diagnostics: [
      { line: 2, column: 8, path: "api.port", message: "invalid type: string, expected u16" },
    ],
    locked: [],
    pendingRestart: [],
  };
}

beforeEach(() => resetConfigInvalidForTests());

describe("the banner at startup", () => {
  it("is set when the file read at launch carries faults", async () => {
    // GIVEN a runtime whose file was refused
    // WHEN the startup check reads it
    await checkConfigAtStartup(async () => refused());

    // THEN the banner holds the faults, first at line 2
    const state = get(configInvalidAtStart);
    expect(state?.diagnostics[0]?.line).toBe(2);
    expect(firstFaultLine(state!)).toBe(2);
  });

  it("stays absent for a valid file", async () => {
    // GIVEN a runtime whose file is valid
    // WHEN the startup check reads it
    await checkConfigAtStartup(async () => ({ ...refused(), diagnostics: [] }));

    // THEN no banner
    expect(get(configInvalidAtStart)).toBeNull();
  });

  it("waits for a runtime still starting", async () => {
    // GIVEN a runtime that answers on the third try
    let tries = 0;
    const read = vi.fn(async () => {
      tries++;
      if (tries < 3) throw new Error("failed to connect to runtime API");
      return refused();
    });

    // WHEN the startup check runs
    await checkConfigAtStartup(read, async () => {});

    // THEN it read three times and set the banner
    expect(read).toHaveBeenCalledTimes(3);
    expect(get(configInvalidAtStart)).not.toBeNull();
  });

  it("is set by the startup event, and cleared once a valid file applies", () => {
    // GIVEN no read yet
    // WHEN the runtime publishes ConfigInvalid, then ConfigReloaded
    onConfigEvent("ConfigInvalid", {
      ConfigInvalid: { path: "/p", diagnostics: refused().diagnostics },
    });
    const shown = get(configInvalidAtStart);
    onConfigEvent("ConfigReloaded", { ConfigReloaded: { sections: ["api"], pending_restart: [] } });

    // THEN the banner appeared, then left
    expect(shown?.path).toBe("/p");
    expect(get(configInvalidAtStart)).toBeNull();
  });
});

describe("opening the editor from the banner", () => {
  it("turns the developer mode on first, then opens Configuration at the faulty line", async () => {
    // GIVEN the banner, the developer mode off
    await checkConfigAtStartup(async () => refused());
    const calls: string[] = [];

    // WHEN the user confirmed the dialog
    const opened = await openEditorAtFault(get(configInvalidAtStart)!, {
      developerMode: () => false,
      turnDeveloperModeOn: async () => {
        calls.push("mode on");
        return true;
      },
      openSettings: (section) => calls.push(`open ${section}`),
    });

    // THEN the mode came on, then the editor opened on line 2
    expect(opened).toBe(true);
    expect(calls).toEqual(["mode on", "open configuration"]);
    expect(consumeFocusLine()).toBe(2);
    expect(get(configEditorFocusLine)).toBeNull();
  });

  it("opens nothing when the mode cannot be turned on", async () => {
    // GIVEN a developer mode the policy keeps off
    await checkConfigAtStartup(async () => refused());
    const openSettings = vi.fn();

    // WHEN the banner's action runs
    const opened = await openEditorAtFault(get(configInvalidAtStart)!, {
      developerMode: () => false,
      turnDeveloperModeOn: async () => false,
      openSettings,
    });

    // THEN nothing opened
    expect(opened).toBe(false);
    expect(openSettings).not.toHaveBeenCalled();
  });

  it("is mounted on every screen, outside any developer-mode branch", () => {
    // GIVEN the shell of the main column
    const main = readFileSync(resolve(SRC, "lib/components/app/Main.svelte"), "utf8");

    // WHEN the banner's mount is located
    const mount = main.indexOf("<ConfigInvalidBanner />");

    // THEN it is mounted, and no developer-mode condition opens before it
    expect(mount).toBeGreaterThan(0);
    expect(main.slice(main.indexOf("<main"), mount)).not.toMatch(/\{#if[^}]*developerMode/);
  });

  it("asks before turning the mode on, with Cancel and Turn on and open", () => {
    // GIVEN the banner component
    const banner = readFileSync(
      resolve(SRC, "components/settings/config/ConfigInvalidBanner.svelte"),
      "utf8",
    );

    // WHEN its gestures are read
    // THEN a click with the mode off opens the confirmation, whose buttons exist
    expect(banner).toMatch(/if \(\$developerMode\) void open\(\);\s*else confirming = true;/);
    for (const testid of [
      "config-invalid-banner-open",
      "config-invalid-confirm-cancel",
      "config-invalid-confirm-open",
    ]) {
      expect(banner).toContain(`data-testid="${testid}"`);
    }
  });
});
