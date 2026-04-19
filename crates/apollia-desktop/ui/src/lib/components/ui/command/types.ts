import type { CommandItem } from "$lib/stores/commandPalette";

export interface CommandPaletteGroup {
  label: string;
  items: CommandItem[];
}
