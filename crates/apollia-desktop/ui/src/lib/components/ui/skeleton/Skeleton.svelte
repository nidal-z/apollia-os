<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  type Variant = "plain" | "text" | "card" | "table-row" | "avatar";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    width?: string;
    height?: string;
    /**
     * Canonical skeleton shape (US-SP42-009).
     * - `plain`   — bare pulsing rect (legacy default).
     * - `text`    — single line of text, 1rem tall.
     * - `card`    — block sized like a card header.
     * - `table-row` — full-width row, ~2.5rem tall.
     * - `avatar`  — circular, 2.25rem default.
     */
    variant?: Variant;
  }

  let {
    class: className = "",
    width,
    height,
    variant = "plain",
    ...restProps
  }: Props = $props();

  const variantClass = $derived(({
    plain: "",
    text: "h-4 w-full rounded-md",
    card: "h-24 w-full rounded-xl",
    "table-row": "h-10 w-full rounded-md",
    avatar: "h-9 w-9 rounded-full",
  } as const)[variant]);
</script>

<div
  class={cn("animate-pulse rounded-md bg-muted", variantClass, className)}
  style:width={width}
  style:height={height}
  aria-hidden="true"
  {...restProps}
></div>
