<!--
  The context gauge of the composer: a ring and a percentage, to the right of
  the text field.

  The ring says how much of the loaded context the conversation occupies; the
  hover details it in tokens, and says when the profile is part of the prompt.
  It is the only context gauge of the chat. A backend that reports no context
  window reads `--`, never an invented share.
-->
<script module lang="ts">
  import type { ContextGaugeDetail } from "$lib/chat/contextGauge";

  /** One line of the hover: what is counted and, when it is a count, its value. */
  export interface GaugeRow {
    id: "conversation_tokens" | "loaded_context" | "share_used" | "profile_included";
    labelKey: string;
    /** Tokens to print, `null` for a line that states a fact. */
    tokens: number | null;
  }

  /**
   * The lines of the hover, in order. Without a measured context only the
   * profile line can appear, and only while the profile is in the prompt.
   */
  export function gaugeRows(detail: ContextGaugeDetail | null, profileIncluded: boolean): GaugeRow[] {
    const rows: GaugeRow[] = [];
    if (detail) {
      rows.push(
        { id: "conversation_tokens", labelKey: "chat.gauge.conversation_tokens", tokens: detail.used },
        { id: "loaded_context", labelKey: "chat.gauge.loaded_context", tokens: detail.window },
        { id: "share_used", labelKey: "chat.gauge.share_used", tokens: detail.remaining },
      );
    }
    if (profileIncluded) {
      rows.push({ id: "profile_included", labelKey: "chat.gauge.profile_included", tokens: null });
    }
    return rows;
  }
</script>

<script lang="ts">
  import { locale, t } from "svelte-i18n";
  import { Check } from "lucide-svelte";
  import { mergeProps } from "bits-ui";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import {
    CONTEXT_GAUGE_HIGH_PCT,
    contextGaugeDetail,
    contextGaugeLabel,
    contextPct,
    ringDashOffset,
  } from "$lib/chat/contextGauge";
  import { chatConversationStats } from "$lib/stores/chat";
  import { sessionMetricsStore } from "$lib/stores/chatMetrics";

  interface Props {
    /** The conversation whose context is measured. */
    sessionId: string;
    /** When supplied, a click opens what was recalled for the last turn. */
    onclick?: () => void;
  }

  let { sessionId, onclick }: Props = $props();

  /** Radius of the ring, in the units of its 16 by 16 view box. */
  const RADIUS = 6;
  const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

  const metrics = $derived(sessionMetricsStore(sessionId));
  const detail = $derived(
    contextGaugeDetail($metrics.context_window_tokens, $metrics.context_tokens_used),
  );
  const label = $derived(
    contextGaugeLabel($metrics.context_window_tokens, $metrics.context_tokens_used),
  );
  const pct = $derived(contextPct($metrics.context_window_tokens, $metrics.context_tokens_used));
  const high = $derived(pct >= CONTEXT_GAUGE_HIGH_PCT);
  const rows = $derived(gaugeRows(detail, $chatConversationStats?.user_memory_injected === true));
  const hasActivity = $derived($metrics.exchanges_count > 0);

  const tokens = $derived(new Intl.NumberFormat($locale ?? undefined));
</script>

{#snippet hover()}
  <div class="flex min-w-48 flex-col gap-1">
    {#each rows as row (row.id)}
      <div class="flex items-center justify-between gap-4" data-testid="context-gauge-row-{row.id}">
        <span class="text-muted-foreground">{$t(row.labelKey)}</span>
        {#if row.id === "share_used" && detail}
          <span class="tabular-nums">
            {$t("chat.gauge.share_value", {
              values: { pct: detail.pct, remaining: tokens.format(detail.remaining) },
            })}
          </span>
        {:else if row.tokens !== null}
          <span class="tabular-nums">{tokens.format(row.tokens)}</span>
        {:else}
          <Check size={12} class="text-success" aria-hidden="true" />
        {/if}
      </div>
    {/each}
    {#if !detail}
      <p class="text-muted-foreground">
        {$t("chat.gauge.unmeasured")}
      </p>
    {/if}
  </div>
{/snippet}

{#if hasActivity}
  <Tooltip content={hover} side="top" delay={150}>
    {#snippet trigger(triggerProps)}
      <button
        {...mergeProps(triggerProps, { onclick })}
        type="button"
        class="mr-1.5 inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md px-1.5 text-caption transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary {high
          ? 'text-warning'
          : 'text-muted-foreground'}"
        aria-label={$t("chat.gauge.aria", { values: { value: label } })}
        data-testid="context-indicator-footer"
        data-high={high}
      >
        <svg viewBox="0 0 16 16" class="h-4 w-4 -rotate-90" aria-hidden="true">
          <circle
            cx="8"
            cy="8"
            r={RADIUS}
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            class="opacity-25"
          />
          <circle
            cx="8"
            cy="8"
            r={RADIUS}
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-dasharray={CIRCUMFERENCE}
            stroke-dashoffset={ringDashOffset(pct, CIRCUMFERENCE)}
          />
        </svg>
        <span class="tabular-nums" data-testid="context-gauge-percent">{label}</span>
      </button>
    {/snippet}
  </Tooltip>
{/if}
