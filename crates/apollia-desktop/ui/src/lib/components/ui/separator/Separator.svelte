<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    orientation?: "horizontal" | "vertical";
    /**
     * Rendering style.
     * - `solid` (default) — 1px flat line tinted with `--border`.
     * - `fade` — gradient-fade on both ends, for long scrollable regions
     *   where a hard divider feels abrupt (F.42, F.71).
     */
    variant?: "solid" | "fade";
  }

  let {
    class: className = "",
    orientation = "horizontal",
    variant = "solid",
    ...restProps
  }: Props = $props();
</script>

{#if variant === "fade"}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0",
      orientation === "horizontal" ? "divider-fade w-full" : "divider-fade-vertical self-stretch",
      className,
    )}
    {...restProps}
  ></div>
{:else}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0 bg-border",
      orientation === "horizontal" ? "h-[1px] w-full" : "h-full w-[1px]",
      className,
    )}
    {...restProps}
  ></div>
{/if}
