<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { LlmCostStatsResponse, LlmCostStatsRow } from "$lib/types";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";

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

<Card>
  <CardHeader>
    <CardTitle class="text-base font-semibold">Session Statistics (7 days)</CardTitle>
  </CardHeader>

  <CardContent>
    {#if loading}
      <p class="text-sm text-muted-foreground">Loading statistics...</p>
    {:else if error}
      <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
    {:else if rows.length === 0}
      <p class="text-sm text-muted-foreground">No LLM calls recorded in the last 7 days.</p>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b text-left text-xs text-muted-foreground">
              <th class="pb-2 pr-4 font-medium">Backend</th>
              <th class="pb-2 pr-4 font-medium">Model</th>
              <th class="pb-2 pr-4 text-right font-medium">Calls</th>
              <th class="pb-2 pr-4 text-right font-medium">Tokens</th>
              <th class="pb-2 text-right font-medium">Cost USD</th>
            </tr>
          </thead>
          <tbody>
            {#each rows as row (row.backend + row.model)}
              <tr class="border-b border-dashed">
                <td class="py-2 pr-4">{row.backend}</td>
                <td class="py-2 pr-4 text-muted-foreground">{row.model}</td>
                <td class="py-2 pr-4 text-right">{row.call_count}</td>
                <td class="py-2 pr-4 text-right">{formatTokens(row.total_tokens)}</td>
                <td class="py-2 text-right">{formatCost(row.total_cost_usd)}</td>
              </tr>
            {/each}
            <tr class="font-bold">
              <td class="pt-2" colspan="2">Total</td>
              <td class="pt-2 text-right">{totalCalls}</td>
              <td class="pt-2 text-right">{formatTokens(totalTokens)}</td>
              <td class="pt-2 text-right">{formatCost(totalCost)}</td>
            </tr>
          </tbody>
        </table>
      </div>
    {/if}
  </CardContent>
</Card>
