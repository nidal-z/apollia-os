/**
 * Command palette registry.
 *
 * Components register actions into this store; `Command.svelte` consumes
 * the derived `$commands` list, filters it via fuzzy search, and executes
 * the selected `action` on Enter.
 *
 * Registration is idempotent per `id` - re-registering a command with
 * the same id overwrites the previous entry. Unregistering is expected
 * in the component's `onMount` cleanup.
 */
import { writable, derived, get } from "svelte/store";
// NOSONAR typescript:S1874 - Svelte 4 ComponentType retained for lucide-svelte compat
import type { ComponentType } from "svelte"; // NOSONAR typescript:S1874

/**
 * Local alias for Svelte 4 `ComponentType`. Centralizes deprecation
 * suppression - lucide-svelte's icon exports rely on this legacy alias.
 */
// NOSONAR typescript:S1874
type SvelteComponentType = ComponentType; // NOSONAR typescript:S1874

export interface CommandItem {
  /** Stable id used to dedupe registrations (e.g. `nav.agents`). */
  id: string;
  /** Rendered label (translated string, not a key). */
  label: string;
  /** Optional secondary line shown in muted foreground. */
  description?: string;
  /** Optional lucide icon component rendered at 14px. */
  icon?: SvelteComponentType;
  /** Optional key chips shown on the right (`["Cmd", "K"]`). */
  shortcut?: string[];
  /** Logical group name used to bucket items under a sticky header. */
  group: string;
  /** Extra search tokens that count against fuzzy matching. */
  keywords?: string[];
  /** Invoked when the user confirms this item. */
  action: () => void | Promise<void>;
}

const registry = writable<Record<string, CommandItem>>({});

/** All currently-registered commands, in insertion order per group. */
export const commands = derived(registry, ($reg) => Object.values($reg));

/** Controls the palette's open/closed state. Components bind or set. */
export const commandPaletteOpen = writable<boolean>(false);

/** Register one or more commands. Returns an unregister function. */
export function registerCommand(
  ...entries: CommandItem[]
): () => void {
  registry.update((current) => {
    const next = { ...current };
    for (const entry of entries) next[entry.id] = entry;
    return next;
  });
  return () => unregisterCommand(...entries.map((e) => e.id));
}

/** Remove commands by id. Unknown ids are ignored. */
export function unregisterCommand(...ids: string[]): void {
  registry.update((current) => {
    const next = { ...current };
    for (const id of ids) delete next[id];
    return next;
  });
}

/** Snapshot accessor for non-reactive callers (tests, telemetry). */
export function listCommands(): CommandItem[] {
  return Object.values(get(registry));
}

/** Open the palette from anywhere (e.g. sidebar button). */
export function openCommandPalette(): void {
  commandPaletteOpen.set(true);
}

/** Close the palette from anywhere. */
export function closeCommandPalette(): void {
  commandPaletteOpen.set(false);
}
