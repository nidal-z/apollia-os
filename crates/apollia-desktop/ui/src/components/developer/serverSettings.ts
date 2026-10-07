// The settings of the local server, opened from its bar on the developer
// screen: which rows the panel shows and in what order, what each writes, when
// a write reopens the server, and what the bar warns about.
//
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything. The values are the keys of `[server]`, through
// the one store `$lib/stores/serverSettings`.

import type { LocalServerRefusal, LocalServerView } from "$lib/ipc/localServer";
import {
  SERVER_SETTING_KEYS,
  type ServerSetting,
  type ServerSettingKey,
} from "$lib/stores/serverSettings";
import { parseIdleMinutes } from "../settings/developer/onDemandLoading";
import type { ServerFailure } from "./localServer";

/** What a row is set with. */
export type ServerSettingControl = "port" | "switch" | "minutes";

/** One row of the panel: its key, its control, its texts, its test id. */
export interface ServerSettingRow {
  key: ServerSettingKey;
  control: ServerSettingControl;
  /** The i18n key of the name of the setting. */
  label: string;
  /** The i18n key of the explanation shown on hover of its "?". */
  help: string;
  /** The short name of the row, which its `data-testid` ends with. */
  name: string;
  /** The row sits under the one before it, which switches it on. */
  under?: ServerSettingKey;
}

/** The `data-testid` of a row; the ids of its parts derive from it. */
export function settingTestId(item: ServerSettingRow): string {
  const testid = `developer-server-setting-${item.name}`;
  return testid;
}

/** The rows of the panel, in reading order. */
export const SERVER_SETTING_ROWS: readonly ServerSettingRow[] = [
  {
    key: SERVER_SETTING_KEYS.port,
    control: "port",
    label: "dev.developer.settings.port_label",
    help: "dev.developer.settings.port_help",
    name: "port",
  },
  {
    key: SERVER_SETTING_KEYS.requireAuth,
    control: "switch",
    label: "dev.developer.settings.require_auth_label",
    help: "dev.developer.settings.require_auth_help",
    name: "require-auth",
  },
  {
    key: SERVER_SETTING_KEYS.serveOnLan,
    control: "switch",
    label: "dev.developer.settings.serve_on_lan_label",
    help: "dev.developer.settings.serve_on_lan_help",
    name: "serve-on-lan",
  },
  {
    key: SERVER_SETTING_KEYS.allowRequestMcp,
    control: "switch",
    label: "dev.developer.settings.allow_request_mcp_label",
    help: "dev.developer.settings.allow_request_mcp_help",
    name: "allow-request-mcp",
  },
  {
    key: SERVER_SETTING_KEYS.cors,
    control: "switch",
    label: "dev.developer.settings.cors_label",
    help: "dev.developer.settings.cors_help",
    name: "cors",
  },
  {
    key: SERVER_SETTING_KEYS.jitLoading,
    control: "switch",
    label: "dev.developer.settings.jit_loading_label",
    help: "dev.developer.settings.jit_loading_help",
    name: "jit-loading",
  },
  {
    key: SERVER_SETTING_KEYS.jitAutoUnload,
    control: "switch",
    label: "dev.developer.settings.jit_auto_unload_label",
    help: "dev.developer.settings.jit_auto_unload_help",
    name: "jit-auto-unload",
  },
  {
    key: SERVER_SETTING_KEYS.jitIdleTtlMinutes,
    control: "minutes",
    label: "dev.developer.settings.jit_idle_label",
    help: "dev.developer.settings.jit_idle_help",
    name: "jit-idle",
    under: SERVER_SETTING_KEYS.jitAutoUnload,
  },
  {
    key: SERVER_SETTING_KEYS.jitKeepLastOnly,
    control: "switch",
    label: "dev.developer.settings.jit_keep_last_label",
    help: "dev.developer.settings.jit_keep_last_help",
    name: "jit-keep-last",
  },
  {
    key: SERVER_SETTING_KEYS.separateReasoning,
    control: "switch",
    label: "dev.developer.settings.separate_reasoning_label",
    help: "dev.developer.settings.separate_reasoning_help",
    name: "separate-reasoning",
  },
];

/** The other i18n keys of the panel and of the warning of the bar. */
export const SERVER_SETTINGS_TEXTS = {
  open: "dev.developer.settings.open",
  title: "dev.developer.settings.title",
  helpLabel: "dev.developer.settings.help_label",
  activeTokens: "dev.developer.settings.active_tokens",
  manageTokens: "dev.developer.settings.manage_tokens",
  minutes: "settings.developer.on_demand.idle_unit",
  portInvalid: "dev.developer.settings.port_invalid",
  portTaken: "dev.developer.settings.port_taken",
  minutesInvalid: "dev.developer.settings.minutes_invalid",
  writeFailed: "dev.developer.settings.write_failed",
  reopenFailed: "dev.developer.settings.reopen_failed",
  policyLocked: "settings.policy_locked",
  policyLockedHint: "settings.policy_locked_hint",
  lanWarning: "dev.developer.settings.lan_warning",
  lanRequireAuth: "dev.developer.settings.lan_require_auth",
} as const;

/** The lowest port a person may choose: those under it belong to the system. */
export const LOWEST_PORT = 1024;
/** The highest port there is. */
export const HIGHEST_PORT = 65535;

/** The port typed in the field; `null` for anything but a whole number in range. */
export function parsePort(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d{1,5}$/.test(trimmed)) return null;
  const port = Number(trimmed);
  return port >= LOWEST_PORT && port <= HIGHEST_PORT ? port : null;
}

/**
 * The settings the listener reads when it opens: a running server applies
 * them once reopened. The others are read at every request.
 */
const READ_AT_OPENING: readonly ServerSettingKey[] = [
  SERVER_SETTING_KEYS.port,
  SERVER_SETTING_KEYS.requireAuth,
  SERVER_SETTING_KEYS.serveOnLan,
  SERVER_SETTING_KEYS.cors,
];

/** Whether a running server must be reopened for `key` to apply. */
export function appliesAtReopening(key: ServerSettingKey): boolean {
  return READ_AT_OPENING.includes(key);
}

/**
 * Whether the organization policy imposes `key`: the shell says so with the
 * source of the value, the runtime in the state of the server for the three
 * settings it can close itself.
 */
export function imposedByPolicy(
  key: ServerSettingKey,
  setting: ServerSetting,
  view: LocalServerView | null,
): boolean {
  if (setting.source === "policy") return true;
  const locks = view?.imposedByPolicy;
  if (!locks) return false;
  if (key === SERVER_SETTING_KEYS.serveOnLan) return locks.serveOnLan;
  if (key === SERVER_SETTING_KEYS.requireAuth) return locks.requireAuth;
  if (key === SERVER_SETTING_KEYS.allowRequestMcp) return locks.allowRequestMcp;
  return false;
}

/** A row under another takes a value only while that other is on. */
export function rowDisabled(
  item: ServerSettingRow,
  settings: Record<ServerSettingKey, ServerSetting>,
  view: LocalServerView | null,
): boolean {
  if (imposedByPolicy(item.key, settings[item.key], view)) return true;
  return item.under !== undefined && settings[item.under].value !== true;
}

/**
 * Whether the bar warns: the server answers the local network and asks for
 * no token. Models then answer any device of the network; agents and
 * workflows still require a token from it.
 */
export function servedOnLanWithoutAuth(view: LocalServerView | null, requireAuth: boolean): boolean {
  return view !== null && view.running && view.serveOnLan && !requireAuth;
}

/** Why a change did not go through, shown in the panel, which stays open. */
export type SettingFailure =
  /** The port typed is not a port a person may choose. */
  | { kind: "port_invalid" }
  /** Another program holds the port the server was reopened on. */
  | { kind: "port_taken"; port: number }
  /** The delay typed is not a whole number of minutes. */
  | { kind: "minutes_invalid" }
  /** The shell refused the write: the policy, the file, the type. */
  | { kind: "write" }
  /** The server did not reopen, in the runtime's words. */
  | { kind: "reopen"; message: string };

/** What a change leaves behind. */
export interface SettingChange {
  /** The failure to show, `null` when the change went through. */
  failure: SettingFailure | null;
  /** The state of the server after a reopening, `undefined` when none ran. */
  view?: LocalServerView | null;
}

/** What a change needs from the outside, replaceable in tests. */
export interface SettingChangeDeps {
  /** Writes the key through the shared store; `false` when refused. */
  write: (key: ServerSettingKey, value: boolean | number) => Promise<boolean>;
  /** Reopens the server; rejects with a `LocalServerRefusal`. */
  restart: () => Promise<LocalServerView>;
  /** Reads the state of the server. */
  read: () => Promise<LocalServerView>;
  /** The state of the server before the change. */
  view: LocalServerView | null;
}

function refusalKind(error: unknown): LocalServerRefusal["kind"] {
  if (typeof error !== "object" || error === null) return "other";
  return (error as Record<string, unknown>).kind === "port_in_use" ? "port_in_use" : "other";
}

function words(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (typeof error === "object" && error !== null) {
    const message = (error as Record<string, unknown>).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

/**
 * Writes `key`, then reopens a running server when the listener reads that
 * key at its opening. A refused reopening leaves the server stopped: the
 * state is read again and the reason kept, a taken port apart from the rest.
 */
export async function changeServerSetting(
  key: ServerSettingKey,
  value: boolean | number,
  deps: SettingChangeDeps,
): Promise<SettingChange> {
  if (!(await deps.write(key, value))) return { failure: { kind: "write" } };
  if (!appliesAtReopening(key) || !deps.view?.running) return { failure: null };
  try {
    return { failure: null, view: await deps.restart() };
  } catch (error) {
    const view = await deps.read().catch(() => deps.view);
    if (refusalKind(error) === "port_in_use") {
      const port = key === SERVER_SETTING_KEYS.port ? Number(value) : (view?.port ?? Number.NaN);
      return { failure: { kind: "port_taken", port }, view };
    }
    return { failure: { kind: "reopen", message: words(error) }, view };
  }
}

/**
 * The port typed in the field, written when it is a port and differs from
 * the one in force. Anything else is refused before any write.
 */
export async function changePort(
  text: string,
  current: number,
  deps: SettingChangeDeps,
): Promise<SettingChange> {
  const port = parsePort(text);
  if (port === null) return { failure: { kind: "port_invalid" } };
  if (port === current) return { failure: null };
  return changeServerSetting(SERVER_SETTING_KEYS.port, port, deps);
}

/** The delay typed in the field, written when it is one and differs. */
export async function changeMinutes(
  text: string,
  current: number,
  deps: SettingChangeDeps,
): Promise<SettingChange> {
  const minutes = parseIdleMinutes(text);
  if (minutes === null) return { failure: { kind: "minutes_invalid" } };
  if (minutes === current) return { failure: null };
  return changeServerSetting(SERVER_SETTING_KEYS.jitIdleTtlMinutes, minutes, deps);
}

/** What the bar shows under itself after a reopening the panel asked for. */
export function barFailure(failure: SettingFailure | null): ServerFailure | null {
  if (failure?.kind === "port_taken") return { kind: "port_in_use", port: failure.port };
  if (failure?.kind === "reopen") return { kind: "other", message: failure.message };
  return null;
}
