// Typed IPC wrappers for "Keep Apollia running without the application".
//
// The shell reports the service as the OS service manager holds it, and
// whether installing it is allowed. Turning it on or off relaunches the
// application, which then attaches to the service or embeds the runtime.

import type { SettingSource } from "$lib/ipc/settings";

/** The service as the shell observes it. */
export interface BackgroundService {
  /** This OS has a service manager the runtime can be installed with. */
  supported: boolean;
  /** The command line bundled with the application was found. */
  bundled: boolean;
  /** The service is installed: the switch is on. */
  installed: boolean;
  /** The service's process is running. */
  running: boolean;
  /** Installing is allowed. */
  allowed: boolean;
  /** Where the authorization comes from; `"policy"` locks the switch. */
  allowed_source: SettingSource;
}

/** Reads the service and its authorization. */
export async function getBackgroundService(): Promise<BackgroundService> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<BackgroundService>("get_background_service");
}

/**
 * Installs (`on`) or removes the service, then relaunches the application.
 * Rejects when installing is not allowed or the service manager refuses.
 */
export async function setBackgroundService(on: boolean): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("set_background_service", { on });
}
