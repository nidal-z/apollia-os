import { beforeAll, beforeEach, describe, expect, test, vi } from "vitest";
import { render } from "svelte/server";
import { get } from "svelte/store";
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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import General, { GENERAL_BEHAVIOUR_ROWS } from "./General.svelte";
import type { ResolvedSetting } from "$lib/ipc/settings";
import {
  GENERAL_BEHAVIOUR_DEFAULTS,
  GENERAL_BEHAVIOUR_KEYS,
  generalBehaviourDefaults,
  generalBehaviours,
  hydrateGeneralBehaviours,
  modelLoadWindow,
  presetSaveConfirm,
  setGeneralBehaviour,
  type GeneralBehavioursIpc,
} from "$lib/stores/generalBehaviours";
import {
  MODEL_FULL_FILE_NAME_DEFAULT,
  MODEL_FULL_FILE_NAME_STORAGE_KEY,
  modelFullFileName,
  parseModelFullFileName,
} from "$lib/stores/modelFullFileName";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) =>
      node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
    catalogue,
  );
}

/** A shell holding `file`, the keys `policy` imposes refusing a write. */
function memoryShell(
  file: Record<string, unknown> = {},
  policy: Record<string, unknown> = {},
): { shell: GeneralBehavioursIpc; file: Record<string, unknown>; writes: string[] } {
  const writes: string[] = [];
  const resolve = (key: string): ResolvedSetting => {
    if (key in policy) return { key, value: policy[key], source: "policy" };
    if (key in file) return { key, value: file[key], source: "user" };
    const fallback = GENERAL_BEHAVIOUR_DEFAULTS[key as keyof typeof GENERAL_BEHAVIOUR_DEFAULTS];
    return { key, value: fallback, source: "default" };
  };
  return {
    file,
    writes,
    shell: {
      read: async (key) => resolve(key),
      write: async (key, value) => {
        if (key in policy) throw new Error(`${key} is imposed by the policy`);
        writes.push(key);
        file[key] = value;
        return resolve(key);
      },
    },
  };
}

/** Whether the switch of the row `testId` is on in `html`. */
function checked(html: string, testId: string): boolean {
  const at = html.indexOf(`data-testid="${testId}-switch"`);
  const tag = html.slice(html.lastIndexOf("<", at), html.indexOf(">", at));
  expect(tag, testId).toMatch(/aria-checked="(true|false)"/);
  return tag.includes('aria-checked="true"');
}

beforeAll(async () => {
  locale.set("fr");
  await waitLocale();
});

beforeEach(() => {
  generalBehaviours.set(generalBehaviourDefaults());
  modelFullFileName.set(MODEL_FULL_FILE_NAME_DEFAULT);
});

describe("Settings > General - the Behaviours block", () => {
  test("the block holds the downloads panel, then the three switches, in the order of the page", () => {
    // GIVEN the rows the block declares
    // WHEN their identifiers are read
    const ids = GENERAL_BEHAVIOUR_ROWS.map((row) => row.testId);

    // THEN the downloads panel comes first, then the loading window, the
    // file name and the confirmation of the presets
    expect(ids).toEqual([
      "general-downloads-open",
      "general-model-load-window",
      "general-model-full-file-name",
      "general-preset-save-confirm",
    ]);
  });

  test("the page renders those rows in that order, under the language", () => {
    // GIVEN the section, in French
    // WHEN it renders
    const html = render(General).body;
    const at = (testId: string) => html.indexOf(`data-testid="${testId}"`);

    // THEN the block follows the language, and every row is on the page,
    // each after the one before it
    const order = [
      "language-select",
      "general-behaviours",
      ...GENERAL_BEHAVIOUR_ROWS.map((row) => row.testId),
    ];
    const positions = order.map(at);
    expect(positions.every((p) => p > -1)).toBe(true);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
    expect(html).toContain(String(lookup(fr, "settings.general.model_load_window_label")));
  });

  test("each row has its label and its explanation in French and in English", () => {
    // GIVEN the rows of the block
    // WHEN their texts are read in both catalogues
    // THEN each exists, and the two languages differ
    for (const row of GENERAL_BEHAVIOUR_ROWS) {
      for (const key of [row.labelKey, row.hintKey]) {
        const texts = { fr: lookup(fr, key), en: lookup(en, key) };
        expect(typeof texts.fr, key).toBe("string");
        expect(typeof texts.en, key).toBe("string");
        expect(texts.fr, key).not.toBe(texts.en);
      }
    }
    expect(lookup(en, "settings.general.model_full_file_name_label")).toBe(
      "Always show the full file name",
    );
    expect(lookup(en, "settings.general.preset_save_confirm_label")).toBe(
      "Confirm before saving to a preset",
    );
    expect(lookup(en, "settings.general.model_load_window_label")).toBe(
      "Always open the full loading window",
    );
  });

  test("the loading window starts on, the three others off", () => {
    // GIVEN the defaults in force
    // WHEN the page renders
    const html = render(General).body;

    // THEN only the full loading window is on
    expect(checked(html, "general-model-load-window")).toBe(true);
    expect(checked(html, "general-downloads-open")).toBe(false);
    expect(checked(html, "general-model-full-file-name")).toBe(false);
    expect(checked(html, "general-preset-save-confirm")).toBe(false);
  });

  test("a value the policy imposes locks its row and names the policy", () => {
    // GIVEN the loading window turned off by the organization policy
    generalBehaviours.update((all) => ({
      ...all,
      [GENERAL_BEHAVIOUR_KEYS.modelLoadWindow]: { value: false, source: "policy" },
    }));

    // WHEN the page renders
    const html = render(General).body;

    // THEN that row shows the value imposed and the policy mention, and the
    // rows the policy does not hold show none
    expect(checked(html, "general-model-load-window")).toBe(false);
    expect(html).toContain('data-testid="general-model-load-window-policy"');
    expect(html).toContain(String(lookup(fr, "settings.policy_locked")));
    expect(html).not.toContain('data-testid="general-preset-save-confirm-policy"');
    expect(html).not.toContain('data-testid="general-model-full-file-name-policy"');
  });
});

describe("the general behaviours written to the configuration file", () => {
  test("the defaults are the ones of the specification", () => {
    // GIVEN a store nobody wrote to
    // WHEN the two behaviours are read
    // THEN the loading window opens, and a preset is saved without asking
    expect(get(modelLoadWindow)).toBe(true);
    expect(get(presetSaveConfirm)).toBe(false);
    expect(GENERAL_BEHAVIOUR_DEFAULTS).toEqual({
      "desktop.model_load_window": true,
      "desktop.preset_save_confirm": false,
    });
  });

  test("a value written survives a restart", async () => {
    // GIVEN a shell, and both switches turned the other way
    const { shell, file, writes } = memoryShell();
    expect(await setGeneralBehaviour(GENERAL_BEHAVIOUR_KEYS.modelLoadWindow, false, shell)).toBe(
      true,
    );
    expect(await setGeneralBehaviour(GENERAL_BEHAVIOUR_KEYS.presetSaveConfirm, true, shell)).toBe(
      true,
    );
    expect(writes).toEqual(["desktop.model_load_window", "desktop.preset_save_confirm"]);
    expect(file).toEqual({
      "desktop.model_load_window": false,
      "desktop.preset_save_confirm": true,
    });

    // WHEN the application starts again: the store forgets, then is seeded
    generalBehaviours.set(generalBehaviourDefaults());
    await hydrateGeneralBehaviours(shell);

    // THEN both values are the ones written
    expect(get(modelLoadWindow)).toBe(false);
    expect(get(presetSaveConfirm)).toBe(true);
    expect(get(generalBehaviours)[GENERAL_BEHAVIOUR_KEYS.modelLoadWindow].source).toBe("user");
  });

  test("a value the policy imposes is read and cannot be written", async () => {
    // GIVEN a policy that imposes the confirmation of the presets
    const { shell, writes } = memoryShell({}, { "desktop.preset_save_confirm": true });
    await hydrateGeneralBehaviours(shell);
    expect(get(presetSaveConfirm)).toBe(true);

    // WHEN the switch is turned off
    const written = await setGeneralBehaviour(
      GENERAL_BEHAVIOUR_KEYS.presetSaveConfirm,
      false,
      shell,
    );

    // THEN nothing is written and the value stays
    expect(written).toBe(false);
    expect(writes).toEqual([]);
    expect(get(presetSaveConfirm)).toBe(true);
  });

  test("a write the shell refuses puts the switch back", async () => {
    // GIVEN a shell that cannot write
    const shell: GeneralBehavioursIpc = {
      read: async (key) => ({ key, value: true, source: "default" }),
      write: async () => {
        throw new Error("the file cannot be written");
      },
    };

    // WHEN the loading window is turned off
    const written = await setGeneralBehaviour(
      GENERAL_BEHAVIOUR_KEYS.modelLoadWindow,
      false,
      shell,
    );

    // THEN the write is reported failed and the value is the former one
    expect(written).toBe(false);
    expect(get(modelLoadWindow)).toBe(true);
  });
});

describe("the full file name, a display preference", () => {
  test("it is off by default and reads only the two values it writes", () => {
    // GIVEN stored values, known and unknown
    // WHEN each is parsed
    // THEN "on" and "off" are read, anything else is the default
    expect(MODEL_FULL_FILE_NAME_DEFAULT).toBe(false);
    expect(parseModelFullFileName("on")).toBe(true);
    expect(parseModelFullFileName("off")).toBe(false);
    expect(parseModelFullFileName(null)).toBe(false);
    expect(parseModelFullFileName("true")).toBe(false);
  });

  test("turning it on is stored beside the theme, never in the configuration file", () => {
    // GIVEN the preference off
    // WHEN it is turned on
    modelFullFileName.set(true);

    // THEN the browser storage holds it, which is what a restart reads
    const stored = localStorage.getItem(MODEL_FULL_FILE_NAME_STORAGE_KEY);
    expect(stored).toBe("on");
    expect(parseModelFullFileName(stored)).toBe(true);
    expect(Object.values(GENERAL_BEHAVIOUR_KEYS)).toEqual([
      "desktop.model_load_window",
      "desktop.preset_save_confirm",
    ]);
  });
});
