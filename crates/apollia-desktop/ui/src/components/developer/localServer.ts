// The bar of the local server, on the developer screen: what it shows for a
// state of the server, what its switch does, what "Load a model" offers.
//
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything.

import type { LocalServerRefusal, LocalServerView } from "$lib/ipc/localServer";
import type { ModelTarget, TargetGroups } from "$lib/chat/chatTargets";
import type { ModelLoadAsk } from "$lib/stores/modelResidency";
import type { LoadedModelsResponse } from "$lib/types";
import { needsLoadWindow } from "$lib/chat/modelLoad";

/** The id of the switch in the shortcut catalogue. */
export const SERVER_TOGGLE_SHORTCUT = "developer.server_toggle";

/** The shell calls the bar makes, replaceable in tests. */
export interface LocalServerIpc {
  read: () => Promise<LocalServerView>;
  start: () => Promise<LocalServerView>;
  stop: () => Promise<LocalServerView>;
}

/** Why the bar shows an error under itself. */
export type ServerFailure =
  /** Another program holds the port the server is set to. */
  | { kind: "port_in_use"; port: number }
  /** The start or the stop failed for another reason, in the runtime's words. */
  | { kind: "other"; message: string }
  /** The state could not be read at all. */
  | { kind: "unreadable"; message: string };

/** What the bar holds between two gestures. */
export interface ServerBarState {
  /** The last state read, `null` until the first read answers. */
  view: LocalServerView | null;
  /** The failure of the last gesture, `null` when it went through. */
  failure: ServerFailure | null;
}

/** The bar before anything was read. */
export const EMPTY_BAR: ServerBarState = { view: null, failure: null };

/** What the switch reads: running, stopped, in error, or not known yet. */
export type ServerStatus = "running" | "stopped" | "error" | "unknown";

/** The status the bar shows for `state`. */
export function serverStatus(state: ServerBarState): ServerStatus {
  if (state.view?.running) return "running";
  if (state.failure) return "error";
  return state.view ? "stopped" : "unknown";
}

/** The i18n key of the status, next to the switch. */
export const STATUS_LABELS: Record<ServerStatus, string> = {
  running: "dev.developer.server.status_running",
  stopped: "dev.developer.server.status_stopped",
  error: "dev.developer.server.status_error",
  unknown: "dev.developer.server.status_unknown",
};

/**
 * The address a client reaches the server at, `null` while it does not
 * listen: a stopped server has an address configured and nobody behind it.
 */
export function reachableAddress(state: ServerBarState): string | null {
  return state.view?.running ? state.view.address : null;
}

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}

function isRefusal(error: unknown): error is LocalServerRefusal {
  if (typeof error !== "object" || error === null) return false;
  const candidate = error as Record<string, unknown>;
  return typeof candidate.kind === "string" && typeof candidate.message === "string";
}

/** The failure a rejected start is shown as, for the port the server is set to. */
export function startFailure(error: unknown, port: number | null): ServerFailure {
  if (isRefusal(error)) {
    if (error.kind === "port_in_use" && port !== null) return { kind: "port_in_use", port };
    return { kind: "other", message: error.message };
  }
  return { kind: "other", message: message(error) };
}

/** Reads the state. A read that fails keeps what was known and says why. */
export async function readServer(
  ipc: LocalServerIpc,
  previous: ServerBarState = EMPTY_BAR,
): Promise<ServerBarState> {
  try {
    return { view: await ipc.read(), failure: previous.failure };
  } catch (error) {
    return { view: previous.view, failure: { kind: "unreadable", message: message(error) } };
  }
}

/**
 * Starts the server when `on`, stops it otherwise, and returns the bar as it
 * stands after. A refused start leaves the server stopped, with its reason;
 * the state is read again so the port named is the one in force.
 */
export async function switchServer(
  ipc: LocalServerIpc,
  previous: ServerBarState,
  on: boolean,
): Promise<ServerBarState> {
  if (!on) {
    try {
      return { view: await ipc.stop(), failure: null };
    } catch (error) {
      return { view: previous.view, failure: { kind: "other", message: message(error) } };
    }
  }
  try {
    return { view: await ipc.start(), failure: null };
  } catch (error) {
    const view = await ipc.read().catch(() => previous.view);
    return { view, failure: startFailure(error, view?.port ?? null) };
  }
}

/** What the shortcut and the switch ask for from `state`: the other position. */
export function nextSwitchPosition(state: ServerBarState): boolean {
  return !(state.view?.running ?? false);
}

/** Copies the reachable address; `false` when there is none or the copy fails. */
export async function copyAddress(
  state: ServerBarState,
  write: (text: string) => Promise<void>,
): Promise<boolean> {
  const address = reachableAddress(state);
  if (address === null) return false;
  try {
    await write(address);
    return true;
  } catch {
    return false;
  }
}

/**
 * The models "Load a model" offers: those of this computer that are not in
 * memory, in the order of the list.
 */
export function loadableModels(
  groups: TargetGroups,
  loaded: LoadedModelsResponse | null,
): ModelTarget[] {
  return groups.models.filter((model) => needsLoadWindow(model, loaded));
}

/** What the loading window is asked to open on for `model`. */
export function loadAsk(model: ModelTarget, onloaded: () => void): ModelLoadAsk {
  return { modelId: model.route, format: model.format, onloaded };
}

/** The test id of a model in the picker, stable across names. */
export function loadModelTestId(model: ModelTarget): string {
  return `developer-load-model-${model.route.replace(/[^A-Za-z0-9_-]/g, "_")}`;
}
