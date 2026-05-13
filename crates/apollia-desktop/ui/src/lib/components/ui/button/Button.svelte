<script lang="ts">
  import { cn } from "$lib/utils";
  import { Loader2 } from "lucide-svelte";
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  interface Props extends HTMLButtonAttributes {
    variant?:
      | "default"
      | "primary-solid"
      | "primary-gradient"
      | "destructive"
      | "success"
      | "outline"
      | "secondary"
      | "ghost"
      | "link"
      | "elevated"
      | "soft";
    /**
     * Height/padding profile:
     * - `default` = `h-10 px-4 py-2` (40px, form buttons / CTAs).
     * - `sm`      = `h-9 px-3` (36px, dense rows, banner trailing actions).
     * - `lg`      = `h-11 px-8` (44px, hero CTAs).
     * - `icon`    = `h-10 w-10` (square icon button).
     * - `icon-sm` = `h-7 w-7` (square icon button, compact rows / tables).
     * - `auto`    = no fixed height (multi-line content cards / list rows).
     */
    size?: "default" | "sm" | "lg" | "icon" | "icon-sm" | "auto";
    loading?: boolean;
    class?: string;
    /** Optional leading icon snippet rendered before the children. */
    icon?: Snippet;
    /** Optional trailing keyboard-shortcut hint (e.g. ⌘+S). */
    kbd?: Snippet;
  }

  let {
    variant = "default",
    size = "default",
    loading = false,
    class: className = "",
    disabled,
    icon,
    kbd,
    children,
    ...restProps
  }: Props = $props();

  const variantClasses: Record<string, string> = {
    default: "bg-primary text-primary-foreground shadow-sm hover:bg-primary/90",
    "primary-solid": "bg-primary-solid",
    "primary-gradient": "bg-primary-gradient",
    destructive: "bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90",
    success: "bg-emerald-600 text-white shadow-sm hover:bg-emerald-600/90 dark:bg-emerald-700 dark:hover:bg-emerald-700/90",
    outline: "border border-border bg-transparent text-foreground hover:bg-muted",
    secondary: "bg-muted text-foreground hover:bg-muted/80",
    ghost: "text-foreground hover:bg-muted",
    link: "text-primary underline-offset-4 hover:underline",
    elevated:
      "bg-primary text-primary-foreground shadow-md hover:shadow-lg hover:bg-primary/90 dark:shadow-primary/30",
    soft: "bg-primary/10 text-primary hover:bg-primary/15",
  };

  const sizeClasses: Record<string, string> = {
    default: "h-10 px-4 py-2",
    sm: "h-9 rounded-md px-3",
    lg: "h-11 rounded-md px-8",
    icon: "h-10 w-10",
    "icon-sm": "h-7 w-7 rounded-md",
    auto: "px-3 py-2",
  };
</script>

<button
  class={cn(
    "inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium ring-offset-background transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 active:scale-[0.98]",
    variantClasses[variant],
    sizeClasses[size],
    className
  )}
  disabled={disabled || loading}
  aria-busy={loading || undefined}
  {...restProps}
>
  {#if loading}
    <Loader2 class="animate-spin mr-2 h-4 w-4" aria-hidden="true" />
  {:else if icon}
    <span class="inline-flex shrink-0 items-center mr-1.5" aria-hidden="true">
      {@render icon()}
    </span>
  {/if}
  {@render children?.()}
  {#if kbd}
    <span class="inline-flex shrink-0 items-center ml-2 opacity-80">
      {@render kbd()}
    </span>
  {/if}
</button>
