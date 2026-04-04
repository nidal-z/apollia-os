<script lang="ts">
  import { sessionBudget } from "$lib/stores/sse";

  /**
   * Formats a USD cost for display.
   * - Zero: "$0.000"
   * - Very small: 4 decimal places
   * - Normal: 3 decimal places
   */
  function formatCost(usd: number): string {
    if (usd === 0) return "$0.000";
    if (usd < 0.001) return `$${usd.toFixed(5)}`;
    return `$${usd.toFixed(3)}`;
  }

  const budget = $derived($sessionBudget);

  /**
   * 0–79% of threshold: neutral
   * 80–99% of threshold: warning (orange)
   * ≥ threshold: exceeded (red)
   */
  const colorClass = $derived(() => {
    if (!budget) return "text-muted-foreground";
    if (budget.threshold_exceeded) return "text-destructive font-semibold";
    if (
      budget.threshold_usd < Number.MAX_VALUE / 2 &&
      budget.session_cost_usd >= budget.threshold_usd * 0.8
    ) {
      return "text-warning font-medium";
    }
    return "text-muted-foreground";
  });
</script>

{#if budget}
  <div
    class="flex items-center gap-1 rounded-full glass-surface glass-border-subtle px-2.5 py-1 text-xs tabular-nums"
    title={`Session cost: ${formatCost(budget.session_cost_usd)} / Threshold: ${budget.threshold_usd < Number.MAX_VALUE / 2 ? formatCost(budget.threshold_usd) : '—'}`}
    data-testid="token-budget-widget"
  >
    <span class={colorClass()}>
      {formatCost(budget.session_cost_usd)}
    </span>
    {#if budget.threshold_exceeded}
      <span
        class="inline-flex h-4 w-4 items-center justify-center rounded-full bg-destructive text-destructive-foreground text-[9px] font-bold leading-none"
        aria-label="Cost threshold exceeded"
      >!</span>
    {/if}
  </div>
{/if}
