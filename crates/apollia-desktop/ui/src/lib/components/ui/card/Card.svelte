<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    interactive?: boolean;
  }

  let { class: className = "", interactive = false, children, ...restProps }: Props = $props();
</script>

<div
  class={cn(
    interactive ? "glass-card-hover" : "glass-card",
    "glass-border rounded-xl text-card-foreground",
    className
  )}
  {...restProps}
>
  {@render children?.()}
</div>
