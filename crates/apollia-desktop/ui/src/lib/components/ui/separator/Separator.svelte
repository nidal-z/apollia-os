<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  type SeparatorColor = "border" | "muted" | "primary";

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
    /** Applies `mx-6` (horizontal) or `my-6` (vertical). */
    inset?: boolean;
    /** Renders as a 1px dashed border instead of a filled bar. */
    dashed?: boolean;
    /** Token color for solid/dashed rendering. Ignored for `fade`/`elevated`. */
    color?: SeparatorColor;
  }

  let {
    class: className = "",
    orientation = "horizontal",
    variant = "solid",
    inset = false,
    dashed = false,
    color = "border",
    ...restProps
  }: Props = $props();

  const horizontal = $derived(orientation === "horizontal");
  const insetCls = $derived(inset ? (horizontal ? "mx-6" : "my-6") : "");

  const bgByColor: Record<SeparatorColor, string> = {
    border: "bg-border",
    muted: "bg-muted",
    primary: "bg-primary",
  };
  const borderByColor: Record<SeparatorColor, string> = {
    border: "border-border",
    muted: "border-muted",
    primary: "border-primary",
  };
</script>

{#if variant === "fade"}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0",
      horizontal ? "divider-fade w-full" : "divider-fade-vertical self-stretch",
      insetCls,
      className,
    )}
    {...restProps}
  ></div>
{:else if dashed}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0",
      horizontal
        ? "h-0 w-full border-t border-dashed"
        : "h-full w-0 border-l border-dashed self-stretch",
      borderByColor[variant === "subtle" ? "border" : color],
      variant === "subtle" && "opacity-50",
      insetCls,
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
      insetCls,
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
      insetCls,
      className,
    )}
    {...restProps}
  ></div>
{:else}
  <div
    role="separator"
    aria-orientation={orientation}
    class={cn(
      "shrink-0",
      bgByColor[color],
      horizontal ? "h-px w-full" : "h-full w-px",
      insetCls,
      className,
    )}
    {...restProps}
  ></div>
{/if}
