/**
 * The accelerated engine of the AI screen: what its card shows and what each
 * gesture does, kept out of the components so every state is held by a test
 * that drives the same object with a simulated IPC.
 *
 * A computer with an NVIDIA card is offered the engine that uses it, in one
 * click or from a folder, and later its new versions. The runtime verifies
 * the signature of the published list before it asks for any archive; this
 * module only proposes, starts, follows and reports. The list of the engines,
 * their versions and the channel they follow stay in the settings.
 */
import { writable, type Readable } from "svelte/store";
import type { LoadedModelsResponse, ModelLoadRequest } from "$lib/types";
import type { EngineVariantUpdate, EnginesResponse } from "$lib/ipc/system";
import type {
  EngineInstallation,
  EngineInstallationKind,
  EngineRefusalCode,
} from "$lib/ipc/engineInstallation";

/** The engine and the variant the card is about. */
export const ACCELERATED_ENGINE = "llama-server";
export const ACCELERATED_VARIANT = "cuda";

/** How often a running installation is read again, in milliseconds. */
export const POLL_INTERVAL_MS = 500;

/** A build, named by the day it was published when the channel says it. */
export interface EngineVersion {
  tag: string;
  /** `YYYY-MM-DD`, or `null` when unknown. */
  released: string | null;
}

/** Why an installation stopped, as the card words it. */
export type RefusalReason =
  | "list_altered"
  | "unknown_key"
  | "older_version"
  | "signature_missing"
  | "signature_invalid"
  | "archive_altered"
  | "network"
  | "other";

/** The catalogue key of the title of a refusal, by what was being done. */
export const REFUSAL_TITLE_KEYS: Record<EngineInstallationKind, string> = {
  install: "ai.engine.refusal.title.install",
  update: "ai.engine.refusal.title.update",
  import: "ai.engine.refusal.title.import",
};

/** The catalogue key of the cause named in the title of a refusal. */
export const REFUSAL_CAUSE_KEYS: Record<RefusalReason, string> = {
  list_altered: "ai.engine.refusal.cause.list_altered",
  unknown_key: "ai.engine.refusal.cause.unknown_key",
  older_version: "ai.engine.refusal.cause.older_version",
  signature_missing: "ai.engine.refusal.cause.signature_missing",
  signature_invalid: "ai.engine.refusal.cause.signature_invalid",
  archive_altered: "ai.engine.refusal.cause.archive_altered",
  network: "ai.engine.refusal.cause.network",
  other: "ai.engine.refusal.cause.other",
};

/** The catalogue key of the sentence that explains a refusal. */
export const REFUSAL_BODY_KEYS: Record<RefusalReason, string> = {
  list_altered: "ai.engine.refusal.body.list_altered",
  unknown_key: "ai.engine.refusal.body.unknown_key",
  older_version: "ai.engine.refusal.body.older_version",
  signature_missing: "ai.engine.refusal.body.signature_missing",
  signature_invalid: "ai.engine.refusal.body.signature_invalid",
  archive_altered: "ai.engine.refusal.body.archive_altered",
  network: "ai.engine.refusal.body.network",
  other: "ai.engine.refusal.body.other",
};

/** The catalogue key of the sentence under the title of a running installation. */
export const PROGRESS_BODY_KEYS: Record<EngineInstallationKind, string> = {
  install: "ai.engine.progress.install_body",
  update: "ai.engine.progress.update_body",
  import: "ai.engine.progress.import_body",
};

/** What the card shows, or `null` for no card at all. */
export type CardView =
  | { status: "proposal"; card: string | null; sizeBytes: number | null }
  | {
      status: "update_available";
      latest: EngineVersion;
      installed: EngineVersion | null;
      sizeBytes: number;
    }
  | {
      status: "verifying";
      flow: EngineInstallationKind;
      sizeBytes: number | null;
      latest: EngineVersion | null;
    }
  | {
      status: "downloading";
      flow: EngineInstallationKind;
      received: number;
      total: number;
      etaSeconds: number | null;
      latest: EngineVersion | null;
    }
  | { status: "installing"; flow: EngineInstallationKind }
  | { status: "installed"; flow: EngineInstallationKind; loadedCount: number }
  | {
      status: "refused";
      flow: EngineInstallationKind;
      reason: RefusalReason;
      /** True when the refusal came before any archive was asked for. */
      nothingDownloaded: boolean;
      /** The runtime's sentence, for the developer mode. */
      detail: string;
    };

/** What the screen holds about the accelerated engine. */
export interface AcceleratedEngineState {
  engines: EnginesResponse | null;
  installation: EngineInstallation;
  /** Proposals put off until the next version or the next launch. */
  later: string[];
  /** Seconds the download still needs, once a rate is known. */
  etaSeconds: number | null;
  /** Local models in memory, which keep the engine they were loaded with. */
  loadedCount: number;
  /** A gesture is being sent. */
  busy: boolean;
}

/** The row of the accelerated engine among the updates of the channel. */
export function acceleratedUpdate(engines: EnginesResponse | null): EngineVariantUpdate | null {
  return (
    engines?.updates.find(
      (u) => u.engine === ACCELERATED_ENGINE && u.variant === ACCELERATED_VARIANT,
    ) ?? null
  );
}

/** The key under which a proposal is put off: one per version offered. */
export function laterKey(engines: EnginesResponse | null): string | null {
  const row = acceleratedUpdate(engines);
  if (engines?.cuda_proposable) return "proposal";
  return row?.update_available ? `update:${row.latest_tag}` : null;
}

/** How the card words a refusal of `code` during `flow`. */
export function refusalReason(
  flow: EngineInstallationKind,
  code: EngineRefusalCode,
): RefusalReason {
  switch (code) {
    case "signature_missing":
      return "signature_missing";
    case "signature_invalid":
      return flow === "import" ? "signature_invalid" : "list_altered";
    case "signature_unknown_key":
      return flow === "import" ? "signature_invalid" : "unknown_key";
    case "rollback":
      return "older_version";
    case "digest_mismatch":
    case "archive_invalid":
      return "archive_altered";
    case "network":
      return "network";
    default:
      return "other";
  }
}

function latestVersion(row: EngineVariantUpdate | null): EngineVersion | null {
  return row ? { tag: row.latest_tag, released: row.latest_released } : null;
}

/**
 * What the card shows. An installation that runs or just ended comes first;
 * otherwise the proposal on a computer with an NVIDIA card and without the
 * engine, or the new version of an installed engine, unless put off. Nothing
 * without a card, and nothing when the engine is installed and up to date.
 */
export function cardView(state: AcceleratedEngineState): CardView | null {
  const { engines, installation } = state;
  const row = acceleratedUpdate(engines);
  switch (installation.state) {
    case "verifying":
      return {
        status: "verifying",
        flow: installation.kind,
        sizeBytes: installation.kind === "import" ? null : (row?.size ?? null),
        latest: latestVersion(row),
      };
    case "downloading":
      return {
        status: "downloading",
        flow: installation.kind,
        received: installation.received,
        total: installation.total,
        etaSeconds: state.etaSeconds,
        latest: latestVersion(row),
      };
    case "installing":
      return { status: "installing", flow: installation.kind };
    case "installed":
      return { status: "installed", flow: installation.kind, loadedCount: state.loadedCount };
    case "failed":
      return {
        status: "refused",
        flow: installation.kind,
        reason: refusalReason(installation.kind, installation.code),
        nothingDownloaded: !installation.archive_requested,
        detail: installation.error,
      };
    case "idle":
      break;
  }
  if (!engines) return null;
  const key = laterKey(engines);
  if (key === null || state.later.includes(key)) return null;
  if (engines.cuda_proposable) {
    return { status: "proposal", card: engines.nvidia_card, sizeBytes: row?.size ?? null };
  }
  if (row?.update_available) {
    return {
      status: "update_available",
      latest: { tag: row.latest_tag, released: row.latest_released },
      installed: row.installed_tag
        ? { tag: row.installed_tag, released: row.installed_released }
        : null,
      sizeBytes: row.size,
    };
  }
  return null;
}

/** True while the runtime is working on the installation. */
export function isRunning(installation: EngineInstallation): boolean {
  return (
    installation.state === "verifying" ||
    installation.state === "downloading" ||
    installation.state === "installing"
  );
}

/**
 * Seconds a download still needs at the rate measured since `sinceMs`, or
 * `null` until a second has passed and something has arrived.
 */
export function remainingSeconds(
  start: { atMs: number; received: number },
  now: { atMs: number; received: number },
  total: number,
): number | null {
  const elapsed = (now.atMs - start.atMs) / 1000;
  const arrived = now.received - start.received;
  if (elapsed < 1 || arrived <= 0) return null;
  return Math.max(0, Math.round((total - now.received) / (arrived / elapsed)));
}

/** A size in megabytes, at least one, as the card writes it. */
export function megabytes(bytes: number, locale: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(
    Math.max(1, Math.round(bytes / 1e6)),
  );
}

/** The day `YYYY-MM-DD` written out in `locale`, or `null` when it is not a day. */
export function releaseDay(released: string | null, locale: string): string | null {
  const match = released?.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return null;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(date);
}

/** What reloading the models in memory did. */
export interface ReloadOutcome {
  reloaded: number;
  /** Models left alone because they were answering. */
  busy: number;
}

/** The calls the accelerated engine makes, injected so a test can simulate them. */
export interface AcceleratedEngineDeps {
  listEngines: () => Promise<EnginesResponse>;
  /** Fetch and verify the list of the followed channel. */
  checkUpdates: () => Promise<unknown>;
  installation: () => Promise<EngineInstallation>;
  start: (engine: string, variant: string) => Promise<EngineInstallation>;
  importFolder: (path: string) => Promise<EngineInstallation>;
  dismiss: () => Promise<EngineInstallation>;
  /** The folder the user chose, or `null` when the picker was closed. */
  pickFolder: () => Promise<string | null>;
  loaded: () => Promise<LoadedModelsResponse>;
  unload: (modelPath: string) => Promise<unknown>;
  load: (request: ModelLoadRequest) => Promise<unknown>;
  /** Resolves after `ms` milliseconds. */
  wait: (ms: number) => Promise<void>;
  now: () => number;
  /** Reports a gesture that could not be sent. */
  onError: (err: unknown) => void;
}

export interface AcceleratedEngine extends Readable<AcceleratedEngineState> {
  /** Read the engines and the installation; follow one that is running. */
  refresh: () => Promise<void>;
  /** Start the verified download of the accelerated engine or of its new version. */
  accelerate: () => Promise<void>;
  /** Choose a folder and import the engine from it. */
  importFromFolder: () => Promise<void>;
  /** Cancel the download. */
  cancel: () => Promise<void>;
  /** Close the card of a refusal. */
  close: () => Promise<void>;
  /** Put the proposal off until the next version or the next launch. */
  later: () => void;
  /** Reload the local models in memory so they take the new engine. */
  reloadModels: () => Promise<ReloadOutcome | null>;
  /** The screen is left: an installed card is not shown again. */
  leave: () => Promise<void>;
}

const IDLE: EngineInstallation = { state: "idle" };

export function createAcceleratedEngine(deps: AcceleratedEngineDeps): AcceleratedEngine {
  let current: AcceleratedEngineState = {
    engines: null,
    installation: IDLE,
    later: [],
    etaSeconds: null,
    loadedCount: 0,
    busy: false,
  };
  const { subscribe, set } = writable(current);
  /** The first sample of the running download, for its rate. */
  let rateStart: { atMs: number; received: number } | null = null;
  let following = false;
  let checked = false;

  function patch(part: Partial<AcceleratedEngineState>): void {
    current = { ...current, ...part };
    set(current);
  }

  function take(installation: EngineInstallation): void {
    let etaSeconds: number | null = null;
    if (installation.state === "downloading") {
      const sample = { atMs: deps.now(), received: installation.received };
      rateStart ??= sample;
      etaSeconds = remainingSeconds(rateStart, sample, installation.total);
    } else {
      rateStart = null;
    }
    patch({ installation, etaSeconds });
  }

  async function readEngines(): Promise<void> {
    try {
      patch({ engines: await deps.listEngines() });
    } catch {
      // Without the list there is nothing to propose; the screen says so
      // nowhere, since the card is an offer and not a state of the screen.
    }
  }

  async function countLoaded(): Promise<void> {
    try {
      const { models } = await deps.loaded();
      patch({ loadedCount: models.filter((m) => m.engine === ACCELERATED_ENGINE).length });
    } catch {
      patch({ loadedCount: 0 });
    }
  }

  /** Read the installation until it stops running, then read what it changed. */
  async function follow(): Promise<void> {
    if (following) return;
    following = true;
    try {
      while (isRunning(current.installation)) {
        await deps.wait(POLL_INTERVAL_MS);
        take(await deps.installation());
      }
      await Promise.all([readEngines(), countLoaded()]);
    } catch (err: unknown) {
      deps.onError(err);
    } finally {
      following = false;
    }
  }

  async function send(call: () => Promise<EngineInstallation>): Promise<void> {
    if (current.busy) return;
    patch({ busy: true });
    try {
      take(await call());
    } catch (err: unknown) {
      deps.onError(err);
    } finally {
      patch({ busy: false });
    }
  }

  return {
    subscribe,

    async refresh() {
      await readEngines();
      // The weight of the download is read in the verified list: a computer
      // that never fetched it asks for it once.
      if (current.engines?.cuda_proposable && !acceleratedUpdate(current.engines) && !checked) {
        checked = true;
        try {
          await deps.checkUpdates();
          await readEngines();
        } catch {
          // Offline, or no list published: the proposal stands without a weight.
        }
      }
      try {
        take(await deps.installation());
      } catch {
        take(IDLE);
      }
      if (isRunning(current.installation)) void follow();
      else if (current.installation.state === "installed") await countLoaded();
    },

    async accelerate() {
      await send(() => deps.start(ACCELERATED_ENGINE, ACCELERATED_VARIANT));
      await follow();
    },

    async importFromFolder() {
      let folder: string | null;
      try {
        folder = await deps.pickFolder();
      } catch (err: unknown) {
        deps.onError(err);
        return;
      }
      if (folder === null) return;
      const path = folder;
      await send(() => deps.importFolder(path));
      await follow();
    },

    async cancel() {
      await send(deps.dismiss);
    },

    async close() {
      await send(deps.dismiss);
    },

    later() {
      const key = laterKey(current.engines);
      if (key !== null && !current.later.includes(key)) {
        patch({ later: [...current.later, key] });
      }
    },

    async reloadModels() {
      if (current.busy) return null;
      patch({ busy: true });
      const outcome: ReloadOutcome = { reloaded: 0, busy: 0 };
      try {
        const { models } = await deps.loaded();
        for (const model of models) {
          if (model.engine !== ACCELERATED_ENGINE) continue;
          if (model.activeRequests > 0) {
            outcome.busy += 1;
            continue;
          }
          await deps.unload(model.modelPath);
          // A model the index holds is loaded again at once; a file it does
          // not hold is loaded by the next request that names it.
          if (model.kind === "chat" && model.modelId) {
            await deps.load({ modelId: model.modelId, nCtx: model.nCtx, confirmed: true });
          }
          outcome.reloaded += 1;
        }
        if (outcome.busy === 0) take(await deps.dismiss());
        return outcome;
      } catch (err: unknown) {
        deps.onError(err);
        return null;
      } finally {
        patch({ busy: false });
        await countLoaded();
      }
    },

    async leave() {
      if (current.installation.state !== "installed") return;
      try {
        take(await deps.dismiss());
      } catch {
        // The card comes back at the next visit; nothing else is lost.
      }
    },
  };
}
