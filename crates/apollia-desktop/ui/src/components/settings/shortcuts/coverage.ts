/**
 * Read-only coverage checks for the shortcuts catalog.
 *
 * The `/settings/shortcuts` page is a keycap reference, not an editor. These
 * helpers surface honest, computed facts about each catalog entry so the page
 * can flag them without fabricating any rebind capability:
 *
 *  - `isReserved`      the chord is claimed by the OS (see `RESERVED_COMBOS`);
 *                      it can be displayed but never reassigned.
 *  - `overlapsGlobal`  a contextual entry shares its chord with a global one
 *                      (e.g. chat.new vs palette.open, both `Cmd+K`).
 *
 * Everything is derived from the presentational catalog; nothing here listens
 * to the keyboard or mutates state.
 */
import {
  SHORTCUTS,
  splitCombo,
  type ShortcutEntry,
} from "$lib/navigation/shortcuts";
import { RESERVED_COMBOS } from "$lib/keyboard/hotkeyCapture";

const MOD_TO_CANONICAL: Record<string, string> = {
  "⌘": "meta",
  Cmd: "meta",
  Command: "meta",
  Win: "meta",
  "⌃": "ctrl",
  Ctrl: "ctrl",
  Control: "ctrl",
  "⌥": "alt",
  Alt: "alt",
  Option: "alt",
  "⇧": "shift",
  Shift: "shift",
};

/** Map a single chord alternative to `hotkeyCapture` canonical form. */
function canonical(combo: string): string {
  return splitCombo(combo)
    .map((token) => MOD_TO_CANONICAL[token] ?? token.toLowerCase())
    .join("+");
}

/** Expand a catalog combo (may hold `X / Y` alternatives) into canonical forms. */
function canonicalForms(combo: string): string[] {
  return combo
    .split("/")
    .map((alt) => canonical(alt.trim()))
    .filter(Boolean);
}

const GLOBAL_CHORDS: ReadonlySet<string> = new Set(
  SHORTCUTS.filter((entry) => entry.scope === "global").flatMap((entry) =>
    canonicalForms(entry.combo.mac),
  ),
);

/** `true` when either platform chord is reserved by the operating system. */
export function isReserved(entry: ShortcutEntry): boolean {
  return [entry.combo.mac, entry.combo.win]
    .flatMap(canonicalForms)
    .some((form) => RESERVED_COMBOS.includes(form));
}

/**
 * `true` when a contextual entry shares its chord with a global shortcut. The
 * dispatcher lets the contextual handler win while its surface is mounted, but
 * the collision is worth showing rather than hiding.
 */
export function overlapsGlobal(entry: ShortcutEntry): boolean {
  if (entry.scope !== "contextual") return false;
  return canonicalForms(entry.combo.mac).some((form) => GLOBAL_CHORDS.has(form));
}
