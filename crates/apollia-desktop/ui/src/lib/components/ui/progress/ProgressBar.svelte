<script lang="ts">
  import { cn } from "$lib/utils";

  type Size = "sm" | "md";
  type Variant = "primary" | "success" | "warning" | "destructive" | "info";

  interface Props {
    /** 0–100. Leave undefined for indeterminate. */
    value?: number;
    label?: string;
    size?: Size;
    variant?: Variant;
    class?: string;
  }

  let {
    value,
    label,
    size = "md",
    variant = "primary",
    class: className = "",
  }: Props = $props();

  const clamped = $derived(
    value === undefined ? undefined : Math.max(0, Math.min(100, value)),
  );
  const indeterminate = $derived(clamped === undefined);

  const heightClass = $derived(size === "sm" ? "h-1" : "h-2");
  const fillClass = $derived(
    ({
      primary: "from-primary to-primary/80",
      success: "from-success to-success/80",
      warning: "from-warning to-warning/80",
      destructive: "from-destructive to-destructive/80",
      info: "from-info to-info/80",
    } as const)[variant],
  );
</script>

<div class={cn("flex w-full flex-col gap-1.5", className)}>
  {#if label}
    <div class="flex items-center justify-between text-xs text-muted-foreground">
      <span>{label}</span>
      {#if !indeterminate}
        <span class="tabular-nums">{Math.round(clamped ?? 0)}%</span>
      {/if}
    </div>
  {/if}
  <div
    class={cn("relative w-full overflow-hidden rounded-full bg-muted", heightClass)}
    role="progressbar"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={indeterminate ? undefined : clamped}
    aria-label={label ?? (indeterminate ? "Loading" : undefined)}
    aria-busy={indeterminate || undefined}
  >
    {#if indeterminate}
      <div
        class={cn(
          "progress-indeterminate absolute inset-y-0 w-[30%] rounded-full bg-gradient-to-r motion-reduce:progress-reduced",
          fillClass,
        )}
      ></div>
    {:else}
      <div
        class={cn("h-full rounded-full bg-gradient-to-r transition-[width] duration-300", fillClass)}
        style:width="{clamped}%"
      ></div>
    {/if}
  </div>
</div>

<style>
  .progress-indeterminate {
    animation: progress-indeterminate 1.5s ease-in-out infinite;
  }
  @media (prefers-reduced-motion: reduce) {
    .progress-indeterminate {
      animation: none !important;
      left: 0 !important;
      width: 50% !important;
    }
  }
</style>
