/**
 * Central chat keyboard shortcuts.
 *
 * One singleton listener replaces the ad-hoc `window.addEventListener`
 * calls scattered across components. Bindings are described declaratively
 * with `event.code` (KeyN, KeyB, …) so that AZERTY/QWERTY layouts pick
 * up the same physical keys instead of producing different `event.key`s.
 *
 * Each binding owns:
 *   - `id` — used by the help dialog to dedupe and i18n the description.
 *   - `chord` — modifier requirements + KeyboardEvent.code.
 *   - `allowInTextarea` — when false (default) the binding is skipped if
 *     focus is inside an editable field, except for chords that include
 *     a Cmd/Ctrl modifier (operators expect Cmd+K to work everywhere).
 *
 * The store does NOT wire any UI — `Chat.svelte` calls `installChatShortcuts`
 * once on mount. Other surfaces (App, modals) keep ownership of their
 * own listeners; this module focuses on the chat-route bindings called
 * out by the story.
 */

import type { Shortcut } from "$lib/stores/shortcuts";

export type Modifier = "mod" | "shift" | "alt";

export interface Chord {
  /** `event.code` (e.g. `KeyN`, `Slash`, `Enter`). Required. */
  code: string;
  /** Required modifiers — order independent. */
  modifiers?: Modifier[];
  /** Modifiers that MUST NOT be pressed (e.g. `shift` for plain `KeyJ`). */
  forbid?: Modifier[];
}

export interface ShortcutBinding {
  id: string;
  chord: Chord;
  /** Logical group — passed through to the help dialog. */
  group: string;
  /** i18n key for the help dialog description. */
  descriptionKey: string;
  /** Visible key chips for the help dialog. */
  keys: string[];
  /** When true, the chord still fires while a textarea/input is focused. */
  allowInTextarea?: boolean;
  /** Handler — return `true` to mark `event.preventDefault()`. */
  run: (event: KeyboardEvent) => boolean | void;
}

const isMac =
  typeof navigator !== "undefined" && navigator.platform.toLowerCase().includes("mac");

export const MOD_LABEL = isMac ? "⌘" : "Ctrl";

function modifierActive(event: KeyboardEvent, modifier: Modifier): boolean {
  switch (modifier) {
    case "mod":
      return isMac ? event.metaKey : event.ctrlKey;
    case "shift":
      return event.shiftKey;
    case "alt":
      return event.altKey;
  }
}

export function chordMatches(event: KeyboardEvent, chord: Chord): boolean {
  if (event.code !== chord.code) return false;
  for (const m of chord.modifiers ?? []) {
    if (!modifierActive(event, m)) return false;
  }
  for (const m of chord.forbid ?? []) {
    if (modifierActive(event, m)) return false;
  }
  // No silent extra modifiers — a binding without `shift` must not match
  // `Shift+code`. The exception is when the binding explicitly lists it
  // in `modifiers` (handled above).
  const required = new Set<Modifier>(chord.modifiers ?? []);
  for (const m of ["mod", "shift", "alt"] as Modifier[]) {
    if (chord.forbid?.includes(m)) continue;
    if (modifierActive(event, m) && !required.has(m)) return false;
  }
  return true;
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLInputElement) {
    // Skip non-text inputs (checkbox/radio/button/file …) — they don't
    // capture printable keys.
    const t = target.type;
    return t !== "checkbox" && t !== "radio" && t !== "button" && t !== "submit";
  }
  return target.isContentEditable;
}

/**
 * Install the singleton chat keydown listener for `bindings`.
 *
 * Returns an unregister function that detaches the listener and clears
 * the registered bindings — call it from `onMount` cleanup.
 */
export function installChatShortcuts(bindings: ShortcutBinding[]): () => void {
  function handler(event: KeyboardEvent): void {
    const editable = isEditableTarget(event.target);
    for (const binding of bindings) {
      if (!chordMatches(event, binding.chord)) continue;
      if (editable && !binding.allowInTextarea) {
        // Cmd/Ctrl chords stay enabled inside textareas — operators expect
        // ⌘K to open the palette even while typing.
        const hasMod = binding.chord.modifiers?.includes("mod");
        if (!hasMod) continue;
      }
      const consumed = binding.run(event);
      if (consumed !== false) event.preventDefault();
      return;
    }
  }
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}

/**
 * Translate a binding to the lightweight `Shortcut` shape consumed by
 * `ShortcutsHelpDialog`. Pure helper — no side effects.
 */
export function describeBinding(binding: ShortcutBinding): Shortcut {
  return {
    id: binding.id,
    keys: binding.keys,
    descriptionKey: binding.descriptionKey,
    group: binding.group,
  };
}
