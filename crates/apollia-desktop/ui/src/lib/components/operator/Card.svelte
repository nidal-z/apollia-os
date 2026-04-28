<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
    /** Apply hover-lift micro-interaction. */
    hover?: boolean;
    class?: string;
  }

  let { children, hover = false, class: className = "" }: Props = $props();
</script>

<div
  class="bg-card border border-border rounded-xl transition-all {hover
    ? 'hover-lift cursor-pointer'
    : ''} {className}"
>
  {@render children()}
</div>
