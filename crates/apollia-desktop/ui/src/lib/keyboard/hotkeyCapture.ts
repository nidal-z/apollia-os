/**
 * Hotkey capture utilities — parses `KeyboardEvent` into a canonical combo
 * string (`"ctrl+shift+space"`), validates it, and flags collisions against a
 * small list of reserved OS/app shortcuts.
 *
 * Introduced for US-SP42-075 (STT settings refonte + Hotkey capture modal).
 */

export const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

const MOD_MAP: Record<string, ModifierName> = {
  Control: "ctrl",
  Shift: "shift",
  Alt: "alt",
  Meta: "meta",
};

export type ModifierName = "ctrl" | "shift" | "alt" | "meta";

/** Keys that must never be captured as the "main" key of a combo. */
const SKIP_KEYS = new Set(["CapsLock", "Dead", "Unidentified", "Process"]);

const NAMED_KEYS: Record<string, string> = {
  Space: "space",
  Enter: "enter",
  Backspace: "backspace",
  Tab: "tab",
  Delete: "delete",
  Insert: "insert",
  Escape: "escape",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Pause: "pause",
  PrintScreen: "print",
};

/** Canonical combo strings reserved by the OS or the app itself. */
export const RESERVED_COMBOS: readonly string[] = [
  "meta+q",
  "meta+w",
  "meta+tab",
  "meta+space",
  "meta+,",
  "meta+c",
  "meta+v",
  "meta+x",
  "meta+a",
  "meta+z",
];

export interface ParsedHotkey {
  /** Held modifiers, in canonical order. */
  modifiers: ModifierName[];
  /** The non-modifier key (e.g. `"space"`), or `null` while only modifiers are held. */
  key: string | null;
  /** Canonical `"ctrl+shift+space"` form. Empty string if neither modifier nor key is present. */
  combo: string;
}

/** Normalize `event.code` → canonical key token (e.g. `KeyP` → `p`, `Digit1` → `1`). */
export function normalizeKey(code: string, fallbackKey: string): string {
  if (code.startsWith("Key")) return code.slice(3).toLowerCase();
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Numpad")) return "numpad" + code.slice(6).toLowerCase();
  if (/^F\d+$/.test(code)) return code.toLowerCase();
  const named = NAMED_KEYS[code];
  if (named) return named;
  return fallbackKey.toLowerCase();
}

/** Parse a raw keyboard event into a canonical hotkey. */
export function parseHotkeyEvent(event: KeyboardEvent): ParsedHotkey {
  const modifiers: ModifierName[] = [];
  if (event.ctrlKey) modifiers.push("ctrl");
  if (event.shiftKey) modifiers.push("shift");
  if (event.altKey) modifiers.push("alt");
  if (event.metaKey) modifiers.push("meta");

  const isModifier = event.key in MOD_MAP || MODIFIER_KEYS.has(event.key);
  if (isModifier) {
    return { modifiers, key: null, combo: modifiers.join("+") };
  }
  if (SKIP_KEYS.has(event.key)) {
    return { modifiers, key: null, combo: modifiers.join("+") };
  }

  const key = normalizeKey(event.code, event.key);
  const combo = [...modifiers, key].join("+");
  return { modifiers, key, combo };
}

/** Whether the parsed hotkey is a complete, saveable combo. */
export function isValidHotkey(hotkey: ParsedHotkey): boolean {
  return hotkey.modifiers.length >= 1 && hotkey.key !== null;
}

/** Whether the combo is known to collide with an OS or app shortcut. */
export function detectCollision(combo: string): boolean {
  if (!combo) return false;
  return RESERVED_COMBOS.includes(combo.toLowerCase());
}

const MAC_DISPLAY: Record<string, string> = {
  ctrl: "⌃",
  shift: "⇧",
  alt: "⌥",
  meta: "⌘",
};

const WIN_DISPLAY: Record<string, string> = {
  ctrl: "Ctrl",
  shift: "Shift",
  alt: "Alt",
  meta: "Win",
};

const MAC_KEY_DISPLAY: Record<string, string> = {
  space: "Space",
  enter: "↵",
  backspace: "⌫",
  tab: "⇥",
  escape: "Esc",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
};

/** Pretty-print a canonical combo for display (`"meta+shift+space"` → `"⌘⇧Space"` on mac). */
export function formatCombo(combo: string, platform: "mac" | "other" = detectPlatform()): string {
  if (!combo) return "";
  const tokens = combo.split("+");
  const map = platform === "mac" ? MAC_DISPLAY : WIN_DISPLAY;
  const keyMap = platform === "mac" ? MAC_KEY_DISPLAY : {};
  return tokens
    .map((token) => {
      if (map[token]) return map[token];
      if (keyMap[token]) return keyMap[token];
      if (token.length === 1) return token.toUpperCase();
      return token.charAt(0).toUpperCase() + token.slice(1);
    })
    .join(platform === "mac" ? "" : "+");
}

/** Build a chip list (one chip per modifier + one for the key) for UI rendering. */
export function hotkeyChips(hotkey: ParsedHotkey, platform: "mac" | "other" = detectPlatform()): string[] {
  const map = platform === "mac" ? MAC_DISPLAY : WIN_DISPLAY;
  const keyMap = platform === "mac" ? MAC_KEY_DISPLAY : {};
  const chips = hotkey.modifiers.map((m) => map[m] ?? m);
  if (hotkey.key) {
    chips.push(keyMap[hotkey.key] ?? (hotkey.key.length === 1 ? hotkey.key.toUpperCase() : hotkey.key));
  }
  return chips;
}

/** Best-effort platform detection. Returns `"mac"` when running on macOS. */
export function detectPlatform(): "mac" | "other" {
  if (typeof navigator === "undefined") return "other";
  return /Mac|iPod|iPhone|iPad/.test(navigator.platform) ? "mac" : "other";
}
