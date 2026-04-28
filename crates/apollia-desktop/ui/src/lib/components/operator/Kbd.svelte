<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    dim?: boolean;
    children: Snippet;
  }

  let { dim = false, children }: Props = $props();
</script>

<span
  class="inline-flex items-center justify-center font-mono text-[10px] leading-none px-[5px] py-[3px] rounded font-medium {dim
    ? 'bg-transparent border border-transparent text-muted-foreground/70'
    : 'bg-card/80 border border-border text-muted-foreground'}"
>
  {@render children()}
</span>
