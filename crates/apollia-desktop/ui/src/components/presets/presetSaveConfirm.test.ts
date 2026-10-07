import { beforeAll, describe, expect, test, vi } from "vitest";
import { render } from "svelte/server";
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

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import PresetSaveConfirmDialog, {
  confirmPresetSave,
  holdPresetSave,
  type PendingPresetSave,
} from "./PresetSaveConfirmDialog.svelte";
import { PRESET_FIELD_KEYS } from "$lib/chat/sessionSettings";
import {
  STRUCTURED_LABEL_KEY,
  changedFieldKeys,
  formFromPreset,
} from "../ai/modelDetail";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

const HERE = path.dirname(fileURLToPath(import.meta.url));

function source(relative: string): string {
  return readFileSync(path.resolve(HERE, relative), "utf8");
}

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) =>
      node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
    catalogue,
  );
}

/** A save that records whether it wrote. */
function save(fieldKeys: string[], name: string | null = "Careful") {
  const written: string[] = [];
  const pending: PendingPresetSave = { name, fieldKeys, run: () => void written.push("written") };
  return { pending, written };
}

beforeAll(async () => {
  locale.set("fr");
  await waitLocale();
});

describe("saving into a preset, with and without the confirmation", () => {
  test("with the confirmation off, the save is written at once", () => {
    // GIVEN the setting off, as it is by default
    const { pending, written } = save([PRESET_FIELD_KEYS.temperature]);
    // WHEN a save is asked
    const held = holdPresetSave(false, pending);
    // THEN it is written and nothing waits for a confirmation
    expect(held).toBeNull();
    expect(written).toEqual(["written"]);
  });

  test("with it on, the save waits and Cancel writes nothing", () => {
    // GIVEN the setting on
    const { pending, written } = save([PRESET_FIELD_KEYS.temperature]);
    // WHEN a save is asked, then cancelled: the dialog drops what it held
    let held = holdPresetSave(true, pending);
    expect(held).toBe(pending);
    expect(written).toEqual([]);
    held = null;
    confirmPresetSave(held);
    // THEN nothing was written
    expect(written).toEqual([]);
  });

  test("with it on, confirming writes the save once", () => {
    // GIVEN the setting on and a save that waits
    const { pending, written } = save([PRESET_FIELD_KEYS.max_tokens]);
    const held = holdPresetSave(true, pending);
    // WHEN it is confirmed
    confirmPresetSave(held);
    // THEN it is written, once
    expect(written).toEqual(["written"]);
  });
});

describe("the confirmation dialog", () => {
  test("it lists the settings changed and names the preset", () => {
    // GIVEN a save of two settings into the preset "Careful"
    const { pending } = save([PRESET_FIELD_KEYS.temperature, PRESET_FIELD_KEYS.max_tokens]);
    // WHEN the dialog renders
    const html = render(PresetSaveConfirmDialog, {
      props: { pending, onsettled: () => {}, testid: "confirm" },
    }).body;
    // THEN both settings are listed, the preset is named, and the two
    //      buttons are there
    expect(html.split('data-testid="confirm-field"')).toHaveLength(3);
    expect(html).toContain(String(lookup(fr, PRESET_FIELD_KEYS.temperature)));
    expect(html).toContain(String(lookup(fr, PRESET_FIELD_KEYS.max_tokens)));
    expect(html).toContain("Careful");
    expect(html).not.toContain("{name}");
    expect(html).toContain('data-testid="confirm-cancel"');
    expect(html).toContain('data-testid="confirm-confirm"');
  });

  test("it says so when no setting was changed", () => {
    // GIVEN a save that changes nothing, of a preset not named yet
    const { pending } = save([], null);
    // WHEN the dialog renders
    const html = render(PresetSaveConfirmDialog, {
      props: { pending, onsettled: () => {}, testid: "confirm" },
    }).body;
    // THEN it lists nothing and says no setting was changed
    expect(html).not.toContain('data-testid="confirm-fields"');
    const none = String(lookup(fr, "presets.save_confirm.no_fields"));
    expect(html).toContain(none.slice(0, none.indexOf("'")));
  });

  test("nothing renders while no save waits", () => {
    // GIVEN no save held
    // WHEN the dialog renders
    const html = render(PresetSaveConfirmDialog, {
      props: { pending: null, onsettled: () => {}, testid: "confirm" },
    }).body;
    // THEN it is closed
    expect(html).not.toContain('data-testid="confirm-cancel"');
  });

  test("its texts exist in French and in English", () => {
    // GIVEN the keys of the dialog
    // WHEN they are read in both catalogues
    // THEN each exists and the two languages differ
    for (const key of ["title", "message", "message_named", "no_fields", "confirm"]) {
      const full = `presets.save_confirm.${key}`;
      expect(typeof lookup(fr, full), full).toBe("string");
      expect(typeof lookup(en, full), full).toBe("string");
      expect(lookup(fr, full), full).not.toBe(lookup(en, full));
    }
  });
});

describe("the settings a save of the model panel would change", () => {
  test("they are listed by label, in the order of the tabs", () => {
    // GIVEN a panel opened on the automatic settings, then a context, a
    //       temperature and a schema typed
    const baseline = formFromPreset(null);
    const form = formFromPreset(null);
    form.load.n_ctx = "8192";
    form.request.temperature = "0.2";
    form.structured = true;
    form.schema = "{}";
    // WHEN the changes are read, presets keeping the load settings
    const keys = changedFieldKeys(form, baseline, true);
    // THEN the three are named, load first, and each has a text in both languages
    expect(keys).toEqual([
      "ai.panel.load.field.n_ctx.label",
      "ai.panel.inference.field.temperature.label",
      STRUCTURED_LABEL_KEY,
    ]);
    for (const key of keys) {
      expect(typeof lookup(fr, key), key).toBe("string");
      expect(typeof lookup(en, key), key).toBe("string");
    }
  });

  test("a load setting is left out while presets do not keep them", () => {
    // GIVEN the same changes, presets not keeping the load settings
    const baseline = formFromPreset(null);
    const form = formFromPreset(null);
    form.load.n_ctx = "8192";
    form.request.temperature = "0.2";
    // WHEN the changes are read
    // THEN only what the save writes is named, and nothing when nothing changed
    expect(changedFieldKeys(form, baseline, false)).toEqual([
      "ai.panel.inference.field.temperature.label",
    ]);
    expect(changedFieldKeys(baseline, baseline, true)).toEqual([]);
  });
});

describe("the two places that save into a preset go through the confirmation", () => {
  test("the Settings tab of the chat holds its save behind it", () => {
    // GIVEN the source of the tab
    const src = source("../chat/settings/SessionSettingsTab.svelte");
    // WHEN its "Save as a preset" button is read
    // THEN it asks through the confirmation, and never opens the name
    //      dialog on its own
    expect(src).toContain("onclick={askSaveAs}");
    expect(src).toContain("holdPresetSave($presetSaveConfirm");
    expect(src).not.toContain("onclick={() => (saving = true)}");
  });

  test("the model panel holds a new preset and an overwrite behind it", () => {
    // GIVEN the source of the panel
    const src = source("../ai/ModelPanel.svelte");
    // WHEN its writes are read
    // THEN creating and updating a preset are both held, and neither is
    //      called outside a held save
    expect(src.split("holdPresetSave($presetSaveConfirm")).toHaveLength(3);
    const held = src.split("pendingSave = holdPresetSave(").slice(1).join("");
    expect(held).toContain("createModelPreset(id, preset)");
    expect(held).toContain("updateModelPreset(preset.id, body)");
    expect(src.split("createModelPreset(id, preset)")).toHaveLength(2);
    expect(src.split("updateModelPreset(preset.id, body)")).toHaveLength(2);
  });
});
