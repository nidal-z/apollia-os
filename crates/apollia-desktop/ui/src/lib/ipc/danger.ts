/**
 * Typed Tauri command wrappers for the Settings > Danger zone.
 *
 * Groups the irreversible maintenance operations (onboarding replay, memory
 * and log wipes, factory reset) plus the app restart that factory reset needs.
 * Components never call `invoke` directly; they import these helpers so the
 * command names and payload shapes live in one typed place (see
 * `crates/apollia-desktop/ui/AGENTS.md`, section 3).
 */
import { invoke } from "@tauri-apps/api/core";

/** Clear the onboarding flag so the Welcome wizard runs again. No data loss. */
export async function resetOnboarding(): Promise<void> {
  return invoke<void>("reset_onboarding");
}

/**
 * Wipe every memory namespace (user profile, agents, projects). Returns the
 * number of memory entries removed.
 */
export async function clearAllMemories(): Promise<number> {
  return invoke<number>("clear_all_memories");
}

/** Remove the local log directory. The chained audit journal is untouched. */
export async function clearLogs(): Promise<void> {
  return invoke<void>("clear_logs");
}

/**
 * Delete the entire Apollia configuration directory. A restart is mandatory to
 * re-initialise the runtime afterwards (see {@link appRestart}).
 */
export async function factoryReset(): Promise<void> {
  return invoke<void>("factory_reset");
}

/**
 * Restart the packaged app. May reject in dev mode (no packaged bundle), in
 * which case the caller falls back to a manual-restart prompt.
 */
export async function appRestart(): Promise<void> {
  return invoke<void>("app_restart");
}
