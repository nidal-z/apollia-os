// File-reference helpers for the Tauri webview.
//
// Tool bodies render file paths the agent touched (file_read / file_write /
// file_edit / file_list / file_glob / file_grep). A path should be actionable:
// clicking it reveals the file in the OS file manager (Finder, Explorer,
// Nautilus) via the `tauri-plugin-opener` `revealItemInDir` command, paired
// with the `opener:allow-reveal-item-in-dir` capability permission.
//
// Mirrors `externalLink.ts`: an `isTauri()` dev fallback plus swallowed errors
// so a failed reveal never breaks the surrounding render (worst case: nothing
// happens and the user copies the path from the `title`).

import { revealItemInDir } from "@tauri-apps/plugin-opener";

/** True when running inside the Tauri runtime (vs. plain browser dev). */
function isTauri(): boolean {
  return globalThis.window !== undefined && "__TAURI_INTERNALS__" in globalThis.window;
}

/**
 * True when `path` is absolute enough for the OS reveal to resolve it without a
 * working directory: a POSIX root (`/…`), a home-relative path (`~/…`), or a
 * Windows drive (`C:\…` / `C:/…`). Relative paths return `false` so callers
 * render a non-clickable ref instead of a dead click.
 */
export function isRevealablePath(path: string): boolean {
  if (!path) return false;
  return /^(\/|~(\/|$)|[A-Za-z]:[\\/])/.test(path);
}

/**
 * Reveal `path` in the user's OS file manager, selecting the item.
 *
 * In a Tauri context this calls the `opener` plugin (subject to the
 * `opener:allow-reveal-item-in-dir` capability). In a plain browser fallback
 * this is a no-op (logged), since there is no OS file manager to drive.
 *
 * Absolute paths (including `~`-prefixed) are passed through untouched: the OS
 * layer resolves them. `~` is deliberately not expanded here (no home value is
 * readily available client-side); the native reveal handles it.
 *
 * Logs (and swallows) errors so callers don't have to wrap each invocation in
 * try/catch.
 */
export async function revealPath(path: string): Promise<void> {
  if (!path) return;
  try {
    if (isTauri()) {
      await revealItemInDir(path);
    } else {
      console.info("[fileRef] reveal (dev no-op)", path);
    }
  } catch (err) {
    console.error("[fileRef] failed to reveal path", path, err);
  }
}

/** Join a (possibly trailing-slashed) directory with a child name. */
export function joinPath(dir: string, name: string): string {
  if (!dir) return name;
  return dir.endsWith("/") ? `${dir}${name}` : `${dir}/${name}`;
}
