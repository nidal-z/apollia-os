<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { LlmDailyCostsResponse, LlmDailyCostEntry } from "$lib/types";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { BarChart3, TrendingUp } from "lucide-svelte";

  const REFRESH_INTERVAL_MS = 60_000;

  /** SVG viewBox dimensions - the chart rescales to fit its container. */
  const CHART_WIDTH = 920;
  const CHART_HEIGHT = 320;
  const MARGIN = { top: 24, right: 24, bottom: 48, left: 64 };
  const INNER_WIDTH = CHART_WIDTH - MARGIN.left - MARGIN.right;
  const INNER_HEIGHT = CHART_HEIGHT - MARGIN.top - MARGIN.bottom;

  /** Chart palette - first five entries map to --chart-1..5 design tokens. */
  const CHART_CSS_VARS: string[] = [
    "var(--chart-1)",
    "var(--chart-2)",
    "var(--chart-3)",
    "var(--chart-4)",
    "var(--chart-5)",
  ];

  type PeriodKey = "7d" | "14d" | "30d" | "90d" | "365d";
  const PERIODS: { key: PeriodKey; days: number; labelKey: string }[] = [
    { key: "7d", days: 7, labelKey: "observability.period_7d" },
    { key: "14d", days: 14, labelKey: "observability.period_14d" },
    { key: "30d", days: 30, labelKey: "observability.period_30d" },
    { key: "90d", days: 90, labelKey: "observability.period_90d" },
    { key: "365d", days: 365, labelKey: "observability.period_365d" },
  ];

  let activePeriod = $state<PeriodKey>("7d");
  let windowDays = $derived(PERIODS.find((p) => p.key === activePeriod)?.days ?? 7);

  let entries = $state<LlmDailyCostEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;
  let hoverIndex = $state<number | null>(null);

  let backends = $derived([...new Set(entries.map((e) => e.backend))].sort());

  let backendColorMap = $derived(
    Object.fromEntries(backends.map((b, i) => [b, CHART_CSS_VARS[i % CHART_CSS_VARS.length]])),
  );

  /** Build the YYYY-MM-DD axis covering the active window, inclusive of today. */
  let dateLabels = $derived.by(() => {
    const dates: string[] = [];
    const now = new Date();
    for (let i = windowDays - 1; i >= 0; i--) {
      const d = new Date(now);
      d.setDate(d.getDate() - i);
      dates.push(d.toISOString().slice(0, 10));
    }
    return dates;
  });

  /** When the window grows, individual day labels overlap. We thin them: keep
   *  one tick every Nth day, always keep the first and last. */
  let xLabelStride = $derived(
    windowDays <= 14 ? 1 : windowDays <= 30 ? 3 : windowDays <= 90 ? 7 : 30,
  );

  function shortDayLabel(dateStr: string): string {
    const d = new Date(dateStr + "T12:00:00");
    return d.toLocaleDateString(undefined, { weekday: "short" });
  }

  function shortDateLabel(dateStr: string): string {
    const d = new Date(dateStr + "T12:00:00");
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
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
          color: backendColorMap[b] ?? CHART_CSS_VARS[0],
        });
      }
      const total = segments.reduce((sum, s) => sum + s.cost, 0);
      return { date, segments, total };
    }),
  );

  let maxCost = $derived(Math.max(0.01, ...barData.map((d) => d.total)));

  let totalCost = $derived(entries.reduce((sum, e) => sum + e.cost_usd, 0));
  let avgPerDay = $derived(totalCost / Math.max(1, windowDays));

  let maxDay = $derived.by(() => {
    let best: { date: string; total: number } | null = null;
    for (const day of barData) {
      if (best === null || day.total > best.total) {
        best = { date: day.date, total: day.total };
      }
    }
    return best;
  });

  /** Total cost per backend over the window - used in the legend pills. */
  let backendTotals = $derived(
    backends.map((b) => ({
      backend: b,
      color: backendColorMap[b],
      cost: entries.filter((e) => e.backend === b).reduce((s, e) => s + e.cost_usd, 0),
    })),
  );

  let topBackend = $derived.by(() => {
    if (backendTotals.length === 0) return null;
    return [...backendTotals].sort((a, b) => b.cost - a.cost)[0];
  });

  function formatCost(value: number): string {
    if (value === 0) return "$0.00";
    if (value < 0.01) return `$${value.toFixed(4)}`;
    return `$${value.toFixed(2)}`;
  }

  let yScale = $derived.by(() => {
    const niceMax = niceUpperBound(maxCost);
    const tickCount = 4;
    const step = niceMax / tickCount;
    const ticks = Array.from({ length: tickCount + 1 }, (_, i) => i * step);
    return { max: niceMax, ticks };
  });

  function niceUpperBound(value: number): number {
    if (value <= 0) return 0.01;
    const exp = Math.floor(Math.log10(value));
    const base = Math.pow(10, exp);
    const norm = value / base;
    let nice: number;
    if (norm <= 1) nice = 1;
    else if (norm <= 2) nice = 2;
    else if (norm <= 2.5) nice = 2.5;
    else if (norm <= 5) nice = 5;
    else nice = 10;
    return nice * base;
  }

  async function loadCosts(): Promise<void> {
    try {
      const result: LlmDailyCostsResponse = await invoke("get_llm_daily_costs", {
        days: windowDays,
      });
      entries = result.entries;
      error = null;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function setPeriod(key: PeriodKey) {
    if (activePeriod === key) return;
    activePeriod = key;
    loading = true;
    hoverIndex = null;
    void loadCosts();
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

<section class="glass-card glass-border rounded-xl overflow-hidden" data-testid="llm-cost-chart">
  <header
    class="flex flex-wrap items-center justify-between gap-3 px-6 pt-5 pb-4 border-b border-border/40"
  >
    <div class="flex items-baseline gap-3">
      <h3 class="m-0 text-[15px] font-semibold tracking-[-0.2px] text-foreground">
        {$t('observability.llm_costs_title')}
      </h3>
      <span class="font-mono text-[10.5px] tracking-[1.5px] text-muted-foreground/70 uppercase">
        {$t('observability.period_window_days', { values: { days: windowDays } })}
      </span>
    </div>

    <!-- Period selector - segmented control aligned with the design system. -->
    <div
      class="inline-flex items-center gap-0.5 rounded-md glass-border glass-surface p-0.5"
      role="tablist"
      aria-label={$t('observability.period_label')}
      data-testid="period-selector"
    >
      {#each PERIODS as period (period.key)}
        {@const isActive = activePeriod === period.key}
        <button
          type="button"
          role="tab"
          aria-selected={isActive}
          class="px-2.5 py-1 rounded text-[11.5px] font-medium tabular-nums transition-colors {isActive
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => setPeriod(period.key)}
          data-testid="period-{period.key}"
        >
          {$t(period.labelKey)}
        </button>
      {/each}
    </div>
  </header>

  <div class="px-6 py-5">
    {#if loading}
      <Skeleton width="100%" height="320px" />
    {:else if error}
      <p class="text-sm text-destructive">{error}</p>
    {:else if entries.length === 0 || totalCost === 0}
      <div class="flex flex-col items-center justify-center py-16" data-testid="llm-costs-empty">
        <div class="rounded-full glass-inset p-4 mb-4">
          <BarChart3 class="h-8 w-8 text-muted-foreground/60" />
        </div>
        <p class="text-[13px] text-muted-foreground">{$t('observability.no_llm_calls')}</p>
      </div>
    {:else}
      <!-- KPI strip -->
      <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6" data-testid="llm-costs-kpis">
        <article class="glass-inset rounded-lg px-4 py-3">
          <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
            {$t('observability.kpi_total')}
          </div>
          <div class="text-[20px] font-semibold tabular-nums leading-none">{formatCost(totalCost)}</div>
        </article>

        <article class="glass-inset rounded-lg px-4 py-3">
          <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
            {$t('observability.kpi_avg')}
          </div>
          <div class="text-[20px] font-semibold tabular-nums leading-none">{formatCost(avgPerDay)}</div>
        </article>

        <article class="glass-inset rounded-lg px-4 py-3">
          <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
            {$t('observability.kpi_peak')}
          </div>
          <div class="flex items-baseline gap-2">
            <span class="text-[20px] font-semibold tabular-nums leading-none">
              {maxDay ? formatCost(maxDay.total) : "-"}
            </span>
            {#if maxDay}
              <span class="text-[11px] text-muted-foreground tabular-nums">{shortDateLabel(maxDay.date)}</span>
            {/if}
          </div>
        </article>

        <article class="glass-inset rounded-lg px-4 py-3">
          <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
            {$t('observability.kpi_top_backend')}
          </div>
          {#if topBackend}
            <div class="flex items-center gap-2 leading-none">
              <span
                class="inline-block h-2.5 w-2.5 rounded-full flex-shrink-0"
                style="background-color: {topBackend.color}"
                aria-hidden="true"
              ></span>
              <span class="text-[15px] font-semibold truncate" title={topBackend.backend}>
                {topBackend.backend}
              </span>
              <span class="text-[11px] text-muted-foreground tabular-nums ml-auto">
                {formatCost(topBackend.cost)}
              </span>
            </div>
          {:else}
            <div class="text-[15px] font-semibold text-muted-foreground">-</div>
          {/if}
        </article>
      </div>

      <!-- Chart -->
      <div class="overflow-x-auto" data-testid="llm-costs-chart-svg-wrapper">
        <svg
          viewBox="0 0 {CHART_WIDTH} {CHART_HEIGHT}"
          class="block w-full min-w-[560px] mx-auto"
          style="height: auto; max-height: 360px;"
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={$t('observability.llm_costs_title')}
        >
          <!-- Horizontal grid lines + Y-axis ticks -->
          {#each yScale.ticks as tick (tick)}
            {@const y = MARGIN.top + INNER_HEIGHT - (tick / yScale.max) * INNER_HEIGHT}
            <line
              x1={MARGIN.left}
              y1={y}
              x2={MARGIN.left + INNER_WIDTH}
              y2={y}
              stroke="currentColor"
              stroke-opacity="0.08"
              stroke-dasharray={tick === 0 ? "0" : "2 3"}
            />
            <text
              x={MARGIN.left - 10}
              y={y + 3.5}
              text-anchor="end"
              class="fill-muted-foreground"
              font-size="10.5"
              font-family="ui-monospace, monospace"
            >
              {formatCost(tick)}
            </text>
          {/each}

          <!-- Stacked bars -->
          {#each barData as day, dayIndex (day.date)}
            {@const barSlot = INNER_WIDTH / dateLabels.length}
            {@const barWidth = Math.max(2, Math.min(barSlot * 0.62, 48))}
            {@const barX = MARGIN.left + dayIndex * barSlot + (barSlot - barWidth) / 2}
            {@const isHover = hoverIndex === dayIndex}
            {@const visibleSegments = day.segments.filter((s) => s.cost > 0)}
            {@const showLabel = dayIndex === dateLabels.length - 1 || dayIndex === 0 || dayIndex % xLabelStride === 0}

            <!-- Invisible hover hit-area covering the full column -->
            <rect
              x={MARGIN.left + dayIndex * barSlot}
              y={MARGIN.top}
              width={barSlot}
              height={INNER_HEIGHT}
              fill="transparent"
              onmouseenter={() => (hoverIndex = dayIndex)}
              onmouseleave={() => (hoverIndex = null)}
              role="img"
              aria-label={`${shortDateLabel(day.date)}: ${formatCost(day.total)}`}
            />

            {#each visibleSegments as segment, segIdx (segment.backend)}
              {@const segmentsBefore = visibleSegments.slice(0, segIdx)}
              {@const yOffset = segmentsBefore.reduce((s, seg) => s + seg.cost, 0)}
              {@const barHeight = (segment.cost / yScale.max) * INNER_HEIGHT}
              {@const barY = MARGIN.top + INNER_HEIGHT - (yOffset / yScale.max) * INNER_HEIGHT - barHeight}
              {@const isTopSegment = segIdx === visibleSegments.length - 1}

              <rect
                x={barX}
                y={barY}
                width={barWidth}
                height={Math.max(barHeight, 1.5)}
                fill={segment.color}
                opacity={isHover || hoverIndex === null ? 1 : 0.45}
                rx={isTopSegment ? Math.min(4, barWidth / 4) : 0}
                ry={isTopSegment ? Math.min(4, barWidth / 4) : 0}
                style="transition: opacity 140ms ease;"
              >
                <title>{segment.backend}: {formatCost(segment.cost)} - {shortDateLabel(day.date)}</title>
              </rect>
            {/each}

            <!-- Day total above bar on hover -->
            {#if isHover && day.total > 0}
              {@const labelY = MARGIN.top + INNER_HEIGHT - (day.total / yScale.max) * INNER_HEIGHT - 8}
              <text
                x={barX + barWidth / 2}
                y={labelY}
                text-anchor="middle"
                font-size="11"
                font-weight="600"
                class="fill-foreground"
                font-family="ui-monospace, monospace"
              >
                {formatCost(day.total)}
              </text>
            {/if}

            <!-- X-axis labels - thinned when the window is wide. -->
            {#if showLabel}
              {#if windowDays <= 14}
                <text
                  x={barX + barWidth / 2}
                  y={CHART_HEIGHT - 26}
                  text-anchor="middle"
                  class={isHover ? "fill-foreground" : "fill-muted-foreground"}
                  font-size="11"
                  font-weight={isHover ? "600" : "500"}
                >
                  {shortDayLabel(day.date)}
                </text>
                <text
                  x={barX + barWidth / 2}
                  y={CHART_HEIGHT - 12}
                  text-anchor="middle"
                  class="fill-muted-foreground/60"
                  font-size="9.5"
                  font-family="ui-monospace, monospace"
                >
                  {shortDateLabel(day.date)}
                </text>
              {:else}
                <text
                  x={barX + barWidth / 2}
                  y={CHART_HEIGHT - 14}
                  text-anchor="middle"
                  class={isHover ? "fill-foreground" : "fill-muted-foreground/80"}
                  font-size="10"
                  font-family="ui-monospace, monospace"
                  font-weight={isHover ? "600" : "500"}
                >
                  {shortDateLabel(day.date)}
                </text>
              {/if}
            {/if}
          {/each}

          <line
            x1={MARGIN.left}
            y1={MARGIN.top + INNER_HEIGHT}
            x2={MARGIN.left + INNER_WIDTH}
            y2={MARGIN.top + INNER_HEIGHT}
            stroke="currentColor"
            stroke-opacity="0.18"
          />
        </svg>
      </div>

      <!-- Legend pills with per-backend totals -->
      <div
        class="flex flex-wrap items-center justify-center gap-2 mt-5 pt-4 border-t border-border/40"
        data-testid="llm-costs-legend"
      >
        <TrendingUp size={11} class="text-muted-foreground/60 mr-1" aria-hidden="true" />
        {#each backendTotals as entry (entry.backend)}
          <span
            class="inline-flex items-center gap-1.5 rounded-full glass-inset px-2.5 py-1"
            title={entry.backend}
          >
            <span
              class="inline-block h-2 w-2 rounded-full flex-shrink-0"
              style="background-color: {entry.color}"
              aria-hidden="true"
            ></span>
            <span class="text-[11.5px] font-medium text-foreground truncate max-w-[10rem]">{entry.backend}</span>
            <span class="text-[10.5px] text-muted-foreground tabular-nums">{formatCost(entry.cost)}</span>
          </span>
        {/each}
      </div>
    {/if}
  </div>
</section>
