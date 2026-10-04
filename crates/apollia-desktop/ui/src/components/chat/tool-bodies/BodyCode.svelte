<script lang="ts">
  /**
   * A block of literal text inside a tool body: a file's lines, a command and
   * its output, code, a response. Quiet fill, monospace, wrapped.
   */
  import type { Snippet } from "svelte";

  interface Props {
    text?: string;
    /** Custom content (a coloured diff), in place of `text`. */
    children?: Snippet;
  }

  let { text = "", children }: Props = $props();
</script>

<pre
  class="m-0 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md bg-surface-2 px-3 py-2 font-mono text-body-xs leading-relaxed text-foreground"
><code>{#if children}{@render children()}{:else}{text}{/if}</code></pre>
