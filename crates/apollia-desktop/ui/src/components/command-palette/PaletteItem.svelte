<script lang="ts">
  /**
   * App-level palette row. Thin wrapper over the headless
   * `CommandItem` primitive — exposed so future iterations can customise
   * the row (session avatars, recency hints) without touching the
   * generic primitive.
   */
  import CommandItemRow from "$lib/components/ui/command/CommandItem.svelte";
  import type { CommandItem } from "$lib/stores/commandPalette";

  interface Props {
    id: string;
    item: CommandItem;
    active: boolean;
    onselect: () => void;
    onhover: () => void;
  }

  let { id, item, active, onselect, onhover }: Props = $props();
</script>

<CommandItemRow {id} {item} {active} {onselect} {onhover} />
