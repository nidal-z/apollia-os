// What the Engines page does with a variant: install it, import engines from a
// folder, remove an installed build. Kept out of the components so every
// state is held by a test that drives the same object with a simulated IPC.
//
// The runtime verifies, downloads and removes; this module starts, follows,
// asks for the confirmations and says what the runtime answered. A variant
// the runtime rates incompatible is installed only once the person confirmed
// with the reason in front of them; a build the runtime refuses to remove
// says why, and the build `engines.gguf` names says what the choice becomes.

import { writable, type Readable } from "svelte/store";
import type { EngineVariant } from "$lib/ipc/system";
import type {
  EngineInstallation,
  EngineRemoval,
  EngineRemovalCode,
} from "$lib/ipc/engineInstallation";
import type { VariantRow } from "$lib/settings/engineVariants";

/** The engine whose variants the page installs. */
export const INSTALLED_ENGINE = "llama-server";

/** How often a running installation is read again, in milliseconds. */
export const POLL_INTERVAL_MS = 500;

/** What the installation the page started is about. */
export type InstallTarget = { kind: "variant"; variant: EngineVariant } | { kind: "import" };

/** One installed build the page may remove. */
export interface RemovalTarget {
  engine: string;
  tag: string;
  variant: string;
}

/** Where the confirmation of a removal stands. */
export type RemovalStep =
  /** Asked once, nothing sent yet. */
  | { phase: "ask" }
  /** The build is the one `engines.gguf` names: `after` is what the choice becomes. */
  | { phase: "chosen"; after: string }
  /** The runtime kept the build; `code` says why, `detail` is its sentence. */
  | { phase: "refused"; code: EngineRemovalCode | "unreachable"; detail: string };

export interface EngineManagementState {
  installation: EngineInstallation;
  /** What the page started, `null` before any gesture of this window. */
  target: InstallTarget | null;
  /** The incompatible variant waiting for a confirmation. */
  pendingInstall: VariantRow | null;
  removal: { target: RemovalTarget; step: RemovalStep } | null;
  /** A gesture is being sent. */
  busy: boolean;
}

/** The calls the page makes, injected so a test can simulate them. */
export interface EngineManagementDeps {
  installation: () => Promise<EngineInstallation>;
  start: (engine: string, variant: string) => Promise<EngineInstallation>;
  importFolder: (path: string) => Promise<EngineInstallation>;
  /** The folder the user chose, or `null` when the picker was closed. */
  pickFolder: () => Promise<string | null>;
  remove: (engine: string, tag: string, variant: string, confirm: boolean) => Promise<EngineRemoval>;
  /** Read the engine list again, once something changed. */
  reloadEngines: () => Promise<unknown>;
  wait: (ms: number) => Promise<void>;
  /** Reports a gesture that could not be sent. */
  onError: (err: unknown) => void;
}

export interface EngineManagement extends Readable<EngineManagementState> {
  /** Read the installation; follow one that is running. */
  refresh: () => Promise<void>;
  /** Install `row`, or ask first when the runtime rates it incompatible. */
  install: (row: VariantRow) => Promise<void>;
  /** Install the incompatible variant the person confirmed. */
  confirmInstall: () => Promise<void>;
  cancelInstall: () => void;
  /** Choose a folder and import the engines it holds. */
  importFromFolder: () => Promise<void>;
  /** Open the confirmation of the removal of `target`. */
  askRemove: (target: RemovalTarget) => void;
  /** Send the removal; true once the build is gone. */
  confirmRemove: () => Promise<boolean>;
  cancelRemove: () => void;
}

/** True while the runtime is working on the installation. */
export function isRunning(installation: EngineInstallation): boolean {
  return (
    installation.state === "verifying" ||
    installation.state === "downloading" ||
    installation.state === "installing"
  );
}

/** The installation as the line of `target` shows it, or `null` for nothing on that line. */
export function installationOn(
  state: EngineManagementState,
  target: InstallTarget,
): EngineInstallation | null {
  const own = state.target;
  if (own === null || state.installation.state === "idle") return null;
  const same =
    own.kind === target.kind &&
    (own.kind === "import" || (target.kind === "variant" && own.variant === target.variant));
  return same ? state.installation : null;
}

/** Whole percent of a download, `null` before its size is known. */
export function downloadPercent(installation: EngineInstallation): number | null {
  if (installation.state !== "downloading" || installation.total <= 0) return null;
  return Math.min(100, Math.floor((installation.received / installation.total) * 100));
}

/** The i18n key of the sentence a refusal of removal is worded with. */
export const REMOVAL_REFUSAL_KEYS: Record<
  Extract<RemovalStep, { phase: "refused" }>["code"],
  string
> = {
  in_service: "settings.engines.remove.refused.in_service",
  embedded: "settings.engines.remove.refused.embedded",
  locked: "settings.engines.remove.refused.locked",
  not_found: "settings.engines.remove.refused.not_found",
  chosen: "settings.engines.remove.refused.other",
  config: "settings.engines.remove.refused.other",
  store: "settings.engines.remove.refused.other",
  unknown: "settings.engines.remove.refused.other",
  unreachable: "settings.engines.remove.refused.other",
};

const IDLE: EngineInstallation = { state: "idle" };

export function createEngineManagement(deps: EngineManagementDeps): EngineManagement {
  let current: EngineManagementState = {
    installation: IDLE,
    target: null,
    pendingInstall: null,
    removal: null,
    busy: false,
  };
  const { subscribe, set } = writable(current);
  let following = false;

  function patch(part: Partial<EngineManagementState>): void {
    current = { ...current, ...part };
    set(current);
  }

  /** Read the installation until it stops running, then the engines it changed. */
  async function follow(): Promise<void> {
    if (following) return;
    following = true;
    try {
      while (isRunning(current.installation)) {
        await deps.wait(POLL_INTERVAL_MS);
        patch({ installation: await deps.installation() });
      }
      if (current.installation.state === "installed") await deps.reloadEngines();
    } catch (err: unknown) {
      deps.onError(err);
    } finally {
      following = false;
    }
  }

  async function begin(target: InstallTarget, call: () => Promise<EngineInstallation>): Promise<void> {
    if (current.busy || isRunning(current.installation)) return;
    patch({ busy: true, target });
    try {
      patch({ installation: await call() });
    } catch (err: unknown) {
      deps.onError(err);
      return;
    } finally {
      patch({ busy: false });
    }
    await follow();
  }

  function startVariant(variant: EngineVariant): Promise<void> {
    return begin({ kind: "variant", variant }, () => deps.start(INSTALLED_ENGINE, variant));
  }

  return {
    subscribe,

    async refresh() {
      try {
        patch({ installation: await deps.installation() });
      } catch {
        patch({ installation: IDLE });
      }
      if (isRunning(current.installation)) await follow();
    },

    async install(row) {
      if (row.installed) return;
      if (row.verdict === "incompatible") {
        patch({ pendingInstall: row });
        return;
      }
      await startVariant(row.variant);
    },

    async confirmInstall() {
      const row = current.pendingInstall;
      if (row === null) return;
      patch({ pendingInstall: null });
      await startVariant(row.variant);
    },

    cancelInstall() {
      patch({ pendingInstall: null });
    },

    async importFromFolder() {
      if (current.busy || isRunning(current.installation)) return;
      let folder: string | null;
      try {
        folder = await deps.pickFolder();
      } catch (err: unknown) {
        deps.onError(err);
        return;
      }
      if (folder === null) return;
      const path = folder;
      await begin({ kind: "import" }, () => deps.importFolder(path));
    },

    askRemove(target) {
      patch({ removal: { target, step: { phase: "ask" } } });
    },

    async confirmRemove() {
      const removal = current.removal;
      if (removal === null || current.busy || removal.step.phase === "refused") return false;
      const { target } = removal;
      const confirm = removal.step.phase === "chosen";
      patch({ busy: true });
      let outcome: EngineRemoval;
      try {
        outcome = await deps.remove(target.engine, target.tag, target.variant, confirm);
      } catch (err: unknown) {
        patch({
          busy: false,
          removal: { target, step: { phase: "refused", code: "unreachable", detail: String(err) } },
        });
        return false;
      }
      if (outcome.outcome === "removed") {
        patch({ busy: false, removal: null });
        try {
          await deps.reloadEngines();
        } catch (err: unknown) {
          deps.onError(err);
        }
        return true;
      }
      const step: RemovalStep =
        outcome.code === "chosen" && !confirm
          ? { phase: "chosen", after: outcome.after ?? "auto" }
          : { phase: "refused", code: outcome.code, detail: outcome.error };
      patch({ busy: false, removal: { target, step } });
      return false;
    },

    cancelRemove() {
      if (current.busy) return;
      patch({ removal: null });
    },
  };
}
