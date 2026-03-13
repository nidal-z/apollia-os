<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLParagraphElement> {
    class?: string;
  }

  let { class: className = "", children, ...restProps }: Props = $props();
</script>

<p class={cn("text-sm text-muted-foreground", className)} {...restProps}>
  {@render children?.()}
</p>
