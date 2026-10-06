/**
 * The folders of the conversation list.
 *
 * A folder is kept by the runtime, with the conversations it holds; which
 * folders are folded is a habit of this window and stays in `localStorage`.
 * Every change asks the runtime, then reads the folders again, so the list
 * shows what was stored and never a guess.
 */
import { writable } from "svelte/store";
import {
  createChatFolder,
  deleteChatFolder,
  listChatFolders,
  moveChatSessionToFolder,
  renameChatFolder,
  type ChatFolder,
} from "$lib/ipc/chat";

export const COLLAPSED_FOLDERS_STORAGE_KEY = "apollia-chat-folders-collapsed";

/** Longest name a folder takes, the limit the runtime holds. */
export const FOLDER_NAME_MAX_CHARS = 100;

/** The type a dragged conversation travels under. */
export const SESSION_DRAG_TYPE = "application/x-apollia-chat-session";

/** The folders, in the order of the list. */
export const chatFolders = writable<ChatFolder[]>([]);

/** A stored list of folded folders, or none when it is missing or unreadable. */
export function parseCollapsedFolders(raw: string | null): Set<string> {
  if (!raw) return new Set();
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((id): id is string => typeof id === "string"));
  } catch {
    return new Set();
  }
}

function readCollapsed(): Set<string> {
  if (typeof localStorage === "undefined") return new Set();
  try {
    return parseCollapsedFolders(localStorage.getItem(COLLAPSED_FOLDERS_STORAGE_KEY));
  } catch {
    return new Set();
  }
}

/** The folders shown folded: their conversations are hidden. */
export const collapsedFolderIds = writable<Set<string>>(readCollapsed());

collapsedFolderIds.subscribe((ids) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(COLLAPSED_FOLDERS_STORAGE_KEY, JSON.stringify(Array.from(ids)));
  } catch {
    /* quota or privacy mode: the habit lives for this session only */
  }
});

/** Fold an unfolded folder, unfold a folded one. */
export function toggleFolderCollapsed(folderId: string): void {
  collapsedFolderIds.update((ids) => {
    const next = new Set(ids);
    if (!next.delete(folderId)) next.add(folderId);
    return next;
  });
}

/** Read the folders from the runtime. A failed read leaves them as they were. */
export async function loadFolders(): Promise<boolean> {
  try {
    chatFolders.set(await listChatFolders());
    return true;
  } catch (err: unknown) {
    console.warn("list_chat_folders failed:", err);
    return false;
  }
}

/** The name a folder is created or renamed with, or `null` when it is blank. */
export function cleanFolderName(raw: string): string | null {
  const name = raw.trim().slice(0, FOLDER_NAME_MAX_CHARS).trim();
  return name === "" ? null : name;
}

/** Create a folder named `name`; the folder created, shown at once. */
export async function createFolder(name: string): Promise<ChatFolder> {
  const folder = await createChatFolder(name);
  chatFolders.update((folders) => [...folders, folder]);
  return folder;
}

/** Rename a folder. A blank name changes nothing. */
export async function renameFolder(folderId: string, rawName: string): Promise<void> {
  const name = cleanFolderName(rawName);
  if (name === null) return;
  await renameChatFolder(folderId, name);
  await loadFolders();
}

/** Remove a folder; its conversations stay in the list, out of any folder. */
export async function deleteFolder(folderId: string): Promise<void> {
  await deleteChatFolder(folderId);
  collapsedFolderIds.update((ids) => {
    if (!ids.has(folderId)) return ids;
    const next = new Set(ids);
    next.delete(folderId);
    return next;
  });
  await loadFolders();
}

/** Put a conversation in a folder, or out of any folder with `null`. */
export async function moveSessionToFolder(
  sessionId: string,
  folderId: string | null,
): Promise<void> {
  await moveChatSessionToFolder(sessionId, folderId);
  await loadFolders();
}

/** The folder a conversation is in, or `null`. */
export function folderIdOf(folders: readonly ChatFolder[], sessionId: string): string | null {
  return folders.find((folder) => folder.session_ids.includes(sessionId))?.id ?? null;
}

/** A folder and the conversations of the list it holds. */
export interface FolderGroup<S> {
  folder: ChatFolder;
  sessions: S[];
}

/** The list cut in folders: each folder with its conversations, then the rest. */
export interface GroupedSessions<S> {
  folders: FolderGroup<S>[];
  loose: S[];
}

/**
 * Cut `sessions` in folders. The order of `sessions` is kept inside each
 * folder and among the conversations out of any folder, so the list keeps the
 * order chosen for it; a folder with none of them is kept, empty.
 */
export function groupByFolder<S extends { id: string }>(
  sessions: readonly S[],
  folders: readonly ChatFolder[],
): GroupedSessions<S> {
  const folderOf = new Map<string, string>();
  for (const folder of folders) {
    for (const sessionId of folder.session_ids) folderOf.set(sessionId, folder.id);
  }
  const groups = new Map<string, S[]>(folders.map((folder) => [folder.id, []]));
  const loose: S[] = [];
  for (const session of sessions) {
    const folderId = folderOf.get(session.id);
    const group = folderId === undefined ? undefined : groups.get(folderId);
    if (group) group.push(session);
    else loose.push(session);
  }
  return {
    folders: folders.map((folder) => ({ folder, sessions: groups.get(folder.id) ?? [] })),
    loose,
  };
}

/** Mark a drag as carrying the conversation `sessionId`. */
export function writeSessionDrag(transfer: DataTransfer | null, sessionId: string): void {
  if (!transfer) return;
  transfer.setData(SESSION_DRAG_TYPE, sessionId);
  transfer.effectAllowed = "move";
}

/** Whether a drag over a folder carries a conversation. */
export function carriesSession(transfer: DataTransfer | null): boolean {
  return transfer ? Array.from(transfer.types).includes(SESSION_DRAG_TYPE) : false;
}

/** The conversation a drop carries, or `null` when it carries something else. */
export function readSessionDrop(transfer: DataTransfer | null): string | null {
  const sessionId = transfer?.getData(SESSION_DRAG_TYPE) ?? "";
  return sessionId === "" ? null : sessionId;
}
