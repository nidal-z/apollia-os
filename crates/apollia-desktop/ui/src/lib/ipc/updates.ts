/**
 * Typed Tauri command wrappers for the application updater.
 *
 * Backed by `tauri-plugin-updater`. The check is always operator-initiated:
 * nothing polls in the background, in line with the local-first principle.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface UpdateCheckResult {
  available: boolean;
  current_version: string;
  new_version?: string | null;
  release_notes?: string | null;
}

/**
 * Download progress emitted by `install_update` on the
 * `"update-download-progress"` event. `total` is null when the backend cannot
 * determine the payload size (indeterminate download).
 */
export interface UpdateDownloadProgress {
  downloaded: number;
  total: number | null;
}

/** Query the configured update endpoint. Resolves with the availability. */
export async function checkForUpdate(): Promise<UpdateCheckResult> {
  return invoke<UpdateCheckResult>("check_for_update");
}

/** Download and install the available update, then restart the app. */
export async function installUpdate(): Promise<void> {
  await invoke("install_update");
}

/**
 * Subscribe to the download-progress stream emitted by `install_update`.
 * Returns the unlisten handle; the caller must invoke it to stop listening.
 */
export async function listenUpdateDownloadProgress(
  cb: (p: UpdateDownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<UpdateDownloadProgress>("update-download-progress", (event) => {
    cb(event.payload);
  });
}
