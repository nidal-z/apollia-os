/**
 * The accelerated engine of the AI screen: what its card shows and what each
 * gesture does, kept out of the components so every state is held by a test
 * that drives the same object with a simulated IPC.
 *
 * A computer is offered the build of the engine the runtime recommends for
 * its card, CUDA, ROCm, Vulkan or another, in one click or from a folder, and
 * later its new versions. Nothing is offered when the recommended build is the
 * one the bundle ships or the processor one. The runtime verifies
 * the signature of the published list before it asks for any archive; this
 * module only proposes, starts, follows and reports. The list of the engines,
 * their versions and the channel they follow stay in the settings.
 */
import { writable, type Readable } from "svelte/store";
import type { LoadedModelsResponse, ModelLoadRequest } from "$lib/types";
import type {
  EngineHardwareCard,
  EngineVariant,
  EngineVariantUpdate,
  EnginesResponse,
} from "$lib/ipc/system";
import type {
  EngineInstallation,
  EngineInstallationKind,
  EngineRefusalCode,
} from "$lib/ipc/engineInstallation";

/** The engine the card is about; its variant is the one the runtime recommends. */
export const ACCELERATED_ENGINE = "llama-server";

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
  | {
      status: "proposal";
      variant: EngineVariant;
      card: string | null;
      sizeBytes: number | null;
    }
  | {
      status: "update_available";
      variant: EngineVariant;
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

/**
 * The variant the card is about: the one the runtime recommends for this
 * machine, unless it is the processor build or the build the bundle ships,
 * which need nothing installed. `null` while the runtime rates nothing.
 */
export function acceleratedVariant(engines: EnginesResponse | null): EngineVariant | null {
  const recommended =
    (engines?.ratings ?? []).find((r) => r.verdict === "recommended")?.variant ?? null;
  if (!engines || recommended === null || recommended === "cpu") return null;
  const shipped = engines.engines.some(
    (e) => e.engine === ACCELERATED_ENGINE && e.source === "embedded" && e.variant === recommended,
  );
  return shipped ? null : recommended;
}

/** Whether a build of `variant` is installed from the channel or a folder. */
export function variantInstalled(engines: EnginesResponse, variant: EngineVariant): boolean {
  return (
    engines.engines.some(
      (e) => e.engine === ACCELERATED_ENGINE && e.source !== "embedded" && e.variant === variant,
    ) ||
    (engines.available ?? []).some(
      (p) => p.engine === ACCELERATED_ENGINE && p.variant === variant && p.installed,
    )
  );
}

/** The recommended variant when it is not installed yet: the one the card proposes. */
export function proposedVariant(engines: EnginesResponse | null): EngineVariant | null {
  const variant = acceleratedVariant(engines);
  return engines && variant !== null && !variantInstalled(engines, variant) ? variant : null;
}

/** The row of the accelerated engine among the updates of the channel. */
export function acceleratedUpdate(engines: EnginesResponse | null): EngineVariantUpdate | null {
  const variant = acceleratedVariant(engines);
  if (variant === null) return null;
  return (
    engines?.updates.find((u) => u.engine === ACCELERATED_ENGINE && u.variant === variant) ?? null
  );
}

/** Bytes installing `variant` downloads, as the verified list states; `null` when unknown. */
function downloadSize(engines: EnginesResponse, variant: EngineVariant): number | null {
  const row = acceleratedUpdate(engines);
  if (row) return row.size;
  return (
    (engines.available ?? []).find((p) => p.engine === ACCELERATED_ENGINE && p.variant === variant)
      ?.size ?? null
  );
}

/** The maker of the card each accelerated variant is built for. */
const VARIANT_VENDOR: Partial<Record<EngineVariant, EngineHardwareCard["vendor"]>> = {
  "cuda-12": "nvidia",
  "cuda-13": "nvidia",
  rocm: "amd",
};

/**
 * The card `variant` uses, by its name: the first card of its maker for CUDA
 * and ROCm, the first card that is not NVIDIA for Vulkan. `null` when none is
 * known.
 */
export function cardFor(engines: EnginesResponse, variant: EngineVariant): string | null {
  const cards = engines.hardware?.cards ?? [];
  const vendor = VARIANT_VENDOR[variant];
  const card =
    vendor !== undefined
      ? cards.find((c) => c.vendor === vendor)
      : (cards.find((c) => c.vendor !== "nvidia") ?? cards[0]);
  return card?.name ?? null;
}

/** The key under which a proposal is put off: one per variant and version offered. */
export function laterKey(engines: EnginesResponse | null): string | null {
  const proposed = proposedVariant(engines);
  if (proposed !== null) return `proposal:${proposed}`;
  const row = acceleratedUpdate(engines);
  return row?.update_available ? `update:${row.variant}:${row.latest_tag}` : null;
}

/**
 * Whether the verified list has to be fetched once to know what to propose:
 * a machine with a card the runtime cannot rate yet, or a proposal whose
 * weight is not known.
 */
function listWanted(engines: EnginesResponse | null): boolean {
  if (!engines?.verifiable) return false;
  const proposed = proposedVariant(engines);
  if (proposed !== null) return downloadSize(engines, proposed) === null;
  return (engines.ratings ?? []).length === 0 && (engines.hardware?.cards.length ?? 0) > 0;
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
 * otherwise the proposal of the recommended variant when it is not installed,
 * or the new version of the installed one, unless put off. Nothing without a
 * card, nothing when the recommended build is the shipped or the processor
 * one, and nothing when the engine is installed and up to date.
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
  const proposed = proposedVariant(engines);
  if (proposed !== null) {
    return {
      status: "proposal",
      variant: proposed,
      card: cardFor(engines, proposed),
      sizeBytes: downloadSize(engines, proposed),
    };
  }
  if (row?.update_available) {
    return {
      status: "update_available",
      variant: row.variant,
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
      if (listWanted(current.engines) && !checked) {
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
      const variant = acceleratedVariant(current.engines);
      if (variant === null) return;
      await send(() => deps.start(ACCELERATED_ENGINE, variant));
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
