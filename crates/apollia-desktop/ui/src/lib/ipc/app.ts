// Typed IPC wrappers for app-lifecycle commands.
//
// Keeping the `invoke` call here removes direct Tauri usage from the `.svelte`
// shell components. `quitApp` routes through the `quit_app` command rather than
// `window.close()`: closing the window only hides it (close-to-tray). The
// command exits the process gracefully via the shared quit path.

/** Quits the desktop app gracefully. No-op in a web/dev context. */
export async function quitApp(): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("quit_app");
  } catch {
    // Web/dev fallback: no Tauri bridge, nothing to quit.
  }
}

/**
 * Transmit the interface locale to the native shell (`ui-locale-changed`
 * event), so the tray menu, the macOS app menu and the STT notifications
 * render in the language of the interface. No-op in a web/dev context.
 */
export async function notifyUiLocale(locale: string): Promise<void> {
  try {
    const { emit } = await import("@tauri-apps/api/event");
    await emit("ui-locale-changed", { locale });
  } catch {
    // Web/dev fallback: no Tauri bridge, no native surface to relabel.
  }
}

/**
 * Open a config file in the system's default editor. Without `path`, the
 * runtime opens the active `apollia.toml`. This is the single wrapper for
 * `open_config_in_editor`, shared by Settings > Configuration and
 * Settings > Observability.
 */
export async function openConfigInEditor(path?: string): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_config_in_editor", path ? { path } : undefined);
}
