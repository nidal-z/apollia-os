/**
 * The download of a language model started at the first launch, followed
 * after the step that started it is left.
 *
 * "Continue" is offered as soon as the download is launched, so the steps that
 * follow are done while the file arrives. The step that started it is unmounted
 * by then: the object that follows the download, and wires the model as the
 * default engine when the file lands, lives here, outside any component. The
 * header of the modal reads the same object to keep the progress in sight.
 */
import { writable, type Readable } from "svelte/store";
import type { DownloadProgress, GgufModelInfo } from "$lib/ipc/models";

/** Where the download stands. */
export type LlmDownloadPhase =
  /** Nothing was asked for, or the last download was cancelled. */
  | "idle"
  /** The file is being fetched. */
  | "downloading"
  /** The file landed and is being wired as the default engine. */
  | "installing"
  /** The file landed, and is the default engine unless another one was wired meanwhile. */
  | "ready"
  /** The download did not start, stopped, or its file could not be wired. */
  | "failed";

/** A failure the step shows; `detail` is the backend's own sentence. */
export interface LlmDownloadFailure {
  kind: "start" | "download" | "install";
  detail: string | null;
}

export interface LlmDownloadState {
  phase: LlmDownloadPhase;
  /** Name of the file asked for, `null` while nothing was. */
  filename: string | null;
  /** Its size as the catalogue announced it, then as found on disk. */
  sizeBytes: number | null;
  progress: DownloadProgress | null;
  /** Path of the file this object wired as the default engine. */
  wiredPath: string | null;
  failure: LlmDownloadFailure | null;
}

/** Everything the download asks of the application, so a test can stand in for it. */
export interface LlmDownloadDeps {
  cancelDownload: (downloadId: string) => Promise<void>;
  /** Lists the model files on disk. */
  scan: () => Promise<GgufModelInfo[]>;
  /** Makes the file at `path` the default engine and loads it. */
  wire: (path: string) => Promise<void>;
  /** Subscribes to the download progress events; resolves to the unsubscribe. */
  onProgress: (handler: (progress: DownloadProgress) => void) => Promise<() => void>;
}

export interface LlmDownload extends Readable<LlmDownloadState> {
  /**
   * Follows the download `launch` starts. `launch` resolves to the identifier
   * the progress events carry.
   */
  start: (launch: () => Promise<string>, filename: string, sizeBytes: number | null) => Promise<void>;
  cancel: () => Promise<void>;
  /**
   * Says an engine was wired by hand meanwhile: the file that lands is kept on
   * disk and does not replace that choice.
   */
  keepCurrentEngine: () => void;
  /** Feeds one progress event; public so a test can play a download. */
  handleProgress: (progress: DownloadProgress) => Promise<void>;
}

const INITIAL: LlmDownloadState = {
  phase: "idle",
  filename: null,
  sizeBytes: null,
  progress: null,
  wiredPath: null,
  failure: null,
};

function detailOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Whether a download is on its way: fetched, or landed and being wired. */
export function isInFlight(state: LlmDownloadState): boolean {
  return state.phase === "downloading" || state.phase === "installing";
}

/** Percentage 0-100 of the download, `null` while the total size is unknown. */
export function downloadPercent(state: LlmDownloadState): number | null {
  const progress = state.progress;
  if (!progress?.total_bytes) return null;
  return Math.min(100, Math.round((progress.downloaded_bytes / progress.total_bytes) * 100));
}

/** What the header of the modal says about the download, or `null` for nothing. */
export interface LlmDownloadNotice {
  key: string;
  values?: Record<string, number>;
  tone: "progress" | "ready" | "failed";
}

export function downloadNotice(state: LlmDownloadState): LlmDownloadNotice | null {
  switch (state.phase) {
    case "idle":
      return null;
    case "downloading": {
      const percent = downloadPercent(state);
      return percent === null
        ? { key: "onboarding_modal.model_downloading", tone: "progress" }
        : { key: "onboarding_modal.model_percent", values: { percent }, tone: "progress" };
    }
    case "installing":
      return { key: "onboarding_modal.model_installing", tone: "progress" };
    case "ready":
      return { key: "onboarding_modal.model_ready", tone: "ready" };
    case "failed":
      return { key: "onboarding_modal.model_failed", tone: "failed" };
  }
}

export function createLlmDownload(deps: LlmDownloadDeps): LlmDownload {
  const store = writable<LlmDownloadState>(INITIAL);
  let state = INITIAL;
  let downloadId: string | null = null;
  let keepEngine = false;
  let listening: Promise<() => void> | null = null;

  function set(patch: Partial<LlmDownloadState>): void {
    state = { ...state, ...patch };
    store.set(state);
  }

  // One subscription for the life of the object, never released: the download
  // has to be followed after the step that started it is gone.
  function listen(): Promise<() => void> {
    listening ??= deps.onProgress((progress) => void handleProgress(progress));
    return listening;
  }

  async function start(
    launch: () => Promise<string>,
    filename: string,
    sizeBytes: number | null,
  ): Promise<void> {
    if (isInFlight(state)) return;
    keepEngine = false;
    set({ ...INITIAL, phase: "downloading", filename, sizeBytes });
    try {
      await listen();
      downloadId = await launch();
    } catch (err: unknown) {
      downloadId = null;
      set({ phase: "failed", failure: { kind: "start", detail: detailOf(err) } });
    }
  }

  async function cancel(): Promise<void> {
    if (!downloadId) return;
    try {
      await deps.cancelDownload(downloadId);
    } catch {
      // Refused: no such download runs any more, and no event will say so.
      downloadId = null;
      set({ ...INITIAL });
    }
  }

  async function install(filename: string): Promise<void> {
    set({ phase: "installing", progress: null });
    try {
      const model = (await deps.scan()).find((entry) => entry.filename === filename);
      if (!model) {
        set({ phase: "failed", failure: { kind: "install", detail: null } });
        return;
      }
      if (keepEngine) {
        set({ phase: "ready", sizeBytes: model.size_bytes });
        return;
      }
      await deps.wire(model.path);
      set({ phase: "ready", sizeBytes: model.size_bytes, wiredPath: model.path });
    } catch (err: unknown) {
      set({ phase: "failed", failure: { kind: "install", detail: detailOf(err) } });
    }
  }

  async function handleProgress(progress: DownloadProgress): Promise<void> {
    if (!downloadId || progress.id !== downloadId) return;
    if (progress.status === "in_progress") {
      set({ progress });
      return;
    }
    downloadId = null;
    if (progress.status === "completed" && state.filename) {
      await install(state.filename);
    } else if (progress.status === "failed") {
      set({ phase: "failed", progress: null, failure: { kind: "download", detail: null } });
    } else {
      set({ ...INITIAL });
    }
  }

  return {
    subscribe: store.subscribe,
    start,
    cancel,
    keepCurrentEngine: () => {
      keepEngine = true;
    },
    handleProgress,
  };
}
