<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
  }

  let { class: className = "", children, ...restProps }: Props = $props();
</script>

<div
  class={cn("rounded-lg border bg-card text-card-foreground shadow-sm transition-shadow duration-200 hover:shadow-md", className)}
  {...restProps}
>
  {@render children?.()}
</div>
