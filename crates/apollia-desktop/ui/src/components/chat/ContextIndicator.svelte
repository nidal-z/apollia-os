<script lang="ts">
  import { t } from "svelte-i18n";
  import { Gauge } from "lucide-svelte";
  import { contextGaugeLabel, contextPct } from "$lib/chat/contextGauge";
  import { sessionMetricsStore } from "$lib/stores/chatMetrics";

  interface Props {
    /** Current chat session id - required to read live metrics. */
    sessionId: string;
    /** Variant: `pill` (used in Metrics tab) or `footer` (mini under input). */
    variant?: "pill" | "footer";
    /**
     * Click handler - when supplied, the indicator becomes a button (P7 -
     * opens the `<InjectedMemorySheet />` sheet).
     */
    onclick?: () => void;
  }

  let { sessionId, variant = "footer", onclick }: Props = $props();

  const metrics = $derived(sessionMetricsStore(sessionId));

  // `--` when the backend reports no context window (Agent mode, or an engine
  // that omits usage): zero is an absent measurement there, not an empty
  // context, and rendering it as a percentage misreads the session state.
  const ctxLabel = $derived(
    contextGaugeLabel($metrics.context_window_tokens, $metrics.context_tokens_used),
  );
  const ctxPct = $derived(
    contextPct($metrics.context_window_tokens, $metrics.context_tokens_used),
  );
  const budgetPct = $derived(
    $metrics.budget_max_steps > 0
      ? Math.min(100, ($metrics.steps_used / $metrics.budget_max_steps) * 100)
      : 0,
  );

  const hasActivity = $derived($metrics.exchanges_count > 0);
  const toneClass = $derived(
    ctxPct >= 90 ? "text-warning" : "text-muted-foreground/60",
  );
</script>

{#if hasActivity}
  {#if variant === "footer"}
    {#if onclick}
      <button
        type="button"
        class="flex items-center justify-center gap-1.5 rounded px-3 py-1 {toneClass} text-micro transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary"
        data-testid="context-indicator-footer"
        aria-label={$t("chat.context_inspect_turn")}
        {onclick}
      >
        <Gauge size={10} />
        <span class="tabular-nums">
          {$t("chat.context_gauge", {
            values: { ctx: ctxLabel, budget: Math.round(budgetPct) },
          })}
        </span>
      </button>
    {:else}
      <div
        class="flex items-center justify-center gap-1.5 px-3 py-1 {toneClass} text-micro"
        data-testid="context-indicator-footer"
        aria-label={$t("chat.context_summary")}
      >
        <Gauge size={10} />
        <span class="tabular-nums">
          {$t("chat.context_gauge", {
            values: { ctx: ctxLabel, budget: Math.round(budgetPct) },
          })}
        </span>
      </div>
    {/if}
  {:else if onclick}
    <button
      type="button"
      class="inline-flex items-center gap-1 rounded-full leading-none bg-primary/10 px-2 py-0.5 text-micro text-primary transition-colors hover:bg-primary/20 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary"
      data-testid="context-indicator-pill"
      aria-label={$t("chat.context_inspect")}
      {onclick}
    >
      <Gauge size={10} />
      {ctxLabel}
    </button>
  {:else}
    <span
      class="inline-flex items-center gap-1 rounded-full leading-none bg-primary/10 px-2 py-0.5 text-micro text-primary"
      data-testid="context-indicator-pill"
    >
      <Gauge size={10} />
      {ctxLabel}
    </span>
  {/if}
{/if}
