/**
 * Typed Tauri command wrappers for the OneDrive folders and files a Microsoft
 * account lets Apollia read. The token of the account sees the whole drive;
 * these choices are the limit Apollia holds, read before every OneDrive call
 * of an agent. Error handling stays at the call site.
 */
import { invoke } from "@tauri-apps/api/core";

/** One chosen folder or file. */
export interface OneDriveChoice {
  id: string;
  name: string;
  /** A folder admits everything under it. */
  is_folder: boolean;
  /** Place of the item in the drive, as OneDrive gave it. */
  path: string;
}

/** One item of a folder being browsed. */
export interface OneDriveEntry {
  id: string;
  name: string;
  is_folder: boolean;
  /** The item itself is already chosen. */
  chosen: boolean;
}

/** The folders and files `accountId` lets Apollia read. */
export function onedriveListChoices(accountId: string): Promise<OneDriveChoice[]> {
  return invoke<OneDriveChoice[]>("onedrive_list_choices", { accountId });
}

/** The children of `folderId`, or of the root of the drive when null. */
export function onedriveBrowse(
  accountId: string,
  folderId: string | null,
): Promise<OneDriveEntry[]> {
  return invoke<OneDriveEntry[]>("onedrive_browse", { accountId, folderId });
}

/** Let Apollia read the folder or file `itemId`; answers the choices. */
export function onedriveChoose(accountId: string, itemId: string): Promise<OneDriveChoice[]> {
  return invoke<OneDriveChoice[]>("onedrive_choose", { accountId, itemId });
}

/** Stop letting Apollia read `itemId`; answers the choices. */
export function onedriveForget(accountId: string, itemId: string): Promise<OneDriveChoice[]> {
  return invoke<OneDriveChoice[]>("onedrive_forget", { accountId, itemId });
}
