<script lang="ts">
  import { Gauge } from "lucide-svelte";
  import { sessionMetricsStore } from "$lib/stores/chatMetrics";

  interface Props {
    /** Current chat session id — required to read live metrics. */
    sessionId: string;
    /** Variant: `pill` (used in Metrics tab) or `footer` (mini under input, B.13). */
    variant?: "pill" | "footer";
  }

  let { sessionId, variant = "footer" }: Props = $props();

  const metrics = $derived(sessionMetricsStore(sessionId));

  const contextPct = $derived(
    $metrics.context_window_size > 0
      ? Math.min(
          100,
          ($metrics.messages_in_history / $metrics.context_window_size) * 100,
        )
      : 0,
  );
  const budgetPct = $derived(
    $metrics.budget_max_steps > 0
      ? Math.min(100, ($metrics.steps_used / $metrics.budget_max_steps) * 100)
      : 0,
  );

  const hasActivity = $derived($metrics.exchanges_count > 0);
  const toneClass = $derived(
    contextPct >= 90 ? "text-warning" : "text-muted-foreground/60",
  );
</script>

{#if hasActivity}
  {#if variant === "footer"}
    <div
      class="flex items-center justify-center gap-1.5 px-3 py-1 {toneClass} text-[10px]"
      data-testid="context-indicator-footer"
      aria-label="Contexte et budget de la session"
    >
      <Gauge size={10} />
      <span class="tabular-nums">
        Ctx {Math.round(contextPct)}% · Budget {Math.round(budgetPct)}%
      </span>
    </div>
  {:else}
    <span
      class="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] text-primary"
      data-testid="context-indicator-pill"
    >
      <Gauge size={10} />
      {Math.round(contextPct)}%
    </span>
  {/if}
{/if}
