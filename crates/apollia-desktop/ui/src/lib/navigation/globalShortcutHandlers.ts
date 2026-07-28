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
 *  - `companion.focus` (Cmd/Ctrl+Shift+C): previously unhandled.
 *
 * The remaining global chords stay on the surfaces that already own them and
 * are tracked as a follow-up (each requires editing that surface to drop its
 * legacy listener before it can move here):
 *  - `help.overlay` (?)                -> KeyboardHintOverlay.svelte
 *  - `nav.back` / `nav.forward` (Cmd+[ / Cmd+]) -> Main.svelte
 *  - `nav.sidebar` (Cmd+B)             -> chat.toggle_sidebar (chat route)
 *  - `companion.toggle` (Cmd+/)        -> CompanionToggle.svelte + chat.help
 *  - `companion.move` (Cmd+Alt+Arrows) -> CompanionPanel.svelte (directional,
 *    not dispatchable by the catalog dispatcher)
 *  - `app.quit` (Cmd+Q)               -> native window manager
 */
import { registerShortcutHandler } from "$lib/navigation/shortcutDispatcher";
import { commandPaletteOpen } from "$lib/stores/commandPalette";
import { companionStore } from "$lib/stores/companion";

/**
 * Focus the Companion chat input once the panel has had a frame to render.
 * The lookup is scoped to the panel and fails silently when the input is not
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
      id: "companion.focus",
      allowInInput: true,
      handler: () => {
        companionStore.openCompanion();
        focusCompanionInput();
      },
    }),
  ];

  return () => {
    for (const dispose of disposers) dispose();
  };
}
