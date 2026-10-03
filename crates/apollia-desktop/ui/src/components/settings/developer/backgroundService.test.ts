import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  BACKGROUND_SERVICE_KEYS,
  SWITCH_OFF,
  backgroundServiceSwitch,
  backgroundServiceVisible,
  readBackgroundService,
  type BackgroundServiceIpc,
} from "./backgroundService";
import type { BackgroundService } from "$lib/ipc/backgroundService";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

/** A machine where the service can be installed and is not. */
const FREE: BackgroundService = {
  supported: true,
  bundled: true,
  installed: false,
  running: false,
  allowed: true,
  allowed_source: "default",
};

function lookup(catalogue: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalogue,
    );
}

describe("Settings > Developer: keep Apollia running without the application", () => {
  test("the block shows with the developer mode on and not with it off", () => {
    // GIVEN the two positions of the developer mode
    // WHEN the block's visibility is asked for each
    const on = backgroundServiceVisible(true);
    const off = backgroundServiceVisible(false);
    // THEN it shows only in developer mode
    expect(on).toBe(true);
    expect(off).toBe(false);
  });

  test("the switch is off by default, and off when the shell cannot answer", async () => {
    // GIVEN a machine where nothing was installed, and a shell that fails
    const failing: BackgroundServiceIpc = {
      read: vi.fn(async () => {
        throw new Error("shell unreachable");
      }),
      write: vi.fn(async () => {}),
    };
    // WHEN the switch is read for each
    const fresh = backgroundServiceSwitch(FREE);
    const unreachable = await readBackgroundService(failing);
    // THEN it is off and free on the first, off on the second
    expect(fresh).toEqual(SWITCH_OFF);
    expect(unreachable).toEqual(SWITCH_OFF);
  });

  test("the switch follows the service the OS reports", async () => {
    // GIVEN a shell that reports the service installed and running
    const ipc: BackgroundServiceIpc = {
      read: vi.fn(async () => ({ ...FREE, installed: true, running: true })),
      write: vi.fn(async () => {}),
    };
    // WHEN the switch is read
    const read = await readBackgroundService(ipc);
    // THEN it is on, running, and can be turned off
    expect(read).toMatchObject({ on: true, running: true, disabled: false });
  });

  test("a policy that forbids the service locks the switch off but lets it be turned off", () => {
    // GIVEN the authorization imposed off by the policy, with and without a
    // service already installed
    const forbidden = { ...FREE, allowed: false, allowed_source: "policy" as const };
    // WHEN the switch is computed for each
    const off = backgroundServiceSwitch(forbidden);
    const on = backgroundServiceSwitch({ ...forbidden, installed: true });
    // THEN off is locked and disabled, on stays movable to off and names the policy
    expect(off).toMatchObject({ on: false, disabled: true, locked: true });
    expect(on).toMatchObject({ on: true, disabled: false, locked: true });
  });

  test("no service manager or no bundled command line disables the switch and says why", () => {
    // GIVEN a system without a service manager, and an application without
    // its command line
    // WHEN the switch is computed for each
    const unsupported = backgroundServiceSwitch({ ...FREE, supported: false });
    const noCli = backgroundServiceSwitch({ ...FREE, bundled: false });
    // THEN both are disabled with their reason
    expect(unsupported).toMatchObject({
      disabled: true,
      reasonKey: "settings.developer.service.unsupported",
    });
    expect(noCli).toMatchObject({ disabled: true, reasonKey: "settings.developer.service.no_cli" });
  });

  test("the block, its switch, its explanation and its help read in French and English", () => {
    // GIVEN the two catalogues
    // WHEN the block's keys are looked up in each
    const inFr = (key: string) => lookup(fr, key);
    const inEn = (key: string) => lookup(en, key);
    // THEN the block, the switch and its explanation carry the wording of the
    // specification, the "?" has a text, and each French entry is its own
    // translation rather than the English one
    expect(inEn(BACKGROUND_SERVICE_KEYS.title)).toBe("Service without interface");
    expect(inEn(BACKGROUND_SERVICE_KEYS.label)).toBe(
      "Keep Apollia running without the application",
    );
    expect(inEn(BACKGROUND_SERVICE_KEYS.hint)).toBe(
      "Keep the agents, the batches and the automations running without leaving the application open.",
    );
    for (const key of Object.values(BACKGROUND_SERVICE_KEYS)) {
      const french = inFr(key);
      expect(typeof french).toBe("string");
      expect(String(french).length).toBeGreaterThan(0);
      expect(french).not.toBe(inEn(key));
    }
    expect(String(inFr(BACKGROUND_SERVICE_KEYS.label))).toContain("Apollia");
  });
});
