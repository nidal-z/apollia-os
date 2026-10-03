// "Keep Apollia running without the application", in Settings > Developer,
// block "Service without interface".
//
// The decisions live here, apart from the component, so Vitest can hold them
// without rendering anything. The switch shows the service the OS reports,
// never a stored value; the organization policy only locks the authorization
// to install it.

import type { BackgroundService } from "$lib/ipc/backgroundService";

/** The i18n keys of the block, in reading order. */
export const BACKGROUND_SERVICE_KEYS = {
  title: "settings.developer.service.title",
  label: "settings.developer.service.switch_label",
  hint: "settings.developer.service.switch_hint",
  help: "settings.developer.service.help",
  helpLabel: "settings.developer.service.help_label",
} as const;

/** The shell calls the block makes, replaceable in tests. */
export interface BackgroundServiceIpc {
  read: () => Promise<BackgroundService>;
  write: (on: boolean) => Promise<void>;
}

/** The switch as the block shows it. */
export interface BackgroundServiceSwitch {
  /** On when the service is installed. */
  on: boolean;
  running: boolean;
  /** True when the switch refuses a change. */
  disabled: boolean;
  /** True when the organization policy forbids installing. */
  locked: boolean;
  /** The i18n key of the reason the switch cannot be turned on, if any. */
  reasonKey: string | null;
}

/** The block exists only with the developer mode on. */
export function backgroundServiceVisible(developerMode: boolean): boolean {
  return developerMode;
}

/** Off, free: what the block shows until the shell answers. */
export const SWITCH_OFF: BackgroundServiceSwitch = {
  on: false,
  running: false,
  disabled: false,
  locked: false,
  reasonKey: null,
};

/**
 * The switch for an observed service. Turning it off is always possible;
 * turning it on needs a service manager, the bundled command line and the
 * authorization.
 */
export function backgroundServiceSwitch(service: BackgroundService): BackgroundServiceSwitch {
  const locked = !service.allowed && service.allowed_source === "policy";
  let reasonKey: string | null = null;
  if (!service.supported) reasonKey = "settings.developer.service.unsupported";
  else if (!service.bundled) reasonKey = "settings.developer.service.no_cli";
  const cannotTurnOn = reasonKey !== null || !service.allowed;
  return {
    on: service.installed,
    running: service.running,
    disabled: !service.installed && cannotTurnOn,
    locked,
    reasonKey: service.installed ? null : reasonKey,
  };
}

/** Reads the switch; off when the shell cannot be reached. */
export async function readBackgroundService(
  ipc: BackgroundServiceIpc,
): Promise<BackgroundServiceSwitch> {
  try {
    return backgroundServiceSwitch(await ipc.read());
  } catch {
    return SWITCH_OFF;
  }
}
