<script lang="ts">
  import { cn } from "$lib/utils";
  import { AlertCircle, CheckCircle, Info, AlertTriangle, WifiOff } from "lucide-svelte";
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";

  type Variant = "info" | "success" | "warning" | "destructive" | "neutral";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    /**
     * Persistent banner shown at the top of a region (chat, panel, app).
     * Distinct from `Toast` which auto-dismisses — Banner stays visible
     * while the underlying condition is true (e.g. runtime disconnected,
     * platform sandbox limitation, agent unavailable).
     */
    variant?: Variant;
    /** Optional override of the default lucide icon for the chosen variant. */
    icon?: typeof AlertCircle;
    /** Trailing slot for action buttons / retry affordances. */
    trailing?: Snippet;
    /**
     * Surface treatment:
     * - `"edge"` (default) — `border-b`, full-width, used at the top of a
     *   region (chat banner, panel header). Single-line content, truncated
     *   with ellipsis, items-center, dense padding.
     * - `"card"` — rounded border on every side, used as a standalone
     *   notice card (e.g. MacOS sandbox warning). Multi-line content allowed,
     *   icon top-aligned, comfortable padding.
     */
    surface?: "edge" | "card";
    class?: string;
  }

  let {
    variant = "info",
    icon,
    trailing,
    surface = "edge",
    class: className = "",
    children,
    ...restProps
  }: Props = $props();

  const defaultIcon: Record<Variant, typeof AlertCircle> = {
    info: Info,
    success: CheckCircle,
    warning: AlertTriangle,
    destructive: WifiOff,
    neutral: Info,
  };

  const variantClasses: Record<Variant, string> = {
    info: "border-info/30 bg-info/10 text-info",
    success: "border-success/30 bg-success/10 text-success-a11y",
    warning: "border-warning/30 bg-warning/10 text-warning-a11y",
    destructive: "border-destructive/30 bg-destructive/10 text-destructive",
    neutral: "border-border bg-muted/40 text-muted-foreground",
  };

  const ariaRole = $derived(
    variant === "destructive" || variant === "warning" ? "alert" : "status",
  );
  const ariaLive = $derived(
    variant === "destructive" ? "assertive" : "polite",
  );

  const IconComponent = $derived(icon ?? defaultIcon[variant]);
  const isCard = $derived(surface === "card");
</script>

<div
  class={cn(
    "flex justify-between gap-3 px-4",
    isCard ? "items-start py-3 text-sm rounded-lg border" : "items-center py-2 text-[12px] border-b",
    variantClasses[variant],
    className,
  )}
  role={ariaRole}
  aria-live={ariaLive}
  {...restProps}
>
  <div class={cn("flex gap-2 min-w-0 flex-1", isCard ? "items-start" : "items-center")}>
    <IconComponent
      size={isCard ? 16 : 14}
      aria-hidden="true"
      class={cn("shrink-0", isCard && "mt-0.5")}
    />
    {#if isCard}
      <div class="flex-1">
        {@render children?.()}
      </div>
    {:else}
      <span class="truncate">
        {@render children?.()}
      </span>
    {/if}
  </div>
  {#if trailing}
    <div class={cn("shrink-0", isCard && "mt-0.5")}>
      {@render trailing()}
    </div>
  {/if}
</div>
