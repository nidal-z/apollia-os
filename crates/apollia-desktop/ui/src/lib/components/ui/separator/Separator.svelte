<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    orientation?: "horizontal" | "vertical";
    /**
     * Rendering style.
     * - `solid`    — 1px flat line tinted with `--border`.
     * - `subtle`   — 1px line with --border at 50% for secondary groupings.
     * - `elevated` — 1px line + faint top highlight, sits on surface-1.
     * - `fade`     — gradient-fade on both ends for long scrollable regions
     *   where a hard divider feels abrupt (F.42, F.71).
     */
    variant?: "solid" | "subtle" | "elevated" | "fade";
  }

  let {
    class: className = "",
    orientation = "horizontal",
    variant = "solid",
    ...restProps
  }: Props = $props();

  const horizontal = $derived(orientation === "horizontal");
</script>

{#if variant === "fade"}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0",
      horizontal ? "divider-fade w-full" : "divider-fade-vertical self-stretch",
      className,
    )}
    {...restProps}
  ></div>
{:else if variant === "subtle"}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0 bg-border/50",
      horizontal ? "h-px w-full" : "h-full w-px",
      className,
    )}
    {...restProps}
  ></div>
{:else if variant === "elevated"}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0 bg-border",
      horizontal
        ? "h-px w-full shadow-[0_1px_0_0_hsl(var(--background))]"
        : "h-full w-px shadow-[1px_0_0_0_hsl(var(--background))]",
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
      horizontal ? "h-px w-full" : "h-full w-px",
      className,
    )}
    {...restProps}
  ></div>
{/if}
