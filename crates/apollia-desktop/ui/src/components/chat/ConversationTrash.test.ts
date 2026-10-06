import { describe, expect, test, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { trashHintKey, trashedTitle } from "./ConversationTrash.svelte";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalogue,
  );
}

const ROW = {
  id: "s-1",
  mode: "agent",
  agent_name: "Accountant",
  title: "Table trial",
  created_at: "2026-10-01T08:00:00Z",
  deleted_at: "2026-10-04T10:00:00Z",
};

describe("chat trash view", () => {
  test("the two actions of a row are Restore and Delete permanently, in both languages", () => {
    // GIVEN the catalogue of the view
    // WHEN the labels of the two actions and the title are read
    // THEN they are the words of the mockup
    expect(lookup(en, "chat.trash.title")).toBe("Trash");
    expect(lookup(en, "chat.trash.restore")).toBe("Restore");
    expect(lookup(en, "chat.trash.erase")).toBe("Delete permanently");
    expect(lookup(fr, "chat.trash.title")).toBe("Corbeille");
    expect(lookup(fr, "chat.trash.restore")).toBe("Restaurer");
    expect(lookup(fr, "chat.trash.erase")).toMatch(/^Supprimer d.finitivement$/);
  });

  test("a row is named by its title, else its agent, else the placeholder", () => {
    // GIVEN a titled conversation, one with a blank title, one with neither
    const titled = ROW;
    const blank = { ...ROW, title: "  " };
    const bare = { ...ROW, title: null, agent_name: null };

    // WHEN each is named
    // THEN the title wins, then the agent, then the placeholder
    expect(trashedTitle(titled, "Untitled")).toBe("Table trial");
    expect(trashedTitle(blank, "Untitled")).toBe("Accountant");
    expect(trashedTitle(bare, "Untitled")).toBe("Untitled");
  });

  test("the foot says where deleted conversations go, or that the trash is off", () => {
    // GIVEN the trash on, then off
    // WHEN the foot of the view is read
    const on = lookup(en, trashHintKey(true));
    const off = lookup(en, trashHintKey(false));

    // THEN on, it is the sentence of the mockup; off, it says a deletion is final
    expect(on).toBe("Deleted conversations go to the Apollia trash, where you can restore them.");
    expect(off).toMatch(/gone for good/);
    expect(typeof lookup(fr, trashHintKey(true))).toBe("string");
    expect(lookup(fr, trashHintKey(false))).not.toBe(lookup(fr, trashHintKey(true)));
  });
});
