/**
 * How onboarding step 3 renders sizes, speeds and destinations.
 *
 * Presentation only: no state, no IPC beyond resolving the models directory,
 * which is a path computation the two sections both need.
 */
import { homeDir, join as pathJoin } from "@tauri-apps/api/path";
import type { DownloadProgress, HfFile } from "$lib/ipc/models";

export function ramLabel(gb: number): string {
  return gb >= 1 ? `${Math.round(gb)} GB` : `${Math.round(gb * 1024)} MB`;
}

export function osLabel(os: string): string {
  if (os === "macos") return "macOS";
  if (os === "linux") return "Linux";
  if (os === "windows") return "Windows";
  return os;
}

export function dlBytes(p: DownloadProgress): string {
  const dl = (p.downloaded_bytes / 1e9).toFixed(2);
  if (!p.total_bytes) return `${dl} GB`;
  const total = (p.total_bytes / 1e9).toFixed(2);
  const pct = Math.round((p.downloaded_bytes / p.total_bytes) * 100);
  return `${dl} / ${total} GB · ${pct}%`;
}

export function dlSpeed(bps: number): string {
  return bps >= 1e6 ? `${(bps / 1e6).toFixed(1)} MB/s` : `${Math.round(bps / 1000)} KB/s`;
}

export function dlPct(p: DownloadProgress): number {
  if (!p.total_bytes) return 0;
  return Math.min(100, Math.round((p.downloaded_bytes / p.total_bytes) * 100));
}

export function hfFileLabelKey(f: HfFile): string | null {
  if (f.compatibility === "fits") return "onboarding.ai_setup.compat_fits";
  if (f.compatibility === "might_fit") return "onboarding.ai_setup.compat_might_fit";
  if (f.compatibility === "too_large") return "onboarding.ai_setup.compat_too_large";
  return null;
}

/** Where the file picker opens, and where an import lands. */
export function pickModelsDir(): Promise<string> {
  return homeDir().then((home) => pathJoin(home, ".apollia", "models"));
}
