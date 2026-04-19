<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    keys: string[];
    class?: string;
    /** Accessible label if used in isolation from surrounding text. */
    "aria-label"?: string;
  }

  let { keys, class: className = "", ...rest }: Props = $props();
</script>

<span
  class={cn("inline-flex items-center gap-1 text-[11px] text-muted-a11y", className)}
  {...rest}
>
  {#each keys as key, i}
    <kbd
      class="rounded-md border border-border bg-surface-2 px-1.5 py-0.5 font-mono text-[10px] leading-none text-foreground shadow-sm"
    >
      {key}
    </kbd>
    {#if i < keys.length - 1}
      <span aria-hidden="true" class="text-muted-foreground">+</span>
    {/if}
  {/each}
</span>
