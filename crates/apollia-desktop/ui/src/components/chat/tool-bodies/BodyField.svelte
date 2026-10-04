<script lang="ts">
  /**
   * One labelled value at the head of a tool body: what was searched, read,
   * listed or called. The label is quiet, the value is the content.
   */
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    value?: string;
    /** Render the value in the monospace ramp (paths, patterns, requests). */
    mono?: boolean;
    /** Custom value markup, in place of `value`. */
    children?: Snippet;
  }

  let { label, value = "", mono = false, children }: Props = $props();
</script>

<div class="flex min-w-0 items-baseline gap-2">
  <span class="flex-none text-caption text-muted-foreground">{label}</span>
  <span
    class="min-w-0 truncate text-body-xs text-foreground {mono ? 'font-mono' : ''}"
    title={value || undefined}
  >
    {#if children}{@render children()}{:else}{value}{/if}
  </span>
</div>
