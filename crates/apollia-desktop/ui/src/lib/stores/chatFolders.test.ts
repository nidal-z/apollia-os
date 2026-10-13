import { beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  FOLDER_NAME_MAX_CHARS,
  chatFolders,
  cleanFolderName,
  collapsedFolderIds,
  createFolder,
  deleteFolder,
  folderIdOf,
  groupByFolder,
  loadFolders,
  moveSessionToFolder,
  parseCollapsedFolders,
  renameFolder,
  toggleFolderCollapsed,
} from "./chatFolders";
import type { ChatFolder } from "$lib/ipc/chat";

const invoke = vi.mocked(tauriInvoke);

const CLIENTS: ChatFolder = { id: "f-clients", name: "Clients", position: 0, session_ids: ["s-2", "s-1"] };
const DRAFTS: ChatFolder = { id: "f-drafts", name: "Drafts", position: 1, session_ids: [] };

/** Answer each command the way the runtime does, the folders read from `stored`. */
function runtime(stored: { folders: ChatFolder[] }): void {
  invoke.mockImplementation(async (command: string) => {
    if (command === "list_chat_folders") return stored.folders;
    if (command === "create_chat_folder") {
      return { id: "f-new", name: "New folder", position: 2, session_ids: [] };
    }
    return undefined;
  });
}

beforeEach(() => {
  invoke.mockReset();
  chatFolders.set([]);
  collapsedFolderIds.set(new Set());
});

describe("chat folders - the list cut in folders", () => {
  test("each folder holds its conversations in the order of the list, the rest follows", () => {
    // GIVEN a list sorted s-1, s-2, s-3 and a folder naming s-2 then s-1
    const sessions = [{ id: "s-1" }, { id: "s-2" }, { id: "s-3" }];

    // WHEN the list is cut in folders
    const grouped = groupByFolder(sessions, [CLIENTS, DRAFTS]);

    // THEN the folder shows its two conversations in the order of the list,
    // the empty folder is kept, and the third stays out of any folder
    expect(grouped.folders.map((g) => [g.folder.name, g.sessions.map((s) => s.id)])).toEqual([
      ["Clients", ["s-1", "s-2"]],
      ["Drafts", []],
    ]);
    expect(grouped.loose.map((s) => s.id)).toEqual(["s-3"]);
  });

  test("a conversation the list does not show is not invented by its folder", () => {
    // GIVEN a folder naming two conversations, one of which the list left out
    // WHEN the list is cut in folders
    const grouped = groupByFolder([{ id: "s-1" }], [CLIENTS]);

    // THEN the folder shows the one that is listed
    expect(grouped.folders[0].sessions).toEqual([{ id: "s-1" }]);
    expect(grouped.loose).toEqual([]);
  });

  test("the folder of a conversation is the one that names it, or none", () => {
    // GIVEN the two folders
    // WHEN the folder of a filed and of a loose conversation is asked
    // THEN the first is its folder and the second none
    expect(folderIdOf([CLIENTS, DRAFTS], "s-1")).toBe("f-clients");
    expect(folderIdOf([CLIENTS, DRAFTS], "s-3")).toBeNull();
  });
});

describe("chat folders - folded folders", () => {
  test("a folder folds, unfolds, and the choice is written for the next launch", () => {
    // GIVEN no folded folder
    // WHEN one is toggled twice
    toggleFolderCollapsed("f-clients");
    const folded = get(collapsedFolderIds).has("f-clients");
    toggleFolderCollapsed("f-clients");

    // THEN it was folded then unfolded
    expect(folded).toBe(true);
    expect(get(collapsedFolderIds).has("f-clients")).toBe(false);
  });

  test("a stored value no version wrote reads as nothing folded", () => {
    // GIVEN nothing stored, a broken value, an object, and a real list
    // WHEN each is read
    // THEN only the real list folds anything, and only its strings
    expect(parseCollapsedFolders(null).size).toBe(0);
    expect(parseCollapsedFolders("{not json").size).toBe(0);
    expect(parseCollapsedFolders('{"a":1}').size).toBe(0);
    expect(Array.from(parseCollapsedFolders('["f-1", 7, "f-2"]'))).toEqual(["f-1", "f-2"]);
  });
});

describe("chat folders - the commands", () => {
  test("the folders are read from the runtime, and a failed read keeps the last ones", async () => {
    // GIVEN a runtime holding one folder
    runtime({ folders: [CLIENTS] });

    // WHEN the folders are loaded, then loaded again while the runtime fails
    const read = await loadFolders();
    invoke.mockRejectedValue(new Error("runtime gone"));
    const failed = await loadFolders();

    // THEN the first read shows the folder and the second leaves it shown
    expect(read).toBe(true);
    expect(failed).toBe(false);
    expect(get(chatFolders)).toEqual([CLIENTS]);
  });

  test("a new folder is created under the given name and shown at once", async () => {
    // GIVEN a runtime holding one folder, shown
    runtime({ folders: [CLIENTS] });
    chatFolders.set([CLIENTS]);

    // WHEN a folder is created
    const created = await createFolder("New folder");

    // THEN the runtime was asked with the name, and the list ends with it
    expect(invoke).toHaveBeenCalledWith("create_chat_folder", { name: "New folder" });
    expect(created.id).toBe("f-new");
    expect(get(chatFolders).map((f) => f.id)).toEqual(["f-clients", "f-new"]);
  });

  test("renaming sends the trimmed name and reads the folders again; a blank name sends nothing", async () => {
    // GIVEN a runtime that will answer the folder under its new name
    runtime({ folders: [{ ...CLIENTS, name: "Customers" }] });

    // WHEN the folder is renamed with spaces around, then with a blank name
    await renameFolder("f-clients", "  Customers ");
    const calls = invoke.mock.calls.length;
    await renameFolder("f-clients", "   ");

    // THEN the first reached the runtime trimmed, the second did not
    expect(invoke).toHaveBeenCalledWith("rename_chat_folder", {
      folderId: "f-clients",
      name: "Customers",
    });
    expect(get(chatFolders)[0].name).toBe("Customers");
    expect(invoke.mock.calls.length).toBe(calls);
  });

  test("a name is cut at the limit the runtime holds", () => {
    // GIVEN a name of 130 characters
    // WHEN it is cleaned
    // THEN 100 are kept
    expect(cleanFolderName("n".repeat(130))?.length).toBe(FOLDER_NAME_MAX_CHARS);
    expect(cleanFolderName(" \t ")).toBeNull();
  });

  test("deleting a folder asks the runtime, forgets its fold, and reads what is left", async () => {
    // GIVEN two folders shown, the first folded, and a runtime left with the second
    chatFolders.set([CLIENTS, DRAFTS]);
    collapsedFolderIds.set(new Set(["f-clients"]));
    runtime({ folders: [DRAFTS] });

    // WHEN the first is deleted
    await deleteFolder("f-clients");

    // THEN the runtime was asked, the fold is forgotten, the second remains
    expect(invoke).toHaveBeenCalledWith("delete_chat_folder", { folderId: "f-clients" });
    expect(get(collapsedFolderIds).size).toBe(0);
    expect(get(chatFolders)).toEqual([DRAFTS]);
  });

  test("moving a conversation names its folder, or null for out of any folder", async () => {
    // GIVEN a runtime
    runtime({ folders: [CLIENTS] });

    // WHEN a conversation goes into a folder, then out of any folder
    await moveSessionToFolder("s-3", "f-clients");
    await moveSessionToFolder("s-3", null);

    // THEN both reached the runtime as asked, and the folders were read again
    expect(invoke).toHaveBeenCalledWith("move_chat_session_to_folder", {
      sessionId: "s-3",
      folderId: "f-clients",
    });
    expect(invoke).toHaveBeenCalledWith("move_chat_session_to_folder", {
      sessionId: "s-3",
      folderId: null,
    });
    expect(invoke.mock.calls.filter(([command]) => command === "list_chat_folders")).toHaveLength(2);
  });

  test("a move the runtime refuses is not hidden", async () => {
    // GIVEN a runtime that does not know the folder
    invoke.mockRejectedValue("folder not found: ghost");

    // WHEN a conversation is moved there
    // THEN the refusal reaches the caller with the words of the runtime
    await expect(moveSessionToFolder("s-1", "ghost")).rejects.toBe("folder not found: ghost");
  });
});
