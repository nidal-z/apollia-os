import { beforeAll, describe, test, expect, vi } from "vitest";
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

import ChatSettings, {
  CHAT_CONVERSATION_ROWS,
  CHAT_NAMING_CHOICES,
  CHAT_SETTINGS_BLOCKS,
  CHAT_SHORTCUT_ROWS,
  CHAT_TOKEN_COUNT_ROW,
  CHAT_TRASH_ROW,
  conversationRowTestId,
  namingLocked,
  regenerateRowKeys,
  sendKeyRowKeys,
} from "./ChatSettings.svelte";
import { CHAT_BEHAVIOUR_KEYS, chatBehaviours } from "$lib/stores/chatBehaviours";
import { SHORTCUTS } from "$lib/navigation/shortcuts";
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

beforeAll(async () => {
  locale.set("fr");
  await waitLocale("fr");
});

describe("Settings > Chat - how the chat behaves", () => {
  test("the Conversations block carries its rows in the order of the reference", () => {
    // GIVEN the Conversations block
    // WHEN its rows are read in order
    const ids = CHAT_CONVERSATION_ROWS.map(conversationRowTestId);

    // THEN one empty conversation, unloading before loading, the trash, the
    // double click that edits, the token count, the double click that
    // renames, and the plan mode Apollia adds last
    expect(ids).toEqual([
      "chat-single-empty",
      "chat-unload-before-load",
      "chat-trash-toggle",
      "chat-double-click-edit",
      "chat-token-count",
      "chat-double-click-rename",
      "plan-mode-default-toggle",
    ]);
  });

  test("the page renders those rows in that order, then the regeneration shortcut", () => {
    // GIVEN the section, in French
    // WHEN it renders
    const html = render(ChatSettings).body;
    const at = (testId: string) => html.indexOf(`data-testid="${testId}"`);

    // THEN every row is on the page, each after the one before it, and the
    // regeneration row follows the send key in Keyboard shortcuts
    const order = [
      ...CHAT_CONVERSATION_ROWS.map(conversationRowTestId),
      "chat-send-key",
      "chat-regenerate-shortcut",
    ];
    const positions = order.map(at);
    expect(positions.every((p) => p > -1)).toBe(true);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
  });

  test("each row has its label and its explanation in French and in English", () => {
    // GIVEN the five rows of the story and the regeneration row
    const rows = [
      ...CHAT_CONVERSATION_ROWS.flatMap((r) => (r.kind === "behaviour" ? [r.row] : [])),
      CHAT_TOKEN_COUNT_ROW,
      regenerateRowKeys(true),
      regenerateRowKeys(false),
    ];

    // WHEN their texts are read in both catalogues
    // THEN each exists, and the two languages differ
    expect(rows).toHaveLength(7);
    for (const row of rows) {
      for (const key of [row.labelKey, row.hintKey]) {
        const texts = { fr: lookup(fr, key), en: lookup(en, key) };
        expect(typeof texts.fr, key).toBe("string");
        expect(typeof texts.en, key).toBe("string");
        expect(texts.fr, key).not.toBe(texts.en);
      }
    }
    expect(lookup(fr, "settings.chat.single_empty_label")).toBe(
      "Une seule conversation vide à la fois",
    );
    expect(lookup(fr, "settings.chat.unload_before_load_label")).toBe(
      "Décharger les modèles chargés avant d'en charger un autre",
    );
    expect(lookup(fr, "settings.chat.unload_before_load_hint")).toContain(
      "plusieurs modèles restent chargés",
    );
    expect(lookup(fr, "settings.chat.double_click_edit_hint")).toBe(
      "Allumé, un double-clic sur un de vos messages l'ouvre en modification. Éteint, on le modifie par son action dédiée.",
    );
    expect(lookup(en, "settings.chat.regenerate_label_mac")).toBe(
      "Use ⌘ + R to regenerate the last answer",
    );
    expect(lookup(en, "settings.chat.regenerate_label_other")).toBe(
      "Use Ctrl + R to regenerate the last answer",
    );
    expect(lookup(en, "settings.chat.regenerate_hint_mac")).toMatch(/reloads the last model/);
  });

  test("the switches start as the specification says", () => {
    // GIVEN the defaults in force
    // WHEN the page renders
    const html = render(ChatSettings).body;
    // The opening tag of the switch that carries the test id.
    const checked = (testId: string) => {
      const at = html.indexOf(`data-testid="${testId}-switch"`);
      const tag = html.slice(html.lastIndexOf("<", at), html.indexOf(">", at));
      expect(tag, testId).toMatch(/aria-checked="(true|false)"/);
      return tag.includes('aria-checked="true"');
    };

    // THEN one empty conversation, renaming and the shortcut are on;
    // unloading, editing and the token count are off
    expect(checked("chat-single-empty")).toBe(true);
    expect(checked("chat-double-click-rename")).toBe(true);
    expect(checked("chat-regenerate-shortcut")).toBe(true);
    expect(checked("chat-unload-before-load")).toBe(false);
    expect(checked("chat-double-click-edit")).toBe(false);
    expect(checked("chat-token-count")).toBe(false);
  });

  test("a behaviour the policy imposes shows its lock, the others do not", () => {
    // GIVEN a policy imposing one model at a time
    chatBehaviours.update((all) => ({
      ...all,
      [CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad]: { value: true, source: "policy" },
    }));

    // WHEN the page renders
    const html = render(ChatSettings).body;
    chatBehaviours.update((all) => ({
      ...all,
      [CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad]: { value: false, source: "default" },
    }));

    // THEN that row names the policy and no other row does
    expect(html).toContain('data-testid="chat-unload-before-load-policy"');
    expect(html).not.toContain('data-testid="chat-single-empty-policy"');
    expect(html).not.toContain('data-testid="chat-double-click-rename-policy"');
  });

  test("the regeneration shortcut is in the catalogue of all the shortcuts", () => {
    // GIVEN the catalogue
    // WHEN the regeneration entry is looked up
    const entry = SHORTCUTS.find((s) => s.id === "chat.regenerate");

    // THEN it reads Cmd R on a Mac and Ctrl R elsewhere, in the chat, with a
    // description in both languages that says the second meaning
    expect(entry?.combo).toEqual({ mac: "⌘+R", win: "Ctrl+R" });
    expect(entry?.category).toBe("chat");
    expect(lookup(fr, entry?.descriptionKey ?? "")).toMatch(/recharger le dernier modèle/);
    expect(lookup(en, entry?.descriptionKey ?? "")).toMatch(/reload the last model/);
  });
});

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
