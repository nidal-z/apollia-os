/**
 * Global keyboard-shortcut handlers.
 *
 * Wires the GLOBAL-scope entries of the shortcut catalog
 * (`$lib/navigation/shortcuts`) to real actions through the single window
 * listener installed by `mountShortcutDispatcher`. The app shell calls
 * `registerGlobalShortcutHandlers()` once on mount and runs the returned
 * cleanup on teardown.
 *
 * Only the global chords with no competing surface-owned listener are wired
 * here, so the dispatcher never double-fires against a listener this task is
 * not allowed to touch:
 *
 *  - `palette.open` (Cmd/Ctrl+K): the sole legacy owner was
 *    `$lib/keyboard/globalShortcuts`, removed in favour of this path.
 *  - `companion.toggle` (Cmd+/ on macOS, Ctrl+/ elsewhere): opens the detail
 *    sidebar on the Guide tab, or collapses it when the guide is shown.
 *  - `companion.focus` (Cmd/Ctrl+Shift+C): opens the Guide tab and focuses
 *    its input.
 *  - `settings.engines` (Cmd/Ctrl+Shift+R) and `settings.hardware`
 *    (Cmd/Ctrl+Shift+H): open the settings window on the System group.
 *
 * The remaining global chords stay on the surfaces that already own them and
 * are tracked as a follow-up (each requires editing that surface to drop its
 * legacy listener before it can move here):
 *  - `help.overlay` (?)                -> KeyboardHintOverlay.svelte
 *  - `nav.back` / `nav.forward` (Cmd+[ / Cmd+]) -> Main.svelte
 *  - `nav.sidebar` (Cmd+B)             -> chat.toggle_sidebar (chat route)
 *  - `app.quit` (Cmd+Q)               -> native window manager
 */
import { registerShortcutHandler } from "$lib/navigation/shortcutDispatcher";
import { commandPaletteOpen } from "$lib/stores/commandPalette";
import { detailSidebar } from "$lib/stores/detailSidebar";
import { openSettingsWindow } from "$lib/stores/settingsWindow";
import type { SettingsSubRoute } from "$lib/stores/settings";

/** The System sections each have a chord that opens the settings on them. */
export const SETTINGS_SECTION_SHORTCUTS: Readonly<Record<string, SettingsSubRoute>> = {
  "settings.engines": "engines",
  "settings.hardware": "hardware",
};

/**
 * Focus the guide's chat input once the Guide tab has had a frame to render.
 * The lookup is scoped to the tab and fails silently when the input is not
 * present, so a missing surface never throws.
 */
function focusCompanionInput(): void {
  if (typeof document === "undefined") return;
  requestAnimationFrame(() => {
    const input = document.querySelector<HTMLTextAreaElement | HTMLInputElement>(
      "[data-testid='companion-panel'] textarea, [data-testid='companion-panel'] input",
    );
    input?.focus();
  });
}

/**
 * Register every global shortcut handler. Returns a cleanup that unregisters
 * all of them.
 */
export function registerGlobalShortcutHandlers(): () => void {
  const disposers: Array<() => void> = [
    registerShortcutHandler({
      id: "palette.open",
      // Operators expect the palette to open even while typing.
      allowInInput: true,
      handler: () => commandPaletteOpen.update((open) => !open),
    }),
    registerShortcutHandler({
      id: "companion.toggle",
      allowInInput: true,
      handler: () => detailSidebar.toggleGuide(),
    }),
    registerShortcutHandler({
      id: "companion.focus",
      allowInInput: true,
      handler: () => {
        detailSidebar.openGuide();
        focusCompanionInput();
      },
    }),
    ...Object.entries(SETTINGS_SECTION_SHORTCUTS).map(([id, section]) =>
      registerShortcutHandler({
        id,
        allowInInput: true,
        handler: () => {
          openSettingsWindow(section);
        },
      }),
    ),
  ];

  return () => {
    for (const dispose of disposers) dispose();
  };
}
