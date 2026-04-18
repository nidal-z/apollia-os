<script lang="ts">
  /**
   * LoadingShimmer — translucent band sliding across a muted background.
   * Use for content-sized placeholders (lines of text, cards, rows).
   *
   * Driven by `@keyframes shimmerSlide` in app.css. Honours
   * `prefers-reduced-motion` via the global media rule.
   */
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    width?: string;
    height?: string;
    /** Round the corners more / less to match the surrounding design. */
    radius?: "sm" | "md" | "lg" | "full";
  }

  let {
    class: className = "",
    width,
    height = "0.75rem",
    radius = "md",
    ...restProps
  }: Props = $props();

  const radiusClass = $derived(({
    sm: "rounded-sm",
    md: "rounded-md",
    lg: "rounded-lg",
    full: "rounded-full",
  } as const)[radius]);
</script>

<div
  class={cn(
    "relative isolate overflow-hidden bg-muted/60",
    radiusClass,
    className,
  )}
  style:width={width}
  style:height={height}
  aria-hidden="true"
  data-testid="loading-shimmer"
  {...restProps}
>
  <div
    class="absolute inset-0 animate-shimmer-slide bg-gradient-to-r from-transparent via-foreground/10 to-transparent motion-reduce:hidden"
  ></div>
</div>
