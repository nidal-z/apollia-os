<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { LlmCostStatsResponse, LlmCostStatsRow } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";

  const REFRESH_INTERVAL_MS = 30_000;

  let rows = $state<LlmCostStatsRow[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  async function loadStats(): Promise<void> {
    try {
      const result: LlmCostStatsResponse = await invoke("get_llm_cost_stats", { days: 7 });
      rows = result.rows;
      error = null;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function formatCost(value: number): string {
    if (value === 0) return "$0.00";
    if (value < 0.01) return `$${value.toFixed(4)}`;
    return `$${value.toFixed(2)}`;
  }

  function formatTokens(value: number): string {
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    return value.toString();
  }

  const isOperator = $derived($uiMode === "operator");
  const totalCalls = $derived(rows.reduce((sum, r) => sum + r.call_count, 0));
  const totalTokens = $derived(rows.reduce((sum, r) => sum + r.total_tokens, 0));
  const totalCost = $derived(rows.reduce((sum, r) => sum + r.total_cost_usd, 0));

  onMount(() => {
    void loadStats();
    refreshTimer = setInterval(() => {
      void loadStats();
    }, REFRESH_INTERVAL_MS);
  });

  onDestroy(() => {
    if (refreshTimer !== null) {
      clearInterval(refreshTimer);
      refreshTimer = null;
    }
  });
</script>

<div>
  {#if loading}
    <p class="text-sm text-muted-foreground">{$t('llm.loading_stats')}</p>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else if rows.length === 0}
    <p class="text-sm text-muted-foreground">
      {isOperator ? $t('llm.no_calls_operator') : $t('llm.no_calls')}
    </p>
  {:else if isOperator}
    <!-- Operator mode: single summary line -->
    <p class="text-sm text-muted-foreground">
      {$t('llm.used_today', { values: { count: totalCalls, cost: formatCost(totalCost) } })}
    </p>
  {:else}
    <!-- Builder mode: full table with standard glass-card pattern -->
    <div class="glass-card glass-border overflow-x-auto rounded-lg" data-testid="llm-stats-table">
      <table class="w-full min-w-[520px] text-[13px]">
        <thead class="border-b border-border bg-muted/50">
          <tr>
            <th class="px-3 py-2 text-left text-[11px] font-medium text-muted-foreground">{$t('llm.table.backend')}</th>
            <th class="px-3 py-2 text-left text-[11px] font-medium text-muted-foreground">{$t('llm.table.model')}</th>
            <th class="px-3 py-2 text-right text-[11px] font-medium text-muted-foreground">{$t('llm.table.calls')}</th>
            <th class="px-3 py-2 text-right text-[11px] font-medium text-muted-foreground">{$t('llm.table.tokens')}</th>
            <th class="px-3 py-2 text-right text-[11px] font-medium text-muted-foreground">{$t('llm.table.cost_usd')}</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.backend + row.model)}
            <tr class="border-b border-border last:border-0 hover:bg-muted">
              <td class="px-3 py-2">{row.backend}</td>
              <td class="px-3 py-2 text-muted-foreground">{row.model}</td>
              <td class="px-3 py-2 text-right tabular-nums">{row.call_count}</td>
              <td class="px-3 py-2 text-right tabular-nums">{formatTokens(row.total_tokens)}</td>
              <td class="px-3 py-2 text-right tabular-nums">{formatCost(row.total_cost_usd)}</td>
            </tr>
          {/each}
          <!-- Total row -->
          <tr class="border-t border-border" data-testid="llm-stats-total-row">
            <td class="px-3 py-2 font-medium" colspan="2">{$t('llm.table.total')}</td>
            <td class="px-3 py-2 text-right font-medium tabular-nums">{totalCalls}</td>
            <td class="px-3 py-2 text-right font-medium tabular-nums">{formatTokens(totalTokens)}</td>
            <td class="px-3 py-2 text-right font-medium tabular-nums">{formatCost(totalCost)}</td>
          </tr>
        </tbody>
      </table>
    </div>
  {/if}
</div>
