<script lang="ts">
  /**
   * LlmCostThreshold - puts the configured cost ceiling next to the spend it
   * judges, and makes a crossing visible.
   *
   * Reading of the comparison unit. The runtime enforces
   * `[llm] cost_alert_threshold_usd` on the cumulative cost of a session
   * (`SessionBudgetTracker`), while this surface only knows calendar
   * aggregates: `get_llm_cost_stats` returns a per backend/model total over a
   * trailing window, `get_llm_daily_costs` returns the same spend split by day.
   * Comparing the whole window total against a per-session ceiling would flag
   * every installation after a few days of normal use, so the busiest single
   * day of the window is used instead: a day total is an upper bound on any
   * session that ran inside that day, so a day below the ceiling cannot hide a
   * session above it. The caption names the day explicitly, so the operator is
   * never left guessing which spend is being judged.
   */
  import { onMount, onDestroy } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { formatCost } from "$lib/format";
  import { AlertTriangle, Gauge } from "lucide-svelte";
  import { getCostAlertThreshold } from "$lib/ipc/llm";
  import { getLlmDailyCosts } from "$lib/ipc/llmCosts";
  import { Badge } from "$lib/components/ui/badge";
  import { ProgressBar } from "$lib/components/ui/progress";

  interface Props {
    /** Trailing window, in days, shared with the stats table above. */
    windowDays: number;
  }

  let { windowDays }: Props = $props();

  const REFRESH_INTERVAL_MS = 30_000;
  /** Share of the ceiling above which the strip turns to the warning tone. */
  const NEAR_RATIO = 0.8;

  let threshold = $state<number | null>(null);
  let peakDate = $state<string | null>(null);
  let peakCost = $state(0);
  let ready = $state(false);
  /**
   * A failed read hides the strip instead of raising its own notice: the stats
   * block that hosts it already reports the cost surface being unreachable.
   */
  let failed = $state(false);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  async function load(): Promise<void> {
    try {
      const [limit, daily] = await Promise.all([
        getCostAlertThreshold(),
        getLlmDailyCosts(windowDays),
      ]);

      const perDay = new Map<string, number>();
      for (const entry of daily.entries) {
        perDay.set(entry.date, (perDay.get(entry.date) ?? 0) + entry.cost_usd);
      }
      let bestDate: string | null = null;
      let bestCost = 0;
      for (const [date, cost] of perDay) {
        if (bestDate === null || cost > bestCost) {
          bestDate = date;
          bestCost = cost;
        }
      }

      threshold = limit;
      peakDate = bestDate;
      peakCost = bestCost;
      failed = false;
    } catch {
      failed = true;
    } finally {
      ready = true;
    }
  }

  function formatDay(dateStr: string): string {
    const day = new Date(`${dateStr}T12:00:00`);
    return day.toLocaleDateString($locale ?? "en", { month: "short", day: "numeric" });
  }

  // A hand-edited `cost_alert_threshold_usd = 0` is a ceiling of zero for the
  // runtime, so any spend crosses it. Kept out of the division to stay finite.
  const ratio = $derived.by(() => {
    if (threshold === null) return 0;
    if (threshold <= 0) return peakCost > 0 ? 1 : 0;
    return peakCost / threshold;
  });
  const exceeded = $derived(threshold !== null && peakCost > threshold);
  const near = $derived(!exceeded && ratio >= NEAR_RATIO);
  const percent = $derived(Math.min(100, ratio * 100));

  type Tone = "success" | "warning" | "destructive";
  const tone = $derived<Tone>(exceeded ? "destructive" : near ? "warning" : "success");
  const valueClass = $derived(
    exceeded ? "text-destructive" : near ? "text-warning-a11y" : "text-foreground",
  );

  const caption = $derived.by(() => {
    if (peakDate === null || peakCost === 0) {
      return $t("llm.cost_threshold.no_spend", { values: { days: windowDays } });
    }
    if (exceeded) {
      return $t("llm.cost_threshold.exceeded_note", {
        values: {
          spend: formatCost(peakCost),
          date: formatDay(peakDate),
          threshold: formatCost(threshold ?? 0),
        },
      });
    }
    return $t("llm.cost_threshold.basis", {
      values: { days: windowDays, date: formatDay(peakDate) },
    });
  });

  onMount(() => {
    void load();
    refreshTimer = setInterval(() => {
      void load();
    }, REFRESH_INTERVAL_MS);
  });

  onDestroy(() => {
    if (refreshTimer !== null) {
      clearInterval(refreshTimer);
      refreshTimer = null;
    }
  });
</script>

{#if ready && !failed}
  <div
    class="mb-3 rounded-xl glass-card glass-border px-4 py-3"
    role="status"
    aria-live="polite"
    data-testid="llm-cost-threshold"
  >
    {#if threshold === null}
      <p
        class="flex items-start gap-2 text-body-xs text-muted-foreground"
        data-testid="llm-cost-threshold-unset"
      >
        <Gauge size={14} class="mt-px shrink-0" aria-hidden="true" />
        <span>{$t("llm.cost_threshold.unset")}</span>
      </p>
    {:else}
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <span class="text-sm font-medium">{$t("llm.cost_threshold.title")}</span>
          {#if exceeded || near}
            <Badge
              variant={exceeded ? "danger" : "warning"}
              size="sm"
              data-testid="llm-cost-threshold-badge"
            >
              {exceeded
                ? $t("llm.cost_threshold.exceeded_badge")
                : $t("llm.cost_threshold.near_badge")}
            </Badge>
          {/if}
        </div>
        <span
          class={`flex items-center gap-1.5 text-sm font-medium tabular-nums ${valueClass}`}
          data-testid="llm-cost-threshold-value"
        >
          {#if exceeded}
            <AlertTriangle size={14} aria-hidden="true" />
          {/if}
          {$t("llm.cost_threshold.value", {
            values: { spend: formatCost(peakCost), threshold: formatCost(threshold) },
          })}
        </span>
      </div>

      <div class="mt-2" data-testid="llm-cost-threshold-meter">
        <ProgressBar
          value={percent}
          variant={tone}
          size="sm"
          label={$t("llm.cost_threshold.meter_label")}
        />
      </div>

      <p class={`mt-2 text-body-xs ${exceeded ? "text-destructive" : "text-muted-foreground"}`}>
        {caption}
      </p>
    {/if}
  </div>
{/if}
