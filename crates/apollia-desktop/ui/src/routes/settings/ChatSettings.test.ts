import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  CHAT_NAMING_CHOICES,
  CHAT_SETTINGS_BLOCKS,
  CHAT_SHORTCUT_ROWS,
  CHAT_TRASH_ROW,
  namingLocked,
  sendKeyRowKeys,
} from "./ChatSettings.svelte";
import { CHAT_AUTO_NAMING_DEFAULT } from "$lib/stores/chatAutoNaming";
import { CHAT_TRASH_DEFAULT } from "$lib/stores/chatTrash";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalogue,
  );
}

describe("Settings > Chat - the section as the mockup lays it out", () => {
  test("the blocks are Conversations, Keyboard shortcuts, Automatic naming, then all the shortcuts", () => {
    // GIVEN the section
    // WHEN its blocks are read in order
    const titles = CHAT_SETTINGS_BLOCKS.map((k) => lookup(en, k));

    // THEN they follow the mockup, and the send key sits in Keyboard shortcuts
    expect(titles).toEqual([
      "Conversations",
      "Keyboard shortcuts",
      "Automatic naming",
      "All shortcuts",
    ]);
    expect(CHAT_SHORTCUT_ROWS).toContain("chat-send-key");
  });

  test("the sort of the list is not a row of the section", () => {
    // GIVEN the maintainer's decision that the sort is set in the list only
    // WHEN the blocks and rows of the section are read
    const everything = [...CHAT_SETTINGS_BLOCKS, ...CHAT_SHORTCUT_ROWS].join(" ");

    // THEN none of them is the sort
    expect(everything).not.toMatch(/sort/);
  });

  test("the trash row says where deleted conversations go, and is on by default", () => {
    // GIVEN the trash row of the Conversations block
    // WHEN its label and its explanation are read in both languages
    const label = { fr: lookup(fr, CHAT_TRASH_ROW.labelKey), en: lookup(en, CHAT_TRASH_ROW.labelKey) };
    const hint = { fr: lookup(fr, CHAT_TRASH_ROW.hintKey), en: lookup(en, CHAT_TRASH_ROW.hintKey) };

    // THEN the label is the one of the mockup, the explanation names the
    // trash of Apollia and the restore, says what off does, never mentions a
    // trash of folders, and the switch starts on
    expect(label.fr).toBe("Envoyer les conversations supprimées à la corbeille");
    expect(label.en).toBe("Send deleted conversations to the trash");
    expect(hint.fr).toContain(
      "Les conversations supprimées vont dans la corbeille d'Apollia, d'où on les restaure",
    );
    expect(hint.fr).toContain("Éteint, une conversation supprimée l'est définitivement");
    expect(hint.en).toContain(
      "Deleted conversations go to the Apollia trash, where you can restore them",
    );
    expect(`${label.fr} ${hint.fr}`).not.toMatch(/dossier/i);
    expect(`${label.en} ${hint.en}`).not.toMatch(/folder/i);
    expect(CHAT_TRASH_DEFAULT).toBe(true);
    expect(CHAT_TRASH_ROW.testId).toBe("chat-trash-toggle");
  });

  test("naming offers never, automatic and always, each explained, automatic by default", () => {
    // GIVEN the naming block
    // WHEN its choices are read
    const values = CHAT_NAMING_CHOICES.map((c) => c.value);

    // THEN three choices in order, each with a label and an explanation in
    // both languages, and automatic is the default
    expect(values).toEqual(["never", "auto", "always"]);
    expect(CHAT_AUTO_NAMING_DEFAULT).toBe("auto");
    for (const c of CHAT_NAMING_CHOICES) {
      for (const catalogue of [fr, en]) {
        expect(typeof lookup(catalogue, c.labelKey)).toBe("string");
        expect(typeof lookup(catalogue, c.hintKey)).toBe("string");
      }
    }
    expect(lookup(en, "settings.chat.naming_auto_hint")).toMatch(/generation speed/);
  });

  test("the send key row names the modifier of the platform and says what the other key does", () => {
    // GIVEN a Mac and another platform
    // WHEN the row's texts are read
    const mac = sendKeyRowKeys(true);
    const other = sendKeyRowKeys(false);

    // THEN the Mac row reads ⌘, the other Ctrl, and both explain Shift + Enter
    expect(lookup(en, mac.labelKey)).toBe("Use ⌘ + ⏎ to send the message");
    expect(lookup(en, other.labelKey)).toBe("Use Ctrl + Enter to send the message");
    expect(lookup(en, mac.hintKey)).toMatch(/Shift \+ Enter starts a new line/);
    expect(lookup(en, other.hintKey)).toMatch(/Shift \+ Enter starts a new line/);
  });

  test("only a naming the policy imposes locks the choices", () => {
    // GIVEN the three sources of a value
    // WHEN the lock is computed for each
    // THEN only the policy locks
    expect(namingLocked("policy")).toBe(true);
    expect(namingLocked("user")).toBe(false);
    expect(namingLocked("default")).toBe(false);
  });
});
