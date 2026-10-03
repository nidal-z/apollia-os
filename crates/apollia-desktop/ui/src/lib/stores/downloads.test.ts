import { describe, test, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DownloadProgress } from "$lib/ipc/modelHub";
import {
  OPEN_ON_START_KEY,
  activeDownloadCount,
  applyProgress,
  cancelDownload,
  downloadGroups,
  downloadPercent,
  downloadsPanelOpen,
  failureReason,
  listenDownloads,
  openDownloadsOnStart,
  resetDownloads,
  setOpenDownloadsOnStart,
} from "./downloads";

/** An in-memory `localStorage`, since the Vitest environment is node. */
function installStorage(): Map<string, string> {
  const map = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
  });
  return map;
}

function progress(over: Partial<DownloadProgress> & { id: string }): DownloadProgress {
  return {
    downloaded_bytes: 0,
    total_bytes: 1_000,
    speed_bps: 0,
    dest_path: `/home/u/.apollia/models/${over.id}.gguf`,
    status: "in_progress",
    ...over,
  };
}

/**
 * Subscribes the store through the mocked Tauri event API and returns the
 * function that emits one `model-download-progress` event.
 */
async function simulatedEvent(): Promise<(p: DownloadProgress) => void> {
  let handler: ((event: { payload: DownloadProgress }) => void) | null = null;
  vi.mocked(listen).mockImplementationOnce(async (name, cb) => {
    expect(name).toBe("model-download-progress");
    handler = cb as typeof handler;
    return () => {};
  });
  await listenDownloads();
  return (p) => handler?.({ payload: p });
}

beforeEach(() => {
  installStorage();
  resetDownloads();
  openDownloadsOnStart.set(false);
  vi.mocked(invoke).mockClear();
});

describe("downloads panel, three states over the simulated IPC", () => {
  test("a download in progress is listed with its name, progress and speed, and counted", async () => {
    // GIVEN the panel listening to the progress event
    const emit = await simulatedEvent();

    // WHEN a download reports a quarter done at 2 MB/s
    emit(progress({ id: "a", downloaded_bytes: 250, speed_bps: 2_000_000 }));

    // THEN it is in the in-progress group, with its file name, and the icon counts it
    const groups = get(downloadGroups);
    expect(groups.active).toHaveLength(1);
    expect(groups.active[0].name).toBe("a.gguf");
    expect(groups.active[0].speed_bps).toBe(2_000_000);
    expect(downloadPercent(groups.active[0])).toBe(25);
    expect(groups.finished).toHaveLength(0);
    expect(groups.failed).toHaveLength(0);
    expect(get(activeDownloadCount)).toBe(1);
  });

  test("a completed download moves to the finished group and leaves the counter", async () => {
    // GIVEN a download in progress
    const emit = await simulatedEvent();
    emit(progress({ id: "a", downloaded_bytes: 500 }));

    // WHEN it reports completion
    emit(progress({ id: "a", downloaded_bytes: 1_000, status: "completed" }));

    // THEN it is finished, once, and nothing is counted any more
    const groups = get(downloadGroups);
    expect(groups.active).toHaveLength(0);
    expect(groups.finished.map((e) => e.status)).toEqual(["completed"]);
    expect(get(activeDownloadCount)).toBe(0);
  });

  test("a failed download carries a readable reason", async () => {
    // GIVEN a download in progress
    const emit = await simulatedEvent();
    emit(progress({ id: "a" }));

    // WHEN the runtime reports a refused access
    emit(
      progress({
        id: "a",
        status: "failed",
        error: "the server answered HTTP 401 for https://huggingface.co/x/resolve/main/a.gguf",
      }),
    );

    // THEN it is in the failed group, and its reason reads as an access refusal
    const groups = get(downloadGroups);
    expect(groups.failed).toHaveLength(1);
    expect(failureReason(groups.failed[0].error)).toEqual({ reason: "access", status: 401 });
  });

  test("Cancel calls cancel_model_download, and the cancellation is shown as finished", async () => {
    // GIVEN a download in progress
    const emit = await simulatedEvent();
    emit(progress({ id: "a" }));

    // WHEN Cancel is pressed and the runtime confirms
    await cancelDownload("a");
    emit(progress({ id: "a", status: "cancelled" }));

    // THEN the command was invoked with the id, and the row says cancelled
    expect(invoke).toHaveBeenCalledWith("cancel_model_download", { downloadId: "a" });
    expect(get(downloadGroups).finished.map((e) => e.status)).toEqual(["cancelled"]);
  });
});

describe("failure reasons", () => {
  test("each runtime error family maps to its reason", () => {
    // GIVEN the error texts of apollia_llm::DownloadError
    const cases: [string | null, string][] = [
      ["http error: connection reset", "network"],
      ["io error: No space left on device", "disk"],
      ["the server answered HTTP 404 for u", "not_found"],
      ["the server answered HTTP 503 for u", "server"],
      ["integrity check failed for 'a': expected sha256 x, got y; the file was deleted", "integrity"],
      ["size check failed for 'a': expected 1 bytes, got 2; the file was deleted", "integrity"],
      ["something else", "unknown"],
      [null, "unknown"],
    ];

    // WHEN each is read
    const reasons = cases.map(([error]) => failureReason(error).reason);

    // THEN each lands on its family
    expect(reasons).toEqual(cases.map(([, reason]) => reason));
  });

  test("a second event of a download keeps its row in place", () => {
    // GIVEN two downloads, the newest first
    let entries = applyProgress([], progress({ id: "a" })).entries;
    entries = applyProgress(entries, progress({ id: "b" })).entries;

    // WHEN the older one progresses
    const result = applyProgress(entries, progress({ id: "a", downloaded_bytes: 10 }));

    // THEN the order holds and it is not a new start
    expect(result.entries.map((e) => e.id)).toEqual(["b", "a"]);
    expect(result.started).toBe(false);
  });
});

describe("open the panel when a download starts", () => {
  test("off, the default: a starting download leaves the panel closed", async () => {
    // GIVEN nothing stored and the panel listening
    expect(localStorage.getItem(OPEN_ON_START_KEY)).toBeNull();
    const emit = await simulatedEvent();

    // WHEN a download starts
    emit(progress({ id: "a" }));

    // THEN the panel stays closed
    expect(get(openDownloadsOnStart)).toBe(false);
    expect(get(downloadsPanelOpen)).toBe(false);
  });

  test("on: a starting download opens the panel, and the choice is persisted", async () => {
    // GIVEN the preference turned on
    setOpenDownloadsOnStart(true);
    const emit = await simulatedEvent();

    // WHEN a download starts
    emit(progress({ id: "a" }));

    // THEN the panel opens and the choice is stored
    expect(get(downloadsPanelOpen)).toBe(true);
    expect(localStorage.getItem(OPEN_ON_START_KEY)).toBe("true");
  });

  test("on: a later event of the same download does not reopen a closed panel", async () => {
    // GIVEN the preference on and a download that opened the panel
    setOpenDownloadsOnStart(true);
    const emit = await simulatedEvent();
    emit(progress({ id: "a" }));

    // WHEN the user closes the panel and the download progresses
    downloadsPanelOpen.set(false);
    emit(progress({ id: "a", downloaded_bytes: 500 }));

    // THEN the panel stays closed
    expect(get(downloadsPanelOpen)).toBe(false);
  });
});
