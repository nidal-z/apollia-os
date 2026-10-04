<script lang="ts">
  /**
   * One item of a tool body's list: an icon, a title and a quiet line under
   * it (a result and its site, a file and its size, a memory and its date).
   */
  import type { Snippet } from "svelte";
  import type { ToolDisplayInfo } from "$lib/tools/tool-display";

  interface Props {
    icon: ToolDisplayInfo["icon"];
    title?: string;
    sub?: string;
    /** Custom title markup, in place of `title`. */
    children?: Snippet;
    /** A finished item reads quieter (a completed task). */
    muted?: boolean;
  }

  let { icon: Icon, title = "", sub = "", children, muted = false }: Props = $props();
</script>

<div class="flex min-w-0 items-start gap-2.5">
  <span class="mt-0.5 flex-none text-muted-foreground" aria-hidden="true">
    <Icon class="h-3.5 w-3.5" />
  </span>
  <div class="min-w-0 flex-1">
    <p class="truncate text-body-xs {muted ? 'text-muted-foreground' : 'text-foreground'}">
      {#if children}{@render children()}{:else}{title}{/if}
    </p>
    {#if sub}
      <p class="truncate text-caption text-muted-foreground">{sub}</p>
    {/if}
  </div>
</div>
