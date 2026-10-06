// The settings of the compatible server, as the server settings of the
// Developer screen show them all and Settings > Developer shows some.
//
// Each lives under one key of `[server]` in `apollia.toml`, read and written
// through the shell, so every screen reads this one store and writes the same
// key; the organization policy can impose any of them. The runtime reads the
// loading and reasoning settings at every request or every check; the port,
// the token check, the local network and CORS are read when the listener
// opens, so a running server applies them once reopened.

import { get, writable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";

/** The dotted keys, one per setting, never repeated elsewhere. */
export const SERVER_SETTING_KEYS = {
  port: "server.port",
  requireAuth: "server.require_auth",
  serveOnLan: "server.serve_on_lan",
  allowRequestMcp: "server.allow_request_mcp",
  cors: "server.cors",
  jitLoading: "server.jit_loading",
  jitKeepLastOnly: "server.jit_keep_last_only",
  jitAutoUnload: "server.jit_auto_unload",
  jitIdleTtlMinutes: "server.jit_idle_ttl_minutes",
  separateReasoning: "server.separate_reasoning_content",
} as const;

/** One of the keys above. */
export type ServerSettingKey = (typeof SERVER_SETTING_KEYS)[keyof typeof SERVER_SETTING_KEYS];

/** The value of each setting when no one wrote one, as the runtime defaults it. */
export const SERVER_SETTING_DEFAULTS: Record<ServerSettingKey, boolean | number> = {
  "server.port": 7772,
  "server.require_auth": false,
  "server.serve_on_lan": false,
  "server.allow_request_mcp": false,
  "server.cors": false,
  "server.jit_loading": true,
  "server.jit_keep_last_only": true,
  "server.jit_auto_unload": true,
  "server.jit_idle_ttl_minutes": 60,
  "server.separate_reasoning_content": true,
};

/** A setting in force and where its value comes from. */
export interface ServerSetting {
  value: boolean | number;
  source: SettingSource;
}

const ALL_KEYS = Object.values(SERVER_SETTING_KEYS) as ServerSettingKey[];

function defaults(): Record<ServerSettingKey, ServerSetting> {
  const entries = ALL_KEYS.map((key) => [key, { value: SERVER_SETTING_DEFAULTS[key], source: "default" }]);
  return Object.fromEntries(entries) as Record<ServerSettingKey, ServerSetting>;
}

/** The settings in force, the defaults until the shell answers. */
export const serverSettings = writable<Record<ServerSettingKey, ServerSetting>>(defaults());

/** The two shell calls the store makes, replaceable in tests. */
export interface ServerSettingsIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: ServerSettingsIpc = { read: getResolvedSetting, write: setSetting };

/** Whether `value` has the type of `key`'s default. */
function fits(key: ServerSettingKey, value: unknown): value is boolean | number {
  return typeof value === typeof SERVER_SETTING_DEFAULTS[key];
}

function apply(key: ServerSettingKey, resolved: unknown): void {
  if (typeof resolved !== "object" || resolved === null) return;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  serverSettings.update((all) => ({
    ...all,
    [key]: {
      value: fits(key, value) ? value : SERVER_SETTING_DEFAULTS[key],
      source: source ?? all[key].source,
    },
  }));
}

/** Seeds every setting from the value in force. A failure keeps its default. */
export async function hydrateServerSettings(ipc: ServerSettingsIpc = shellIpc): Promise<void> {
  await Promise.all(
    ALL_KEYS.map(async (key) => {
      try {
        apply(key, await ipc.read(key));
      } catch {
        // Shell unreachable: this setting stays at its default.
      }
    }),
  );
}

/**
 * Writes one setting. The control moves at once and comes back when the shell
 * refuses the write (a value the policy imposes, an unreadable file, a value
 * of the wrong type). Resolves to whether the write went through.
 */
export async function setServerSetting(
  key: ServerSettingKey,
  value: boolean | number,
  ipc: ServerSettingsIpc = shellIpc,
): Promise<boolean> {
  const previous = get(serverSettings)[key];
  if (previous.source === "policy" || !fits(key, value)) return false;
  serverSettings.update((all) => ({ ...all, [key]: { ...all[key], value } }));
  try {
    apply(key, await ipc.write(key, value));
    return true;
  } catch {
    serverSettings.update((all) => ({ ...all, [key]: previous }));
    return false;
  }
}
