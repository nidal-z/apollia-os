/**
 * Keyboard shortcut dispatcher.
 *
 * A single `window` keydown listener that matches the pressed chord against
 * the presentational catalog in `./shortcuts` and invokes the handler
 * registered for that shortcut id. It complements the two existing shortcut
 * modules: `./shortcuts` (the keycap catalog rendered in Settings / the
 * palette) and `$lib/stores/shortcuts` (the reactive registry the `?` overlay
 * lists). Neither of those wires a listener; this module does, once.
 *
 * Handlers are registered here (not in the presentational stores) so the
 * dispatcher owns the full match -> guard -> invoke path:
 *  - scope: a handler may declare `global` or `contextual`; contextual
 *    handlers simply register while their surface is mounted.
 *  - input guard: keystrokes inside an input / textarea / contenteditable /
 *    select are ignored unless the handler opts in with `allowInInput`.
 *
 * The app shell calls `mountShortcutDispatcher()` once and keeps the returned
 * cleanup. Surfaces call `registerShortcutHandler(...)` in an effect and run
 * the returned cleanup on teardown.
 */
import {
  SHORTCUTS,
  comboFor,
  splitCombo,
  type ShortcutEntry,
  type ShortcutScope,
} from "$lib/navigation/shortcuts";

export interface ShortcutHandler {
  /** Catalog entry id from `SHORTCUTS` (e.g. `palette.open`). */
  id: string;
  /** Invoked when the chord matches and guards pass. */
  handler: (event: KeyboardEvent) => void;
  /** Overrides the catalog entry scope. */
  scope?: ShortcutScope;
  /** Fire even when focus is in an input / textarea / contenteditable. */
  allowInInput?: boolean;
}

const CATALOG: Map<string, ShortcutEntry> = new Map(
  SHORTCUTS.map((entry) => [entry.id, entry]),
);

const handlers = new Map<string, ShortcutHandler>();

/**
 * Register a handler for a shortcut id. Returns a cleanup that unregisters it.
 * Re-registering the same id replaces the previous handler.
 */
export function registerShortcutHandler(entry: ShortcutHandler): () => void {
  handlers.set(entry.id, entry);
  return () => {
    if (handlers.get(entry.id) === entry) handlers.delete(entry.id);
  };
}

function isEditableTarget(event: KeyboardEvent): boolean {
  const el = event.target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable
  );
}

function keyEquals(token: string, event: KeyboardEvent): boolean {
  const evKey = event.key;
  if (token === "Enter" || token === "Escape" || token === "Tab") {
    return evKey === token;
  }
  if (token.length === 1 && /[a-z]/i.test(token)) {
    return evKey.toUpperCase() === token.toUpperCase();
  }
  // Symbol keys (?, /, [, ]) compare literally.
  return evKey === token;
}

/** Match one chord alternative (e.g. "Cmd+K") against the event. */
function altMatches(alt: string, event: KeyboardEvent): boolean {
  let needMeta = false;
  let needCtrl = false;
  let needAlt = false;
  let needShift = false;
  let key = "";

  for (const token of splitCombo(alt)) {
    switch (token) {
      case "⌘": // command
      case "Cmd":
        needMeta = true;
        break;
      case "⌃": // control
      case "Ctrl":
        needCtrl = true;
        break;
      case "⌥": // option
      case "Alt":
        needAlt = true;
        break;
      case "⇧": // shift
      case "Shift":
        needShift = true;
        break;
      case "Arrows":
        // Directional combos are informational; not dispatched.
        return false;
      default:
        key = token;
    }
  }

  if (!key) return false;
  if (event.metaKey !== needMeta) return false;
  if (event.ctrlKey !== needCtrl) return false;
  if (event.altKey !== needAlt) return false;
  // Enforce shift only for letters; symbol keys may need shift on some layouts.
  if (key.length === 1 && /[a-z]/i.test(key) && event.shiftKey !== needShift) {
    return false;
  }
  return keyEquals(key, event);
}

/** Whether the event matches a catalog combo string (handles "X / Y" forms). */
function comboMatchesEvent(combo: string, event: KeyboardEvent): boolean {
  return combo
    .split("/")
    .map((part) => part.trim())
    .filter(Boolean)
    .some((alt) => altMatches(alt, event));
}

function onKeydown(event: KeyboardEvent): void {
  const editable = isEditableTarget(event);
  for (const registered of handlers.values()) {
    const entry = CATALOG.get(registered.id);
    if (!entry) continue;
    if (editable && !registered.allowInInput) continue;
    if (comboMatchesEvent(comboFor(entry), event)) {
      event.preventDefault();
      registered.handler(event);
      return;
    }
  }
}

/**
 * Attach the single window listener. Call once from the app shell; run the
 * returned cleanup on teardown. No-op on the server.
 */
export function mountShortcutDispatcher(): () => void {
  if (typeof window === "undefined") return () => {};
  window.addEventListener("keydown", onKeydown);
  return () => window.removeEventListener("keydown", onKeydown);
}
