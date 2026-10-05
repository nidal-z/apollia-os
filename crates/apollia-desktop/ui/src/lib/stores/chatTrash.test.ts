import { beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  CHAT_TRASH_DEFAULT,
  eraseFromTrash,
  loadTrash,
  parseChatTrash,
  removalConfirmKey,
  removeConversation,
  restoreFromTrash,
  trashedSessions,
  trashedWhen,
} from "./chatTrash";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalogue,
  );
}

const invoke = vi.mocked(tauriInvoke);

const DRAFT = {
  id: "s-1",
  mode: "libre",
  agent_name: null,
  title: "Letter draft",
  created_at: "2026-10-01T08:00:00Z",
  deleted_at: "2026-10-04T10:00:00Z",
};
const NOTES = { ...DRAFT, id: "s-2", title: "Loose notes", deleted_at: "2026-09-20T10:00:00Z" };

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
  trashedSessions.set([]);
});

describe("chat trash - the choice", () => {
  test("the trash is on by default, and an unknown stored value reads as the default", () => {
    // GIVEN nothing stored, a value no version wrote, and the two real ones
    // WHEN each is read
    // THEN only "off" turns the trash off
    expect(CHAT_TRASH_DEFAULT).toBe(true);
    expect(parseChatTrash(null)).toBe(true);
    expect(parseChatTrash("maybe")).toBe(true);
    expect(parseChatTrash("on")).toBe(true);
    expect(parseChatTrash("off")).toBe(false);
  });

  test("on, removing a conversation sends it to the trash", async () => {
    // GIVEN the trash on
    // WHEN a conversation is removed from the list
    await removeConversation("s-1", true);

    // THEN the runtime is asked to trash it, never to erase it
    expect(invoke).toHaveBeenCalledWith("trash_chat_session", { sessionId: "s-1" });
    expect(invoke).not.toHaveBeenCalledWith("delete_chat_session", expect.anything());
  });

  test("off, removing a conversation erases it for good, as before", async () => {
    // GIVEN the trash off
    // WHEN a conversation is removed from the list
    await removeConversation("s-1", false);

    // THEN the runtime is asked to erase it, and nothing goes to the trash
    expect(invoke).toHaveBeenCalledWith("delete_chat_session", { sessionId: "s-1" });
    expect(invoke).not.toHaveBeenCalledWith("trash_chat_session", expect.anything());
  });

  test("the confirmation says where the conversation goes", () => {
    // GIVEN the trash on, then off
    // WHEN the text of the confirmation is read in both languages
    const on = removalConfirmKey(true);
    const off = removalConfirmKey(false);

    // THEN on, it names the trash and the restore; off, it says nothing comes back
    expect(lookup(en, on)).toBe(
      "The conversation goes to the Apollia trash, where you can restore it.",
    );
    expect(lookup(fr, on)).toMatch(/corbeille/);
    expect(lookup(en, off)).toMatch(/cannot be undone/);
    expect(lookup(fr, off)).not.toBe(lookup(fr, on));
  });
});

describe("chat trash - the view", () => {
  test("the trash lists what the runtime holds, and a failed read leaves it as it was", async () => {
    // GIVEN a runtime whose trash holds two conversations
    invoke.mockResolvedValueOnce([DRAFT, NOTES]);

    // WHEN the trash is read, then read again while the runtime fails
    const first = await loadTrash();
    invoke.mockRejectedValueOnce(new Error("runtime away"));
    const second = await loadTrash();

    // THEN the two conversations are listed, and stay listed after the failure
    expect(invoke).toHaveBeenCalledWith("list_trashed_chat_sessions");
    expect(first).toBe(true);
    expect(second).toBe(false);
    expect(get(trashedSessions).map((s) => s.id)).toEqual(["s-1", "s-2"]);
  });

  test("restoring asks the runtime and takes the conversation out of the trash", async () => {
    // GIVEN two conversations in the trash
    trashedSessions.set([DRAFT, NOTES]);

    // WHEN the first is restored
    await restoreFromTrash("s-1");

    // THEN the runtime restored it, never erased it, and the trash holds the other
    expect(invoke).toHaveBeenCalledWith("restore_chat_session", { sessionId: "s-1" });
    expect(invoke).not.toHaveBeenCalledWith("delete_chat_session", expect.anything());
    expect(get(trashedSessions).map((s) => s.id)).toEqual(["s-2"]);
  });

  test("erasing for good asks the runtime to delete and empties the row", async () => {
    // GIVEN two conversations in the trash
    trashedSessions.set([DRAFT, NOTES]);

    // WHEN the second is erased for good
    await eraseFromTrash("s-2");

    // THEN the runtime deleted it and the trash holds the other
    expect(invoke).toHaveBeenCalledWith("delete_chat_session", { sessionId: "s-2" });
    expect(get(trashedSessions).map((s) => s.id)).toEqual(["s-1"]);
  });

  test("a restore the runtime refuses leaves the conversation in the trash", async () => {
    // GIVEN a conversation in the trash and a runtime that refuses
    trashedSessions.set([DRAFT]);
    invoke.mockRejectedValueOnce(new Error("session not found: s-1"));

    // WHEN it is restored
    const restored = restoreFromTrash("s-1");

    // THEN the refusal reaches the caller and the row stays
    await expect(restored).rejects.toThrow("session not found");
    expect(get(trashedSessions).map((s) => s.id)).toEqual(["s-1"]);
  });

  test("a row says when the conversation was removed: today, yesterday, or its date", () => {
    // GIVEN a reader on the 5th of October at noon
    const now = new Date(2026, 9, 5, 12, 0, 0);

    // WHEN conversations removed that morning, the evening before, and on the
    // 20th of September are read
    const today = trashedWhen(new Date(2026, 9, 5, 8, 0, 0).toISOString(), now, "fr");
    const yesterday = trashedWhen(new Date(2026, 9, 4, 23, 0, 0).toISOString(), now, "fr");
    const older = trashedWhen(new Date(2026, 8, 20, 10, 0, 0).toISOString(), now, "fr");
    const broken = trashedWhen("not a date", now, "fr");

    // THEN each reads in words, and a date that does not parse reads as none
    expect(today.key).toBe("chat.trash.removed_today");
    expect(yesterday.key).toBe("chat.trash.removed_yesterday");
    expect(older.key).toBe("chat.trash.removed_on");
    expect(older.values?.date).toMatch(/20/);
    expect(broken).toEqual({ key: "chat.trash.removed" });
    for (const key of [today.key, yesterday.key, older.key, broken.key]) {
      expect(typeof lookup(fr, key)).toBe("string");
      expect(typeof lookup(en, key)).toBe("string");
    }
  });
});
