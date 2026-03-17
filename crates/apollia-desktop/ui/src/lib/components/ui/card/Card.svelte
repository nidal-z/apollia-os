<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
  }

  let { class: className = "", children, ...restProps }: Props = $props();
</script>

<div
  class={cn("glass-card glass-border rounded-xl text-card-foreground transition-all duration-300 hover:shadow-[0_8px_40px_-8px_rgba(52,53,245,0.13)] hover:-translate-y-0.5", className)}
  {...restProps}
>
  {@render children?.()}
</div>
