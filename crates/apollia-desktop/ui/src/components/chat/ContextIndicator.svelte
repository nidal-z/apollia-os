<script lang="ts">
  import { Gauge } from "lucide-svelte";
  import { sessionMetricsStore } from "$lib/stores/chatMetrics";

  interface Props {
    /** Current chat session id — required to read live metrics. */
    sessionId: string;
    /** Variant: `pill` (used in Metrics tab) or `footer` (mini under input). */
    variant?: "pill" | "footer";
    /**
     * Click handler — when supplied, the indicator becomes a button (P7 —
     * opens the `<InjectedMemorySheet />` sheet).
     */
    onclick?: () => void;
  }

  let { sessionId, variant = "footer", onclick }: Props = $props();

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
    {#if onclick}
      <button
        type="button"
        class="flex items-center justify-center gap-1.5 rounded px-3 py-1 {toneClass} text-[10px] transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary"
        data-testid="context-indicator-footer"
        aria-label="Inspecter la mémoire injectée pour ce tour"
        {onclick}
      >
        <Gauge size={10} />
        <span class="tabular-nums">
          Ctx {Math.round(contextPct)}% · Budget {Math.round(budgetPct)}%
        </span>
      </button>
    {:else}
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
    {/if}
  {:else if onclick}
    <button
      type="button"
      class="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] text-primary transition-colors hover:bg-primary/20 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary"
      data-testid="context-indicator-pill"
      aria-label="Inspecter la mémoire injectée"
      {onclick}
    >
      <Gauge size={10} />
      {Math.round(contextPct)}%
    </button>
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
