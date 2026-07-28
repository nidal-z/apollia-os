<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import type { LlmCostStatsResponse, LlmCostStatsRow } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { getLlmCostStats } from "$lib/ipc/llm";
  import { DataTable, type Column } from "$lib/components/ui/data-table";

  const REFRESH_INTERVAL_MS = 30_000;

  let rows = $state<LlmCostStatsRow[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  async function loadStats(): Promise<void> {
    try {
      const result: LlmCostStatsResponse = await getLlmCostStats(7);
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

  const columns = $derived<Column<LlmCostStatsRow>[]>([
    { key: "backend", header: $t('llm.table.backend') },
    { key: "model", header: $t('llm.table.model'), cell: modelCell },
    { key: "call_count", header: $t('llm.table.calls'), align: "right", cell: callsCell },
    { key: "total_tokens", header: $t('llm.table.tokens'), align: "right", cell: tokensCell },
    { key: "total_cost_usd", header: $t('llm.table.cost_usd'), align: "right", cell: costCell },
  ]);

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

{#snippet modelCell(row: LlmCostStatsRow)}
  <span class="text-muted-foreground">{row.model}</span>
{/snippet}

{#snippet callsCell(row: LlmCostStatsRow)}
  <span class="tabular-nums">{row.call_count}</span>
{/snippet}

{#snippet tokensCell(row: LlmCostStatsRow)}
  <span class="tabular-nums">{formatTokens(row.total_tokens)}</span>
{/snippet}

{#snippet costCell(row: LlmCostStatsRow)}
  <span class="tabular-nums">{formatCost(row.total_cost_usd)}</span>
{/snippet}

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
    <!-- Builder mode: full table -->
    <div data-testid="llm-stats-table">
      <div class="overflow-x-auto">
        <DataTable
          data={rows}
          {columns}
          rowKey={(r) => r.backend + r.model}
          class="min-w-[520px]"
          emptyLabel={$t('llm.no_calls')}
        />
      </div>
      <div
        class="flex items-center justify-between gap-3 rounded-b-xl glass-card glass-border border-t border-border/40 px-4 py-3 text-sm"
        data-testid="llm-stats-total-row"
      >
        <span class="font-medium">{$t('llm.table.total')}</span>
        <div class="flex items-center gap-6 font-medium tabular-nums">
          <span>{totalCalls}</span>
          <span>{formatTokens(totalTokens)}</span>
          <span>{formatCost(totalCost)}</span>
        </div>
      </div>
    </div>
  {/if}
</div>
