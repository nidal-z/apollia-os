<script lang="ts">
  /**
   * App-level palette group header. Thin wrapper around the
   * headless `CommandGroup` primitive so app-specific styling or badges
   * (e.g. "Recently used" pill) can be added later without touching the
   * primitive.
   */
  import CommandGroupPrimitive from "$lib/components/ui/command/CommandGroup.svelte";

  interface Props {
    label: string;
    children?: import("svelte").Snippet;
  }

  let { label, children }: Props = $props();
</script>

<CommandGroupPrimitive {label}>
  {@render children?.()}
</CommandGroupPrimitive>
