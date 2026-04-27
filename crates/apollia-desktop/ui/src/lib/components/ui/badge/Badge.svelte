<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";

  /**
   * Badge — unified variants (+).
   *
   * Canonical variants: `neutral | primary | success | warning | danger | info`.
   * Premium variants: `gradient-primary | gradient-success | gradient-warning | gradient-destructive`.
   * Legacy aliases kept so the 40+ pre-existing call-sites keep working:
   *   `default`     → `primary`
   *   `secondary`   → `neutral`
   *   `destructive` → `danger`
   *   `outline`     → outline rendering
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
    // legacy aliases — do not remove without migrating call-sites first.
    | "default"
    | "secondary"
    | "destructive";

  type Size = "sm" | "md" | "lg";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    variant?: Variant;
    size?: Size;
    icon?: Snippet;
  }

  // Shared inset rim for gradient variants — white highlight at top.
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
    "gradient-success": `bg-gradient-to-b from-emerald-50 to-emerald-100 text-success-a11y dark:from-emerald-900/40 dark:to-emerald-950/60 ${gradientInset}`,
    "gradient-warning": `bg-gradient-to-b from-amber-50 to-amber-100 text-warning-a11y dark:from-amber-900/40 dark:to-amber-950/60 ${gradientInset}`,
    "gradient-destructive": `bg-gradient-to-b from-red-50 to-red-100 text-danger-a11y dark:from-red-900/40 dark:to-red-950/60 ${gradientInset}`,
    default: "bg-primary/10 text-primary dark:bg-primary/20",
    secondary: "bg-muted text-muted-foreground",
    destructive:
      "bg-destructive/10 text-danger-a11y dark:bg-destructive/20",
  };

  const sizeClasses: Record<Size, string> = {
    sm: "text-[10px] px-2 py-0.5 gap-1",
    md: "text-xs px-2.5 py-0.5 gap-1.5",
    lg: "text-sm px-3 py-1 gap-1.5",
  };

  let {
    class: className = "",
    variant = "neutral",
    size = "sm",
    icon,
    children,
    ...restProps
  }: Props = $props();
</script>

<div
  class={cn(
    "inline-flex items-center rounded-full border border-transparent font-medium transition-colors",
    sizeClasses[size],
    variantClasses[variant],
    className,
  )}
  {...restProps}
>
  {#if icon}
    <span class="inline-flex shrink-0 items-center" aria-hidden="true">
      {@render icon()}
    </span>
  {/if}
  {@render children?.()}
</div>
