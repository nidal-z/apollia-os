<script lang="ts" module>
  export interface FilterChip {
    key: string;
    label: string;
    count?: number | string;
    /** Optional StatusDot color (CSS string). */
    color?: string;
    /** Glow the dot when this chip is active. */
    glowWhenActive?: boolean;
  }
</script>

<script lang="ts">
  /**
   * FilterChipBar - canonical status/filter chip row.
   *
   * Replaces the near-identical hand-rolled chip rows in Tasks, Inbox and
   * Automations. `size="compact"` matches the dense variant used inside split
   * sidebars. The active styling is the canonical Tasks treatment, so adopting
   * this intentionally unifies the (slightly divergent) prior looks.
   */
  import { cn } from "$lib/utils";
  import StatusDot from "./StatusDot.svelte";
  import type { Snippet } from "svelte";

  interface Props {
    chips: FilterChip[];
    activeKey: string;
    onchange: (key: string) => void;
    /** default = page-level bar (px-8 pt-5 pb-4), compact = split-sidebar density. */
    size?: "default" | "compact";
    /** Trailing controls aligned to the right (e.g. an agent <Select>). */
    rightSlot?: Snippet;
    /** When set, each chip gets `data-testid="{prefix}-{chip.key}"` (preserves e2e selectors). */
    testidPrefix?: string;
    class?: string;
    "aria-label"?: string;
    "data-testid"?: string;
  }

  let {
    chips,
    activeKey,
    onchange,
    size = "default",
    rightSlot,
    testidPrefix,
    class: className = "",
    "aria-label": ariaLabel = "Filtres",
    "data-testid": testid,
  }: Props = $props();

  const compact = $derived(size === "compact");
</script>

<div
  class={cn(
    "flex flex-wrap items-center gap-1.5",
    compact ? "" : "justify-between px-8 pt-5 pb-4",
    className,
  )}
  data-testid={testid}
>
  <div class="flex flex-wrap items-center gap-1.5" role="tablist" aria-label={ariaLabel}>
    {#each chips as f (f.key)}
      {@const isActive = activeKey === f.key}
      <button
        type="button"
        role="tab"
        aria-selected={isActive}
        onclick={(e) => { onchange(f.key); (e.currentTarget as HTMLButtonElement).blur(); }}
        class={cn(
          "inline-flex items-center rounded-full border font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/60",
          compact ? "gap-1 px-2 py-0.5 text-[10.5px]" : "gap-1.5 px-2.5 py-1 text-[11.5px]",
          isActive
            ? "border-primary/40 bg-primary/10 text-primary"
            : "border-border bg-transparent text-muted-foreground hover:text-foreground hover:border-border/80",
        )}
        data-active={isActive}
        data-testid={testidPrefix ? `${testidPrefix}-${f.key}` : undefined}
      >
        {#if f.color}
          <StatusDot color={f.color} glow={isActive && Boolean(f.glowWhenActive)} />
        {/if}
        {f.label}
        {#if f.count !== undefined}
          <span class={cn("tabular-nums", isActive ? "text-primary/80" : "text-muted-foreground/60")}>
            {f.count}
          </span>
        {/if}
      </button>
    {/each}
  </div>
  {#if rightSlot}
    {@render rightSlot()}
  {/if}
</div>
