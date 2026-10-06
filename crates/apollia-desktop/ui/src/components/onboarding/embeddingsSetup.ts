/**
 * The step that offers the model search by meaning uses: what it shows and
 * what each gesture does.
 *
 * The decisions live here, outside the component, for two reasons. The step
 * may be left while its download runs, and the model still has to become the
 * default when the file lands: the setup object outlives the component. And
 * every outcome (downloaded, imported, offline, passed) is then held by a test
 * that drives the same object with a simulated IPC.
 */
import { writable, type Readable } from "svelte/store";
import type { DownloadProgress } from "$lib/ipc/models";
import type {
  EmbeddingDefault,
  EmbeddingModel,
  EmbeddingReason,
  EmbeddingsRecommendation,
} from "$lib/ipc/embeddings";
import type { VariantDownloadRequest } from "$lib/ipc/modelHub";
import type { MemoryEmbeddings } from "$lib/types";

/** Where the step stands. */
export type EmbeddingsPhase =
  /** The recommendation has not answered yet. */
  | "loading"
  /** A model is offered and nothing has been fetched. */
  | "offer"
  /** Its file is being downloaded. */
  | "downloading"
  /** It is installed and is the default embeddings model. */
  | "ready"
  /** The registry offered nothing, or could not be read. */
  | "unavailable";

/** A failure the step shows; `detail` is the backend's own sentence. */
export interface EmbeddingsFailure {
  kind: "recommendation" | "download" | "import" | "not_offered" | "default";
  detail: string | null;
}

export interface EmbeddingsSetupState {
  phase: EmbeddingsPhase;
  /** The model offered, or the one installed once the step is ready. */
  model: EmbeddingModel | null;
  reason: EmbeddingReason | null;
  progress: DownloadProgress | null;
  /** True while a file picked by the person is being checked and copied. */
  importing: boolean;
  failure: EmbeddingsFailure | null;
}

/** Everything the setup asks of the application, so a test can stand in for it. */
export interface EmbeddingsSetupDeps {
  recommend: (languageModelBytes: number | null) => Promise<EmbeddingsRecommendation>;
  current: () => Promise<MemoryEmbeddings>;
  startDownload: (request: VariantDownloadRequest) => Promise<string>;
  cancelDownload: (downloadId: string) => Promise<void>;
  importFile: (sourcePath: string, allowedExtensions: string[]) => Promise<string>;
  setDefault: (model: string) => Promise<EmbeddingDefault>;
  /** Subscribes to the download progress events; resolves to the unsubscribe. */
  onProgress: (handler: (progress: DownloadProgress) => void) => Promise<() => void>;
}

export interface EmbeddingsSetup extends Readable<EmbeddingsSetupState> {
  /** Reads the recommendation and whether a model is already installed. */
  load: (languageModelBytes: number | null) => Promise<void>;
  /** Downloads the offered model through the verified download. */
  download: () => Promise<void>;
  cancel: () => Promise<void>;
  /** Imports the file at `sourcePath` and makes it the default when it is an offered model. */
  importFrom: (sourcePath: string) => Promise<void>;
  /** Feeds one progress event; public so a test can play a download. */
  handleProgress: (progress: DownloadProgress) => Promise<void>;
}

const INITIAL: EmbeddingsSetupState = {
  phase: "loading",
  model: null,
  reason: null,
  progress: null,
  importing: false,
  failure: null,
};

function detailOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? "";
}

/**
 * The registry entry a file on disk is a copy of, by its file name. `null`
 * for a file the registry does not offer: it is not made the default.
 */
export function entryForFile(
  models: readonly EmbeddingModel[],
  path: string,
): EmbeddingModel | null {
  const name = baseName(path).toLowerCase();
  if (!name) return null;
  return models.find((model) => model.file.toLowerCase() === name) ?? null;
}

/** The request that fetches `model` through the verified variant download. */
export function downloadRequest(model: EmbeddingModel): VariantDownloadRequest {
  return {
    repo_id: model.repo,
    selector: { kind: "gguf", file: model.file },
    hf_token: null,
    dest_dir: null,
  };
}

export function createEmbeddingsSetup(deps: EmbeddingsSetupDeps): EmbeddingsSetup {
  const store = writable<EmbeddingsSetupState>(INITIAL);
  let state = INITIAL;
  let models: EmbeddingModel[] = [];
  let downloadId: string | null = null;
  // The model being fetched: the one made the default when its file lands,
  // even if the recommendation is read again meanwhile.
  let downloading: EmbeddingModel | null = null;
  let listening: Promise<() => void> | null = null;

  function set(patch: Partial<EmbeddingsSetupState>): void {
    state = { ...state, ...patch };
    store.set(state);
  }

  // One subscription for the life of the setup, never released: the download
  // has to be followed after the step that started it is gone.
  function listen(): Promise<() => void> {
    listening ??= deps.onProgress((progress) => void handleProgress(progress));
    return listening;
  }

  async function makeDefault(model: EmbeddingModel): Promise<void> {
    try {
      await deps.setDefault(model.id);
      set({ phase: "ready", model, progress: null, failure: null });
    } catch (err: unknown) {
      set({
        phase: "offer",
        progress: null,
        failure: { kind: "default", detail: detailOf(err) },
      });
    }
  }

  async function load(languageModelBytes: number | null): Promise<void> {
    // A download in flight keeps its state: the step was reopened on it.
    if (state.phase === "downloading") return;
    set({ phase: "loading", failure: null });
    let answer: EmbeddingsRecommendation;
    try {
      answer = await deps.recommend(languageModelBytes);
    } catch (err: unknown) {
      set({
        phase: "unavailable",
        model: null,
        failure: { kind: "recommendation", detail: detailOf(err) },
      });
      return;
    }
    models = answer.models;
    const offered = models.find((model) => model.id === answer.recommended) ?? null;
    let installed: EmbeddingModel | null = null;
    try {
      const current = await deps.current();
      installed = models.find((model) => model.id === current.model) ?? null;
    } catch {
      // Unreadable setting: the step offers the model as on a first launch.
    }
    if (installed) {
      set({ phase: "ready", model: installed, reason: null });
    } else if (offered) {
      set({ phase: "offer", model: offered, reason: answer.reason });
    } else {
      set({ phase: "unavailable", model: null, reason: null });
    }
  }

  async function download(): Promise<void> {
    const model = state.model;
    if (state.phase !== "offer" || !model || state.importing) return;
    set({ phase: "downloading", progress: null, failure: null });
    downloading = model;
    try {
      await listen();
      downloadId = await deps.startDownload(downloadRequest(model));
    } catch (err: unknown) {
      downloading = null;
      downloadId = null;
      set({ phase: "offer", failure: { kind: "download", detail: detailOf(err) } });
    }
  }

  async function cancel(): Promise<void> {
    if (!downloadId) return;
    try {
      await deps.cancelDownload(downloadId);
    } catch {
      // Refused: no such download runs any more, and no event will say so.
      // The model is offered again rather than left on a progress bar.
      downloadId = null;
      downloading = null;
      set({ phase: "offer", progress: null });
    }
  }

  async function handleProgress(progress: DownloadProgress): Promise<void> {
    if (!downloadId || progress.id !== downloadId) return;
    if (progress.status === "in_progress") {
      set({ progress });
      return;
    }
    const model = downloading;
    downloadId = null;
    downloading = null;
    if (progress.status === "completed" && model) {
      await makeDefault(model);
    } else if (progress.status === "failed") {
      set({ phase: "offer", progress: null, failure: { kind: "download", detail: null } });
    } else {
      set({ phase: "offer", progress: null });
    }
  }

  async function importFrom(sourcePath: string): Promise<void> {
    if (state.importing || state.phase === "downloading" || state.phase === "loading") return;
    set({ importing: true, failure: null });
    try {
      const entry = entryForFile(models, sourcePath);
      if (!entry) {
        set({ failure: { kind: "not_offered", detail: baseName(sourcePath) } });
        return;
      }
      try {
        await deps.importFile(sourcePath, ["gguf"]);
      } catch (err: unknown) {
        set({ failure: { kind: "import", detail: detailOf(err) } });
        return;
      }
      await makeDefault(entry);
    } finally {
      set({ importing: false });
    }
  }

  return { subscribe: store.subscribe, load, download, cancel, importFrom, handleProgress };
}

/** What the step renders and allows, from its state and the network. */
export interface EmbeddingsStepView {
  showModelCard: boolean;
  showProgress: boolean;
  showReady: boolean;
  showDownload: boolean;
  downloadDisabled: boolean;
  /** Key of the sentence that says why the download is disabled, or `null`. */
  downloadDisabledReasonKey: string | null;
  showImport: boolean;
  importDisabled: boolean;
  /** The passing warning replaces "Skip" and "Continue" by one explicit button. */
  showSkipWarning: boolean;
  showSkip: boolean;
  showContinue: boolean;
  continueDisabled: boolean;
  showContinueWithout: boolean;
}

export function embeddingsStepView(
  state: EmbeddingsSetupState,
  online: boolean,
  skipAsked: boolean,
): EmbeddingsStepView {
  const offer = state.phase === "offer";
  const settled = state.phase === "ready" || state.phase === "downloading";
  // Passing is only a question while there is nothing installed or on its way.
  const warning = skipAsked && !settled;
  return {
    showModelCard: state.model !== null,
    showProgress: state.phase === "downloading",
    showReady: state.phase === "ready",
    showDownload: offer,
    downloadDisabled: !online || state.importing,
    downloadDisabledReasonKey: offer && !online ? "onboarding.embeddings.offline" : null,
    // The registry has to be known to recognise the file; without it the
    // step can only be passed.
    showImport: offer,
    importDisabled: state.importing,
    showSkipWarning: warning,
    showSkip: !settled && !warning,
    showContinue: !warning,
    continueDisabled: !settled,
    showContinueWithout: warning,
  };
}

/** The functions lost without the model, as the passing warning lists them. */
export const UNAVAILABLE_WITHOUT_EMBEDDINGS = [
  "onboarding.embeddings.skip_lost_sources",
  "onboarding.embeddings.skip_lost_memory",
] as const;

/**
 * Key of the sentence that says why this model was chosen: the small one
 * because the larger had no room beside the language model, the larger one
 * because it has.
 */
export function reasonKey(reason: EmbeddingReason | null): string | null {
  if (!reason) return null;
  return reason.kind === "no_room_beside"
    ? "onboarding.embeddings.reason_light"
    : "onboarding.embeddings.reason_roomy";
}

/** Key of the sentence a failure is shown with. */
export function failureKey(kind: EmbeddingsFailure["kind"]): string {
  switch (kind) {
    case "recommendation":
      return "onboarding.embeddings.error_recommendation";
    case "download":
      return "onboarding.embeddings.error_download";
    case "import":
      return "onboarding.embeddings.error_import";
    case "not_offered":
      return "onboarding.embeddings.error_not_offered";
    case "default":
      return "onboarding.embeddings.error_default";
  }
}

/** The size of the file in megabytes, decimal as the Hub shows it. */
export function sizeInMb(bytes: number): number {
  return Math.round(bytes / 1e6);
}
