/**
 * Slash-command registry for the chat input.
 *
 * Commands are declarative - the input matches the current prefix against
 * `name` (and optional `aliases`), the menu component renders them, and
 * the parent component receives an `oncommand(id)` callback to dispatch.
 */
export interface SlashCommand {
  /** Stable id used by the parent dispatcher. */
  id: "export" | "rename";
  /** Command name shown in the menu, without leading slash. */
  name: string;
  /** Translation key for the human label. */
  labelKey: string;
  /** Translation key for the one-line description. */
  descriptionKey: string;
  /** Optional alternate names (also without the slash). */
  aliases?: string[];
}

export const SLASH_COMMANDS: readonly SlashCommand[] = [
  { id: "export", name: "export", labelKey: "chat.commands.export.label", descriptionKey: "chat.commands.export.description" },
  { id: "rename", name: "rename", labelKey: "chat.commands.rename.label", descriptionKey: "chat.commands.rename.description" },
];

/** Detect a leading slash-command prefix in the input (cursor position ignored). */
export function detectSlashPrefix(value: string): string | null {
  const trimmed = value.trimStart();
  if (!trimmed.startsWith("/")) return null;
  const firstLine = trimmed.split(/\s|\n/)[0] ?? "";
  return firstLine.slice(1);
}

export function filterCommands(prefix: string): SlashCommand[] {
  const p = prefix.toLowerCase();
  if (p === "") return [...SLASH_COMMANDS];
  return SLASH_COMMANDS.filter((c) =>
    c.name.startsWith(p) || (c.aliases ?? []).some((a) => a.startsWith(p)),
  );
}
