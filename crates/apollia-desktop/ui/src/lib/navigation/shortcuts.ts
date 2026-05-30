/**
 * Keyboard shortcut registry.
 *
 * The canonical list of user-facing shortcuts shown on the
 * `/settings/shortcuts` page and surfaced in the Command Palette.
 * This module is presentational only — each entry is the keycap rendering
 * for an action owned elsewhere (command palette, chat, companion…).
 *
 * Keeping the table here (instead of on each feature) means we only have
 * one surface to audit for overlaps and renames.
 */

export type ShortcutCategory =
  | "global"
  | "navigation"
  | "chat"
  | "settings"
  | "companion"
  | "approvals";

export type ShortcutScope = "global" | "contextual";

export interface ShortcutCombo {
  /** macOS chord, split by `+`. Use `⌘`, `⌥`, `⌃`, `⇧` for modifiers. */
  mac: string;
  /** Windows/Linux chord, split by `+`. */
  win: string;
}

export interface ShortcutEntry {
  /** Stable id — used by the palette, tests, and ARIA labels. */
  id: string;
  /** Chord representation per platform. */
  combo: ShortcutCombo;
  /** Translated-label i18n key — resolved at render time. */
  descriptionKey: string;
  /** Whether the shortcut is listened to globally or only in a scope. */
  scope: ShortcutScope;
  category: ShortcutCategory;
  /** Optional scope hint (e.g. "Inside Chat"), i18n key. */
  scopeHintKey?: string;
}

export const SHORTCUT_CATEGORIES: ShortcutCategory[] = [
  "global",
  "navigation",
  "chat",
  "settings",
  "companion",
  "approvals",
];

export const SHORTCUTS: ShortcutEntry[] = [
  // ── Global ─────────────────────────────────────────────────────────
  {
    id: "palette.open",
    combo: { mac: "⌘+K", win: "Ctrl+K" },
    descriptionKey: "settings.shortcuts.items.palette_open",
    scope: "global",
    category: "global",
  },
  {
    id: "help.overlay",
    combo: { mac: "?", win: "?" },
    descriptionKey: "settings.shortcuts.items.help_overlay",
    scope: "global",
    category: "global",
  },
  {
    id: "app.quit",
    combo: { mac: "⌘+Q", win: "Ctrl+Q" },
    descriptionKey: "settings.shortcuts.items.app_quit",
    scope: "global",
    category: "global",
  },

  // ── Navigation ─────────────────────────────────────────────────────
  {
    id: "nav.back",
    combo: { mac: "⌘+[", win: "Ctrl+[" },
    descriptionKey: "settings.shortcuts.items.nav_back",
    scope: "global",
    category: "navigation",
  },
  {
    id: "nav.forward",
    combo: { mac: "⌘+]", win: "Ctrl+]" },
    descriptionKey: "settings.shortcuts.items.nav_forward",
    scope: "global",
    category: "navigation",
  },
  {
    id: "nav.sidebar",
    combo: { mac: "⌘+B", win: "Ctrl+B" },
    descriptionKey: "settings.shortcuts.items.nav_sidebar",
    scope: "global",
    category: "navigation",
  },
  {
    id: "nav.fields",
    combo: { mac: "Tab / ⇧+Tab", win: "Tab / Shift+Tab" },
    descriptionKey: "settings.shortcuts.items.nav_fields",
    scope: "contextual",
    category: "navigation",
  },

  // ── Chat ───────────────────────────────────────────────────────────
  {
    id: "chat.new",
    combo: { mac: "⌘+K", win: "Ctrl+K" },
    descriptionKey: "settings.shortcuts.items.chat_new",
    scope: "contextual",
    category: "chat",
    scopeHintKey: "settings.shortcuts.scope.chat",
  },
  {
    id: "chat.send",
    combo: { mac: "⌘+Enter", win: "Ctrl+Enter" },
    descriptionKey: "settings.shortcuts.items.chat_send",
    scope: "contextual",
    category: "chat",
    scopeHintKey: "settings.shortcuts.scope.chat",
  },
  {
    id: "chat.artifacts",
    combo: { mac: "⌘+⇧+A", win: "Ctrl+Shift+A" },
    descriptionKey: "settings.shortcuts.items.chat_artifacts",
    scope: "contextual",
    category: "chat",
    scopeHintKey: "settings.shortcuts.scope.chat",
  },
  {
    id: "chat.stop",
    combo: { mac: "Escape", win: "Escape" },
    descriptionKey: "settings.shortcuts.items.chat_stop",
    scope: "contextual",
    category: "chat",
    scopeHintKey: "settings.shortcuts.scope.chat",
  },

  // ── Settings ───────────────────────────────────────────────────────
  {
    id: "settings.save",
    combo: { mac: "⌘+S", win: "Ctrl+S" },
    descriptionKey: "settings.shortcuts.items.settings_save",
    scope: "contextual",
    category: "settings",
    scopeHintKey: "settings.shortcuts.scope.settings",
  },
  {
    id: "settings.close",
    combo: { mac: "Escape", win: "Escape" },
    descriptionKey: "settings.shortcuts.items.settings_close",
    scope: "contextual",
    category: "settings",
    scopeHintKey: "settings.shortcuts.scope.dialog",
  },

  // ── Companion ──────────────────────────────────────────────────────
  {
    id: "companion.toggle",
    combo: { mac: "⌘+/", win: "Ctrl+/" },
    descriptionKey: "settings.shortcuts.items.companion_toggle",
    scope: "global",
    category: "companion",
  },
  {
    id: "companion.move",
    combo: { mac: "⌘+⌥+Arrows", win: "Ctrl+Alt+Arrows" },
    descriptionKey: "settings.shortcuts.items.companion_move",
    scope: "global",
    category: "companion",
  },
  {
    id: "companion.focus",
    combo: { mac: "⌘+⇧+C", win: "Ctrl+Shift+C" },
    descriptionKey: "settings.shortcuts.items.companion_focus",
    scope: "global",
    category: "companion",
  },

  // ── Approvals ──────────────────────────────────────────────────────
  {
    id: "approvals.accept",
    combo: { mac: "A", win: "A" },
    descriptionKey: "settings.shortcuts.items.approvals_accept",
    scope: "contextual",
    category: "approvals",
    scopeHintKey: "settings.shortcuts.scope.approvals",
  },
  {
    id: "approvals.refuse",
    combo: { mac: "R", win: "R" },
    descriptionKey: "settings.shortcuts.items.approvals_refuse",
    scope: "contextual",
    category: "approvals",
    scopeHintKey: "settings.shortcuts.scope.approvals",
  },
  {
    id: "approvals.next",
    combo: { mac: "J / K", win: "J / K" },
    descriptionKey: "settings.shortcuts.items.approvals_next",
    scope: "contextual",
    category: "approvals",
    scopeHintKey: "settings.shortcuts.scope.approvals",
  },
];

/** `true` when running on macOS (best-effort browser detection). */
export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  // navigator.platform is deprecated but still the most reliable signal across
  // engines; userAgent is the documented fallback.
  const platform = (navigator as Navigator & { platform?: string }).platform ?? ""; // NOSONAR typescript:S1874
  return /Mac|iPhone|iPad/i.test(platform || navigator.userAgent || "");
}

/** Return the platform-appropriate combo string for a shortcut. */
export function comboFor(entry: ShortcutEntry, mac = isMacPlatform()): string {
  return mac ? entry.combo.mac : entry.combo.win;
}

/** Split a chord into its individual keycaps (e.g. "⌘+⇧+P" → ["⌘","⇧","P"]). */
export function splitCombo(combo: string): string[] {
  return combo
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
}

/**
 * Case-insensitive fuzzy-ish filter: matches if all whitespace-separated
 * terms appear somewhere in either the description or the combo.
 */
export function filterShortcuts(
  entries: ShortcutEntry[],
  query: string,
  translate: (key: string) => string,
  mac = isMacPlatform(),
): ShortcutEntry[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return entries;
  return entries.filter((entry) => {
    const haystack = [
      translate(entry.descriptionKey),
      entry.combo.mac,
      entry.combo.win,
      entry.id,
      mac ? "" : "ctrl",
    ]
      .join(" ")
      .toLowerCase();
    return terms.every((term) => haystack.includes(term));
  });
}
