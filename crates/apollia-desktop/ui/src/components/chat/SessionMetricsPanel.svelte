<script lang="ts">
  import { onMount } from "svelte";
  import { Coins, Clock, Gauge, Wrench, Zap } from "lucide-svelte";
  import { ProgressBar } from "$lib/components/ui/progress";
  import {
    refreshSessionMetrics,
    sessionMetricsStore,
  } from "$lib/stores/chatMetrics";
  import {
    hydrateSessionMetrics,
    initSessionMetricsListener,
    sessionMetricsFor,
  } from "$lib/stores/sessionMetricsV2";
  import ContextWindowBar from "./ContextWindowBar.svelte";
  import ToolTimingList from "./ToolTimingList.svelte";
  import SummarizedMessagesBanner from "./SummarizedMessagesBanner.svelte";
  import {
    estimateCost,
    resolvePricing,
    costToTokensEquivalent,
  } from "$lib/pricing";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  const metrics = $derived(sessionMetricsStore(sessionId));

  // P11 — live snapshot driven by `SessionMetricsUpdated` runtime events.
  const slot = $derived(sessionMetricsFor(sessionId));
  const alertLevel = $derived($slot.alert);
  const p11 = $derived($slot.metrics);

  let toastShown = $state<null | "warning" | "block">(null);
  $effect(() => {
    if (alertLevel === "ok") {
      toastShown = null;
      return;
    }
    if (toastShown === alertLevel) return;
    toastShown = alertLevel;
    // Toast non-bloquant : on pousse dans `console` + une div aria-live ci-dessous.
    // Le système de notifications Tauri gère déjà les alertes bloquantes.
    if (typeof window !== "undefined" && alertLevel === "block") {
      console.warn(
        `[session-metrics] budget bloqué pour la session ${sessionId}`,
      );
    }
  });

  onMount(() => {
    void refreshSessionMetrics(sessionId, true);
    initSessionMetricsListener();
    void hydrateSessionMetrics(sessionId);
  });

  // Pricing resolution — use backend `cost_usd` when present, otherwise
  // estimate from the client-side pricing table.
  const pricing = $derived(resolvePricing($metrics.llm_backend));
  const estimatedCost = $derived(
    $metrics.cost_usd ??
      estimateCost(pricing, $metrics.prompt_tokens, $metrics.completion_tokens),
  );
  const tokensEquivalent = $derived(costToTokensEquivalent(estimatedCost, pricing));

  // Context window usage — messages_in_history vs window size (safe fallback).
  const contextWindowPct = $derived(
    $metrics.context_window_size > 0
      ? Math.min(
          100,
          ($metrics.messages_in_history / $metrics.context_window_size) * 100,
        )
      : 0,
  );
  const contextVariant = $derived(
    contextWindowPct >= 90
      ? "destructive"
      : contextWindowPct >= 75
        ? "warning"
        : "primary",
  );

  // Step budget — steps_used vs budget_max_steps.
  const budgetPct = $derived(
    $metrics.budget_max_steps > 0
      ? Math.min(100, ($metrics.steps_used / $metrics.budget_max_steps) * 100)
      : 0,
  );
  const budgetVariant = $derived(
    budgetPct >= 80 ? "warning" : budgetPct >= 95 ? "destructive" : "primary",
  );

  // Duration — from started_at to updated_at (session LLM time, best-effort).
  const durationLabel = $derived.by(() => {
    if (!$metrics.started_at) return "—";
    const start = new Date($metrics.started_at).getTime();
    const end = $metrics.updated_at
      ? new Date($metrics.updated_at).getTime()
      : Date.now();
    const secs = Math.max(0, Math.floor((end - start) / 1000));
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${s.toString().padStart(2, "0")}s`;
  });

  // Top 5 tools by call count (descending).
  const topTools = $derived(
    [...$metrics.tool_stats]
      .sort((a, b) => b.calls - a.calls)
      .slice(0, 5),
  );

  const formatTokens = (n: number) =>
    n >= 1_000_000
      ? `${(n / 1_000_000).toFixed(2)}M`
      : n >= 1000
        ? `${(n / 1000).toFixed(1)}k`
        : n.toString();

  const formatCost = (usd: number) =>
    usd < 0.01 ? `<$0.01` : `$${usd.toFixed(usd < 1 ? 4 : 2)}`;
</script>

<div
  class="flex flex-col gap-4 px-4 py-4"
  data-testid="session-metrics-panel"
>
  <!-- Tokens card -->
  <section class="glass-card glass-border rounded-lg p-3">
    <header class="mb-2 flex items-center gap-2 text-[11px] font-medium text-primary">
      <Zap size={12} />
      <span>Tokens</span>
    </header>
    <div class="grid grid-cols-2 gap-2 text-[11px]">
      <div class="flex flex-col">
        <span class="text-muted-foreground">Input</span>
        <span class="tabular-nums font-medium" data-testid="metrics-input-tokens">
          {formatTokens($metrics.prompt_tokens)}
        </span>
      </div>
      <div class="flex flex-col">
        <span class="text-muted-foreground">Output</span>
        <span class="tabular-nums font-medium" data-testid="metrics-output-tokens">
          {formatTokens($metrics.completion_tokens)}
        </span>
      </div>
      {#if $metrics.cache_read_input_tokens > 0 || $metrics.cache_write_input_tokens > 0}
        <div class="flex flex-col">
          <span class="text-muted-foreground">Cache read</span>
          <span class="tabular-nums text-secondary">
            {formatTokens($metrics.cache_read_input_tokens)}
          </span>
        </div>
        <div class="flex flex-col">
          <span class="text-muted-foreground">Cache write</span>
          <span class="tabular-nums text-secondary">
            {formatTokens($metrics.cache_write_input_tokens)}
          </span>
        </div>
      {/if}
    </div>
    <div class="mt-3">
      <ProgressBar
        value={budgetPct}
        label={`Budget ${$metrics.steps_used}/${$metrics.budget_max_steps || "∞"}`}
        variant={budgetVariant}
        size="sm"
      />
    </div>
  </section>

  <!-- Cost card -->
  <section class="glass-card glass-border rounded-lg p-3">
    <header class="mb-2 flex items-center gap-2 text-[11px] font-medium text-primary">
      <Coins size={12} />
      <span>Coût estimé</span>
    </header>
    <div class="flex items-end justify-between">
      <div>
        <div class="text-lg font-semibold tabular-nums" data-testid="metrics-cost">
          {formatCost(estimatedCost)}
        </div>
        <div class="text-[10px] text-muted-foreground/70">
          ≈ {formatTokens(tokensEquivalent)} tokens-équivalents
        </div>
      </div>
      <div class="text-right text-[10px] text-muted-foreground">
        <div>{pricing.label}</div>
        {#if $metrics.cost_usd === null}
          <div class="italic text-muted-foreground/60">estimé</div>
        {/if}
      </div>
    </div>
  </section>

  <!-- Duration & context window card -->
  <section class="glass-card glass-border rounded-lg p-3">
    <header class="mb-2 flex items-center gap-2 text-[11px] font-medium text-primary">
      <Clock size={12} />
      <span>Durée &amp; contexte</span>
    </header>
    <div class="mb-3 grid grid-cols-2 gap-2 text-[11px]">
      <div class="flex flex-col">
        <span class="text-muted-foreground">Session</span>
        <span class="tabular-nums font-medium">{durationLabel}</span>
      </div>
      <div class="flex flex-col">
        <span class="text-muted-foreground">Échanges</span>
        <span class="tabular-nums font-medium">{$metrics.exchanges_count}</span>
      </div>
    </div>
    <ProgressBar
      value={contextWindowPct}
      label={`Fenêtre ${$metrics.messages_in_history}/${$metrics.context_window_size || "∞"} msgs`}
      variant={contextVariant}
      size="sm"
    />
    {#if contextWindowPct >= 90}
      <p class="mt-1.5 text-[10px] text-warning">
        Compression imminente — les messages les plus anciens seront résumés.
      </p>
    {/if}
  </section>

  <!-- Tool stats card -->
  <section class="glass-card glass-border rounded-lg p-3">
    <header class="mb-2 flex items-center gap-2 text-[11px] font-medium text-primary">
      <Wrench size={12} />
      <span>Outils — Top 5</span>
    </header>
    {#if topTools.length === 0}
      <p class="text-[11px] italic text-muted-foreground/60">
        Aucun appel d'outil enregistré.
      </p>
    {:else}
      <ul class="flex flex-col gap-1.5 text-[11px]">
        {#each topTools as tool (tool.tool_name)}
          <li
            class="flex items-center justify-between rounded-md bg-muted/30 px-2 py-1"
            data-testid="metrics-tool-row"
          >
            <span class="truncate font-mono text-[10px]">{tool.tool_name}</span>
            <span class="flex items-center gap-2 tabular-nums text-muted-foreground">
              <span>{tool.executed}/{tool.calls}</span>
              {#if tool.refused > 0}
                <span class="text-destructive" title="Refusés">✗{tool.refused}</span>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <!-- P11 — Context window bar (US-SP42-047) -->
  {#if p11.context_window_max > 0}
    <section
      class="glass-card glass-border rounded-lg p-3"
      data-testid="p11-context-section"
    >
      <ContextWindowBar
        used={p11.context_window_used}
        max={p11.context_window_max}
      />
    </section>
  {/if}

  <!-- P11 — Budget alert toast (aria-live for a11y) -->
  {#if alertLevel !== "ok"}
    <div
      role="status"
      aria-live="polite"
      class="rounded-md border px-3 py-2 text-[11px]"
      class:border-warning={alertLevel === "warning"}
      class:text-warning={alertLevel === "warning"}
      class:border-destructive={alertLevel === "block"}
      class:text-destructive={alertLevel === "block"}
      data-testid="budget-alert-toast"
      data-level={alertLevel}
    >
      {#if alertLevel === "warning"}
        ⚠︎ Budget tokens session proche de la limite.
      {:else}
        ⛔ Budget tokens session atteint — nouvelles actions bloquantes.
      {/if}
    </div>
  {/if}

  <!-- P11 — Tool timings -->
  {#if p11.tool_timings.length > 0}
    <section class="glass-card glass-border rounded-lg p-3">
      <header class="mb-2 text-[11px] font-medium text-primary">
        Tool timings
      </header>
      <ToolTimingList timings={p11.tool_timings} />
    </section>
  {/if}

  <!-- P11 — Summarization breakdown -->
  {#if p11.summarization_events.length > 0}
    <SummarizedMessagesBanner events={p11.summarization_events} />
  {/if}

  <!-- Mini context pill (replaces header ContextIndicator - B.13) -->
  <div
    class="flex items-center justify-center gap-1.5 rounded-full bg-muted/20 px-3 py-1 text-[10px] text-muted-foreground/70"
    data-testid="metrics-context-pill"
  >
    <Gauge size={10} />
    <span>Contexte {Math.round(contextWindowPct)}% · Budget {Math.round(budgetPct)}%</span>
  </div>
</div>
