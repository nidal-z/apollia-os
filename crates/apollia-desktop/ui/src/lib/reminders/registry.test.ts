import { vi, describe, test, expect, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

class MemoryStorage {
  private readonly s = new Map<string, string>();
  getItem(k: string) {
    return this.s.has(k) ? (this.s.get(k) as string) : null;
  }
  setItem(k: string, v: string) {
    this.s.set(k, v);
  }
  removeItem(k: string) {
    this.s.delete(k);
  }
  clear() {
    this.s.clear();
  }
  key(i: number) {
    return Array.from(this.s.keys())[i] ?? null;
  }
  get length() {
    return this.s.size;
  }
}
(globalThis as unknown as { localStorage: Storage }).localStorage =
  new MemoryStorage() as unknown as Storage;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import { nextSteps, GLOBAL_SCOPE } from "$lib/stores/nextSteps";
import { onboardingModalOpen } from "$lib/stores/onboarding";
import {
  REMINDERS,
  ONBOARDING_REMINDER_ID,
  readReminderStates,
  resetAllReminders,
  resetReminder,
} from "./registry";
import { reminderStorageKey } from "./storage";

const mockedInvoke = vi.mocked(invoke);
const HERE = dirname(fileURLToPath(import.meta.url));
const UI_SRC = join(HERE, "..", "..");

/** The runtime side of the onboarding: started until `reset_onboarding` runs. */
let onboardingStarted = true;

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>((node, part) => {
    if (node && typeof node === "object") return (node as Record<string, unknown>)[part];
    return undefined;
  }, catalogue);
}

beforeEach(() => {
  localStorage.clear();
  nextSteps.reset();
  nextSteps.clearDismissed();
  onboardingModalOpen.set(false);
  onboardingStarted = true;
  vi.clearAllMocks();
  mockedInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_onboarding_state") {
      return { started_at: onboardingStarted ? "2026-10-01T08:00:00Z" : null };
    }
    if (cmd === "reset_onboarding") {
      onboardingStarted = false;
      return undefined;
    }
    if (cmd === "meta_generate_next_steps") {
      return {
        steps: [
          {
            id: "capture-note",
            title: "Save a note",
            description: "Keep a trace.",
            actionButton: { label: "Open", action: "navigate", payload: {} },
          },
        ],
        fromLlm: true,
      };
    }
    return undefined;
  });
});

async function dismissOneNextStep(): Promise<void> {
  nextSteps.load(GLOBAL_SCOPE, "global_context", {});
  await vi.waitFor(() => {
    expect(get(nextSteps.visible(GLOBAL_SCOPE)).steps).toHaveLength(1);
  });
  nextSteps.dismiss("capture-note");
}

describe("the reminders registry", () => {
  test("lists the onboarding then the next-step suggestions, each with a label in both catalogues", () => {
    // GIVEN the registry
    // WHEN its entries are read
    const ids = REMINDERS.map((r) => r.id);

    // THEN the order is the one of Settings > Appearance, and every label exists in FR and EN
    expect(ids).toEqual([ONBOARDING_REMINDER_ID, "next_steps"]);
    expect(new Set(ids).size).toBe(ids.length);
    for (const reminder of REMINDERS) {
      expect(typeof lookup(fr, reminder.labelKey)).toBe("string");
      expect(typeof lookup(en, reminder.labelKey)).toBe("string");
    }
  });

  test("the next-step suggestions keep their dismissals under the registry key, not a key of their own", async () => {
    // GIVEN no dismissed card
    expect(localStorage.getItem(reminderStorageKey("next_steps"))).toBeNull();

    // WHEN a card is dismissed
    await dismissOneNextStep();

    // THEN the registry key holds it, the former key is unused, and the store source no longer names one
    expect(localStorage.getItem(reminderStorageKey("next_steps"))).not.toBeNull();
    expect(localStorage.getItem("apollia.next_steps.dismissed")).toBeNull();
    const source = readFileSync(join(UI_SRC, "lib/stores/nextSteps.ts"), "utf8");
    expect(source).not.toContain("apollia.next_steps.dismissed");
  });

  test("reads the state of each reminder", async () => {
    // GIVEN a started onboarding and one dismissed card
    await dismissOneNextStep();

    // WHEN the states are read
    const states = await readReminderStates();

    // THEN both read as dismissed
    expect(states).toEqual({ [ONBOARDING_REMINDER_ID]: true, next_steps: true });
  });
});

describe("resetting reminders", () => {
  test("resetting the next-step suggestions clears only theirs", async () => {
    // GIVEN a started onboarding and one dismissed card
    await dismissOneNextStep();
    expect(get(nextSteps.visible(GLOBAL_SCOPE)).steps).toHaveLength(0);

    // WHEN only the next-step suggestions are reset
    await resetReminder("next_steps");

    // THEN the card is back, and the onboarding is untouched
    expect(get(nextSteps.visible(GLOBAL_SCOPE)).steps).toHaveLength(1);
    expect(mockedInvoke).not.toHaveBeenCalledWith("reset_onboarding");
    expect(get(onboardingModalOpen)).toBe(false);
    expect(await readReminderStates()).toEqual({
      [ONBOARDING_REMINDER_ID]: true,
      next_steps: false,
    });
  });

  test("resetting the onboarding clears only its own and opens the conversation again", async () => {
    // GIVEN a started onboarding and one dismissed card
    await dismissOneNextStep();

    // WHEN only the onboarding is reset
    await resetReminder(ONBOARDING_REMINDER_ID);

    // THEN the onboarding restarts and the dismissed card stays dismissed
    expect(mockedInvoke).toHaveBeenCalledWith("reset_onboarding");
    expect(get(onboardingModalOpen)).toBe(true);
    expect(get(nextSteps.visible(GLOBAL_SCOPE)).steps).toHaveLength(0);
    expect(await readReminderStates()).toEqual({
      [ONBOARDING_REMINDER_ID]: false,
      next_steps: true,
    });
  });

  test("reset all clears every reminder", async () => {
    // GIVEN a started onboarding and one dismissed card
    await dismissOneNextStep();

    // WHEN every reminder is reset
    await resetAllReminders();

    // THEN nothing reads as dismissed and the onboarding opens again
    expect(await readReminderStates()).toEqual({
      [ONBOARDING_REMINDER_ID]: false,
      next_steps: false,
    });
    expect(get(onboardingModalOpen)).toBe(true);
  });

  test("an unknown id is refused", async () => {
    // GIVEN the registry
    // WHEN an id it does not hold is reset
    // THEN the call rejects
    await expect(resetReminder("guided_tour")).rejects.toThrow("unknown reminder");
  });
});
