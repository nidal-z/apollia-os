<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { LlmDailyCostsResponse, LlmDailyCostEntry } from "$lib/types";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
  import { Skeleton } from "$lib/components/ui/skeleton";

  const REFRESH_INTERVAL_MS = 60_000;
  const CHART_WIDTH = 600;
  const CHART_HEIGHT = 250;
  const MARGIN = { top: 20, right: 20, bottom: 40, left: 60 };
  const INNER_WIDTH = CHART_WIDTH - MARGIN.left - MARGIN.right;
  const INNER_HEIGHT = CHART_HEIGHT - MARGIN.top - MARGIN.bottom;

  const BACKEND_COLORS: string[] = [
    "#8b5cf6",
    "#3b82f6",
    "#10b981",
    "#f59e0b",
    "#ef4444",
    "#ec4899",
    "#06b6d4",
    "#84cc16",
  ];

  let entries = $state<LlmDailyCostEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  let backends = $derived(
    [...new Set(entries.map((e) => e.backend))].sort(),
  );

  let backendColorMap = $derived(
    Object.fromEntries(backends.map((b, i) => [b, BACKEND_COLORS[i % BACKEND_COLORS.length]])),
  );

  let dateLabels = $derived((() => {
    const dates: string[] = [];
    const now = new Date();
    for (let i = 6; i >= 0; i--) {
      const d = new Date(now);
      d.setDate(d.getDate() - i);
      dates.push(d.toISOString().slice(0, 10));
    }
    return dates;
  })());

  function shortDayLabel(dateStr: string): string {
    const d = new Date(dateStr + "T12:00:00");
    return d.toLocaleDateString(undefined, { weekday: "short" });
  }

  let barData = $derived(
    dateLabels.map((date) => {
      const dayEntries = entries.filter((e) => e.date === date);
      const segments: { backend: string; cost: number; color: string }[] = [];
      for (const b of backends) {
        const entry = dayEntries.find((e) => e.backend === b);
        segments.push({
          backend: b,
          cost: entry ? entry.cost_usd : 0,
          color: backendColorMap[b] ?? BACKEND_COLORS[0],
        });
      }
      const total = segments.reduce((sum, s) => sum + s.cost, 0);
      return { date, segments, total };
    }),
  );

  let maxCost = $derived(
    Math.max(0.01, ...barData.map((d) => d.total)),
  );

  let totalCost = $derived(
    entries.reduce((sum, e) => sum + e.cost_usd, 0),
  );

  function formatCost(value: number): string {
    if (value === 0) return "$0.00";
    if (value < 0.01) return `$${value.toFixed(4)}`;
    return `$${value.toFixed(2)}`;
  }

  let yTicks = $derived((() => {
    const tickCount = 4;
    const step = maxCost / tickCount;
    return Array.from({ length: tickCount + 1 }, (_, i) => i * step);
  })());

  async function loadCosts(): Promise<void> {
    try {
      const result: LlmDailyCostsResponse = await invoke("get_llm_daily_costs", { days: 7 });
      entries = result.entries;
      error = null;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadCosts();
    refreshTimer = setInterval(() => {
      void loadCosts();
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
    <CardTitle class="text-base font-semibold">{$t('observability.llm_costs_title')}</CardTitle>
  </CardHeader>

  <CardContent>
    {#if loading}
      <Skeleton width="100%" height="250px" />
    {:else if error}
      <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
    {:else if entries.length === 0}
      <div class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12">
        <p class="text-muted-foreground">{$t('observability.no_llm_calls')}</p>
      </div>
    {:else}
      <div class="overflow-x-auto">
        <svg
          viewBox="0 0 {CHART_WIDTH} {CHART_HEIGHT}"
          class="w-full max-w-[600px]"
          role="img"
          aria-label="LLM daily cost bar chart"
        >
          {#each yTicks as tick (tick)}
            {@const y = MARGIN.top + INNER_HEIGHT - (tick / maxCost) * INNER_HEIGHT}
            <line
              x1={MARGIN.left}
              y1={y}
              x2={MARGIN.left + INNER_WIDTH}
              y2={y}
              stroke="currentColor"
              stroke-opacity="0.1"
            />
            <text
              x={MARGIN.left - 8}
              y={y + 4}
              text-anchor="end"
              class="fill-muted-foreground"
              font-size="10"
            >
              {formatCost(tick)}
            </text>
          {/each}

          {#each barData as day, dayIndex (day.date)}
            {@const barWidth = (INNER_WIDTH / dateLabels.length) * 0.6}
            {@const gap = (INNER_WIDTH / dateLabels.length) * 0.4}
            {@const barX = MARGIN.left + dayIndex * (barWidth + gap) + gap / 2}

            {#each day.segments as segment, segIdx (segment.backend)}
              {@const segmentsBefore = day.segments.slice(0, segIdx)}
              {@const yOffset = segmentsBefore.reduce((s, seg) => s + seg.cost, 0)}
              {@const barHeight = (segment.cost / maxCost) * INNER_HEIGHT}
              {@const barY = MARGIN.top + INNER_HEIGHT - (yOffset / maxCost) * INNER_HEIGHT - barHeight}

              {#if segment.cost > 0}
                <rect
                  x={barX}
                  y={barY}
                  width={barWidth}
                  height={Math.max(barHeight, 1)}
                  fill={segment.color}
                  rx="2"
                >
                  <title>{segment.backend}: {formatCost(segment.cost)}</title>
                </rect>
              {/if}
            {/each}

            <text
              x={barX + barWidth / 2}
              y={CHART_HEIGHT - 8}
              text-anchor="middle"
              class="fill-muted-foreground"
              font-size="11"
            >
              {shortDayLabel(day.date)}
            </text>
          {/each}

          <line
            x1={MARGIN.left}
            y1={MARGIN.top}
            x2={MARGIN.left}
            y2={MARGIN.top + INNER_HEIGHT}
            stroke="currentColor"
            stroke-opacity="0.2"
          />
          <line
            x1={MARGIN.left}
            y1={MARGIN.top + INNER_HEIGHT}
            x2={MARGIN.left + INNER_WIDTH}
            y2={MARGIN.top + INNER_HEIGHT}
            stroke="currentColor"
            stroke-opacity="0.2"
          />
        </svg>
      </div>

      <div class="mt-4 flex flex-wrap gap-3">
        {#each backends as backend (backend)}
          <div class="flex items-center gap-1.5">
            <span
              class="inline-block h-3 w-3 rounded-sm"
              style="background-color: {backendColorMap[backend]}"
            ></span>
            <span class="text-xs text-muted-foreground">{backend}</span>
          </div>
        {/each}
      </div>

      <div class="mt-3 text-center">
        <span class="text-lg font-bold">{$t('observability.total_7d', { values: { cost: formatCost(totalCost) } })}</span>
      </div>
    {/if}
  </CardContent>
</Card>
