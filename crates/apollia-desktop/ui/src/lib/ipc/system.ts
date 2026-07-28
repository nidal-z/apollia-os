/**
 * Typed Tauri IPC wrappers for the System settings sub-page.
 *
 * One thin function per `#[tauri::command]` backing the System page: the CLI
 * symlink install / uninstall. Components call these instead of `invoke()`
 * directly, so command names and payload shapes live in a single place.
 *
 * The self-update check / install wrappers live in `$lib/ipc/updates` (the
 * single source for `check_for_update` / `install_update`). The read-side
 * commands (`get_system_info`, `get_security_posture`, `get_cli_status`,
 * `get_config`) are loaded through the settings store's `settingsLoaders`.
 */
import { invoke } from "@tauri-apps/api/core";

/** Install the `apollia` CLI symlink so the runtime is drivable from a shell. */
export async function installCli(): Promise<void> {
  return invoke<void>("install_cli");
}

/** Remove the `apollia` CLI symlink. */
export async function uninstallCli(): Promise<void> {
  return invoke<void>("uninstall_cli");
}
