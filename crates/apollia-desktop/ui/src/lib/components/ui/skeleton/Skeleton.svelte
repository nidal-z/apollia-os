<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    width?: string;
    height?: string;
  }

  let { class: className = "", width, height, ...restProps }: Props = $props();
</script>

<div
  class={cn("animate-pulse rounded-md bg-muted", className)}
  style:width={width}
  style:height={height}
  {...restProps}
></div>
