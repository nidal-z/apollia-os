/**
 * Downloads panel store.
 *
 * One list of every download the window has seen since it started, fed by the
 * single `model-download-progress` event (`listenDownloadProgress`). The
 * models, the dictation models and the projector files all report on that
 * event, so the panel reads one source and never a second one.
 *
 * The preference "open the panel when a download starts" is a display
 * preference, persisted in `localStorage` like the theme. It is off by default.
 */
import { derived, get, writable, type Readable } from "svelte/store";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  cancelModelDownload,
  listenDownloadProgress,
  type DownloadProgress,
  type DownloadStatus,
} from "$lib/ipc/modelHub";

/** One row of the panel. */
export interface DownloadEntry {
  id: string;
  name: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bps: number;
  status: DownloadStatus;
  /** The runtime's own words for a failure, `null` otherwise. */
  error: string | null;
}

/** The rows of the panel, split in its three groups. */
export interface DownloadGroups {
  active: DownloadEntry[];
  /** Completed and cancelled downloads, newest first. */
  finished: DownloadEntry[];
  failed: DownloadEntry[];
}

/** Why a download failed, as a key of `topbar.downloads.reason.*`. */
export type FailureReason =
  | "network"
  | "disk"
  | "access"
  | "not_found"
  | "server"
  | "integrity"
  | "unknown";

export const OPEN_ON_START_KEY = "apollia-downloads-open-on-start";

/** The file name shown for a download: the last segment of its path. */
export function downloadName(path: string): string {
  const segments = path.split(/[\\/]/).filter((s) => s.length > 0);
  return segments.at(-1) ?? path;
}

/**
 * Folds one progress event into the list. Returns the new list, and whether
 * this event is the first one of a download (the moment the panel may open).
 * The newest download comes first; an event keeps its row in place.
 */
export function applyProgress(
  entries: DownloadEntry[],
  progress: DownloadProgress,
): { entries: DownloadEntry[]; started: boolean } {
  const next: DownloadEntry = {
    id: progress.id,
    name: downloadName(progress.dest_path),
    downloaded_bytes: progress.downloaded_bytes,
    total_bytes: progress.total_bytes,
    speed_bps: progress.status === "in_progress" ? progress.speed_bps : 0,
    status: progress.status,
    error: progress.status === "failed" ? (progress.error ?? null) : null,
  };
  const index = entries.findIndex((e) => e.id === progress.id);
  if (index === -1) {
    return { entries: [next, ...entries], started: progress.status === "in_progress" };
  }
  const copy = entries.slice();
  copy[index] = next;
  return { entries: copy, started: false };
}

/** Splits the list into the panel's groups, keeping the list's order. */
export function groupDownloads(entries: DownloadEntry[]): DownloadGroups {
  return {
    active: entries.filter((e) => e.status === "in_progress"),
    finished: entries.filter((e) => e.status === "completed" || e.status === "cancelled"),
    failed: entries.filter((e) => e.status === "failed"),
  };
}

/** Percentage 0-100, `undefined` while the total size is still unknown. */
export function downloadPercent(entry: DownloadEntry): number | undefined {
  if (!entry.total_bytes) return undefined;
  return Math.min(100, Math.round((entry.downloaded_bytes / entry.total_bytes) * 100));
}

const GB = 1_073_741_824;
const MB = 1_048_576;
const KB = 1_024;

/** A size in the unit the panel reads: GB, MB, KB or bytes. */
export function formatBytes(bytes: number): string {
  if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`;
  if (bytes >= MB) return `${Math.round(bytes / MB)} MB`;
  if (bytes >= KB) return `${Math.round(bytes / KB)} KB`;
  return `${bytes} B`;
}

/** A speed per second, in the same units. */
export function formatSpeed(bps: number): string {
  return `${formatBytes(Math.max(0, Math.round(bps)))}/s`;
}

/**
 * The readable reason of a failure, read from the runtime's error text
 * (`apollia_llm::DownloadError`). The text itself stays available as detail.
 */
export function failureReason(error: string | null): { reason: FailureReason; status?: number } {
  if (!error) return { reason: "unknown" };
  const status = /answered HTTP (\d{3})/.exec(error);
  if (status) {
    const code = Number(status[1]);
    if (code === 401 || code === 403) return { reason: "access", status: code };
    if (code === 404) return { reason: "not_found", status: code };
    return { reason: "server", status: code };
  }
  if (/integrity check failed|size check failed/.test(error)) return { reason: "integrity" };
  if (/^http error/.test(error)) return { reason: "network" };
  if (/^io error/.test(error)) return { reason: "disk" };
  return { reason: "unknown" };
}

function readOpenOnStart(): boolean {
  try {
    return localStorage.getItem(OPEN_ON_START_KEY) === "true";
  } catch {
    return false;
  }
}

/** "Open the downloads panel when a download starts", off by default. */
export const openDownloadsOnStart = writable<boolean>(readOpenOnStart());

/** Persists the preference; the store follows. */
export function setOpenDownloadsOnStart(value: boolean): void {
  openDownloadsOnStart.set(value);
  try {
    localStorage.setItem(OPEN_ON_START_KEY, value ? "true" : "false");
  } catch {
    // localStorage unavailable: the choice holds for this session only.
  }
}

/** Whether the panel under the top bar icon is open. */
export const downloadsPanelOpen = writable<boolean>(false);

const entriesStore = writable<DownloadEntry[]>([]);

/** Every download seen since the window started, newest first. */
export const downloads: Readable<DownloadEntry[]> = { subscribe: entriesStore.subscribe };

/** The panel's three groups. */
export const downloadGroups: Readable<DownloadGroups> = derived(entriesStore, groupDownloads);

/** How many downloads are in progress: the counter on the top bar icon. */
export const activeDownloadCount: Readable<number> = derived(
  entriesStore,
  (list) => list.filter((e) => e.status === "in_progress").length,
);

/** Applies one progress event to the store, opening the panel if asked. */
export function receiveProgress(progress: DownloadProgress): void {
  const { entries, started } = applyProgress(get(entriesStore), progress);
  entriesStore.set(entries);
  if (started && get(openDownloadsOnStart)) downloadsPanelOpen.set(true);
}

/** Empties the list. Tests only need it; the window keeps its list for its life. */
export function resetDownloads(): void {
  entriesStore.set([]);
  downloadsPanelOpen.set(false);
}

/**
 * Subscribes the store to the progress event. Returns the unlisten handle,
 * to call on cleanup.
 */
export function listenDownloads(
  listen: (handler: (p: DownloadProgress) => void) => Promise<UnlistenFn> = listenDownloadProgress,
): Promise<UnlistenFn> {
  return listen(receiveProgress);
}

/** Asks the download manager of this window to cancel one download. */
export function cancelDownload(
  id: string,
  cancel: (id: string) => Promise<void> = cancelModelDownload,
): Promise<void> {
  return cancel(id);
}
