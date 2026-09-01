<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes, HTMLButtonAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";

  /**
   * Badge - unified variants (+).
   *
   * Canonical variants: `neutral | primary | success | warning | danger | info`.
   * Premium variants: `gradient-primary | gradient-success | gradient-warning | gradient-destructive`.
   * Legacy aliases kept so the 40+ pre-existing call-sites keep working:
   *   `default`     → `primary`
   *   `secondary`   → `neutral`
   *   `destructive` → `danger`
   *   `outline`     → outline rendering
   *
   * Render mode :
   * - default            → `<div>` (decorative badge).
   * - with `onclick` prop → `<button>` (interactive filter chip / toggle).
   */
  type Variant =
    | "neutral"
    | "primary"
    | "success"
    | "warning"
    | "danger"
    | "info"
    | "outline"
    | "gradient-primary"
    | "gradient-success"
    | "gradient-warning"
    | "gradient-destructive"
    // legacy aliases - do not remove without migrating call-sites first.
    | "default"
    | "secondary"
    | "destructive";

  type Size = "sm" | "md" | "lg";

  // The Badge supports two render modes : <div> when decorative, <button> when
  // an `onclick` handler is provided. Both DOM event maps are merged loosely
  // here so call-sites can pass any standard attribute without TS complaining.
  type Props = {
    class?: string;
    variant?: Variant;
    size?: Size;
    /**
     * Outline mode - drops the variant's filled background and keeps only the
     * variant's foreground colour against a transparent surface with a
     * `border-border` ring. Lets a "primary outline" badge coexist with a
     * "primary filled" badge without duplicating variants.
     */
    outline?: boolean;
    icon?: Snippet;
    children?: Snippet;
    /**
     * When provided, the Badge renders as a `<button>` with focus ring,
     * cursor-pointer and hover state - for filter chips and toggles.
     * When absent, the Badge stays a decorative `<div>`.
     */
    onclick?: (e: MouseEvent) => void;
  } & Omit<HTMLButtonAttributes, "class" | "onclick">
    & Omit<HTMLAttributes<HTMLElement>, "class" | "onclick">;

  // Shared inset rim for gradient variants - white highlight at top.
  const gradientInset = "shadow-[inset_0_1px_0_rgba(255,255,255,0.5)]";

  const variantClasses: Record<Variant, string> = {
    neutral: "bg-muted text-muted-foreground",
    primary: "bg-primary/10 text-primary dark:bg-primary/20",
    success:
      "bg-success/10 text-success-a11y dark:bg-success/20",
    warning:
      "bg-warning/10 text-warning-a11y dark:bg-warning/20",
    danger:
      "bg-destructive/10 text-danger-a11y dark:bg-destructive/20",
    info: "bg-info/10 text-info dark:bg-info/20",
    outline: "border border-border text-foreground",
    "gradient-primary": `bg-gradient-to-b from-primary/20 to-primary/30 text-primary dark:from-primary/30 dark:to-primary/40 ${gradientInset}`,
    "gradient-success": `bg-gradient-to-b from-success/20 to-success/30 text-success-a11y dark:from-success/30 dark:to-success/40 ${gradientInset}`,
    "gradient-warning": `bg-gradient-to-b from-warning/20 to-warning/30 text-warning-a11y dark:from-warning/30 dark:to-warning/40 ${gradientInset}`,
    "gradient-destructive": `bg-gradient-to-b from-destructive/20 to-destructive/30 text-danger-a11y dark:from-destructive/30 dark:to-destructive/40 ${gradientInset}`,
    default: "bg-primary/10 text-primary dark:bg-primary/20",
    secondary: "bg-muted text-muted-foreground",
    destructive:
      "bg-destructive/10 text-danger-a11y dark:bg-destructive/20",
  };

  // `sm` is the default and carries 95 of the 98 badges of the tree. It sat on
  // `micro`, the lowest tier of the scale, where the config's own px map gives
  // a badge the `caption` role. The three sizes now read 12.5 / 13 / 14.
  // `leading-none` is what keeps a pill the height of its glyph. Without it the
  // tier's own line-height applies, and `caption` carries 1.45: a 12.5px badge
  // measured 24.1px tall beside a 21px title, so the metadata stood taller than
  // the entity it annotates. The sidebar count badge already carried the class;
  // this one did not. The three sizes now measure 18.5 / 23 / 28px.
  const sizeClasses: Record<Size, string> = {
    sm: "text-caption leading-none px-2 py-0.5 gap-1",
    md: "text-xs leading-none px-2.5 py-1 gap-1.5",
    lg: "text-sm leading-none px-3 py-1.5 gap-1.5",
  };

  // Foreground-only classes for outline mode. Mirrors the `fg` slot of the
  // legacy operator/Chip.svelte so consumers migrating from Chip preserve their
  // tone-coloured text against a transparent surface.
  const outlineForeground: Record<Variant, string> = {
    neutral: "text-muted-foreground",
    primary: "text-primary",
    success: "text-success-a11y",
    warning: "text-warning-a11y",
    danger: "text-danger-a11y",
    info: "text-info",
    outline: "text-foreground",
    "gradient-primary": "text-primary",
    "gradient-success": "text-success-a11y",
    "gradient-warning": "text-warning-a11y",
    "gradient-destructive": "text-danger-a11y",
    default: "text-primary",
    secondary: "text-muted-foreground",
    destructive: "text-danger-a11y",
  };

  let {
    class: className = "",
    variant = "neutral",
    size = "sm",
    outline = false,
    icon,
    children,
    ...restProps
  }: Props = $props();

  const isInteractive = $derived(
    "onclick" in restProps && typeof (restProps as { onclick?: unknown }).onclick === "function",
  );

  const baseClasses = cn(
    "inline-flex items-center rounded-full font-medium transition-colors",
    outline ? "bg-transparent border border-border" : "border border-transparent",
    sizeClasses[size],
    outline ? outlineForeground[variant] : variantClasses[variant],
    isInteractive &&
      "cursor-pointer hover:brightness-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background active:scale-[0.98]",
    className,
  );
</script>

{#if isInteractive}
  <button type="button" class={baseClasses} {...restProps as Record<string, unknown>}>
    {#if icon}
      <span class="inline-flex shrink-0 items-center" aria-hidden="true">
        {@render icon()}
      </span>
    {/if}
    {@render children?.()}
  </button>
{:else}
  <div class={baseClasses} {...restProps as Record<string, unknown>}>
    {#if icon}
      <span class="inline-flex shrink-0 items-center" aria-hidden="true">
        {@render icon()}
      </span>
    {/if}
    {@render children?.()}
  </div>
{/if}
