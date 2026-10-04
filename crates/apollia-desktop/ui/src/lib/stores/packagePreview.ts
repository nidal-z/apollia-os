/**
 * Open the preview of a `.apo` package, from wherever its path comes from.
 *
 * The preview dialog is mounted once, above every screen. A gesture that
 * yields a package file (a button and the file chooser, a file opened from the
 * system, a file dropped on the window) hands its path here, and the dialog
 * opens on it. The package is installed or updated only from the dialog.
 */
import { writable } from "svelte/store";

/** How the file reached the preview, which its header says. */
export type PackageOrigin = "files" | "update";

/** The package the preview is open on. */
export interface PackagePreviewRequest {
  /** Path of the `.apo` file on this machine. */
  path: string;
  origin: PackageOrigin;
}

/** The open preview, `null` when the dialog is closed. */
export const packagePreviewRequest = writable<PackagePreviewRequest | null>(null);

/** Window event sent once a package is installed or updated. */
export const PACKAGE_INSTALLED_EVENT = "apollia:packages:installed";

/** Opens the preview on the package at `path`. */
export function openPackagePreview(path: string, origin: PackageOrigin = "files"): void {
  packagePreviewRequest.set({ path, origin });
}

/** Closes the preview. */
export function closePackagePreview(): void {
  packagePreviewRequest.set(null);
}
