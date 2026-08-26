<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
    count?: number | string;
    action?: Snippet;
  }

  let { children, count, action }: Props = $props();
</script>

<div
  class="flex items-baseline justify-between px-8 pt-6 pb-2.5"
>
  <div class="flex items-baseline gap-2">
    <h3
      class="m-0 text-caption font-semibold tracking-[1.5px] text-muted-foreground uppercase font-mono"
    >
      {@render children()}
    </h3>
    {#if count !== undefined}
      <span class="text-caption text-muted-foreground/70 font-mono">{count}</span>
    {/if}
  </div>
  {#if action}
    {@render action()}
  {/if}
</div>
