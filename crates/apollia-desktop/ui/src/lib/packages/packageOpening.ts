/**
 * A `.apo` package that reaches the window opens its preview.
 *
 * Two gestures end here, beside the buttons that open a file chooser:
 *
 * - **a file opened from the system** (a double-click, "Open with"): the
 *   shell queues its path and emits `PACKAGE_OPENED_EVENT`; the paths are
 *   taken when the window mounts, for the file that started the application,
 *   and again at each event;
 * - **a file dropped on the window**: the native drag-and-drop event of the
 *   webview carries the paths of the dropped files.
 *
 * Only a package file opens the preview. Any other file is left alone here,
 * so that a surface with its own use for a dropped file keeps it. The package
 * is installed or updated only from the preview.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { get } from "svelte/store";
import { takeOpenedPackageFiles } from "$lib/ipc/packageFiles";
import { openPackagePreview, packagePreviewRequest } from "$lib/stores/packagePreview";

/** Tauri event the shell emits when package files wait to be taken. */
export const PACKAGE_OPENED_EVENT = "package-file-opened";

/** Whether `path` names a package file, whatever the case of its extension. */
export function isPackageFile(path: string): boolean {
  return /[^\\/]\.apo$/i.test(path);
}

/** The first package file among `paths`, `null` when there is none. */
export function packageAmong(paths: readonly string[]): string | null {
  return paths.find(isPackageFile) ?? null;
}

/**
 * Opens the preview on the first package among `paths` and says whether it
 * did. A preview already open stays: the user answers the package in front of
 * them, and an installation under way is never swapped for another file.
 */
export function openPackageAmong(paths: readonly string[]): boolean {
  const path = packageAmong(paths);
  if (path === null || get(packagePreviewRequest) !== null) return false;
  openPackagePreview(path);
  return true;
}

/** Takes the package files the system opened and opens the first one. */
export async function openPackagesFromSystem(): Promise<void> {
  openPackageAmong(await takeOpenedPackageFiles());
}

/**
 * Starts listening for package files opened from the system or dropped on the
 * window, and returns what stops it. Outside the desktop shell there is no
 * window to listen to, and nothing is attached.
 */
export function mountPackageOpening(): () => void {
  const unlisteners: UnlistenFn[] = [];
  let stopped = false;
  const keep = (pending: Promise<UnlistenFn>): void => {
    pending
      .then((unlisten) => {
        if (stopped) unlisten();
        else unlisteners.push(unlisten);
      })
      .catch(() => {});
  };
  const fromSystem = (): void => {
    openPackagesFromSystem().catch(() => {});
  };

  try {
    keep(listen(PACKAGE_OPENED_EVENT, fromSystem));
    keep(
      getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "drop") openPackageAmong(event.payload.paths);
      }),
    );
  } catch {
    // No desktop shell behind this page.
  }
  // The file that started the application was queued before this mounted.
  fromSystem();

  return () => {
    stopped = true;
    for (const unlisten of unlisteners) unlisten();
  };
}
