<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLHeadingElement> {
    class?: string;
  }

  let { class: className = "", children, ...restProps }: Props = $props();
</script>

<h3 class={cn("text-2xl font-medium leading-none tracking-tight", className)} {...restProps}>
  {@render children?.()}
</h3>
