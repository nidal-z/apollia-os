<script lang="ts">
  /**
   * The generation figures of a turn, under its answer: one line of five
   * figures, each explained on hover, and the detail behind them, one card per
   * model call, unfolded on demand.
   */

  import { t, locale } from "svelte-i18n";
  import { ChevronRight, Gauge, Hash, Clock, Timer, CircleStop, Cpu } from "lucide-svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import {
    callFieldText,
    callFields,
    generationInfoItems,
    type GenerationInfoId,
    type TurnGeneration,
  } from "$lib/chat/generationStats";
  import { formatSecondsLocalized } from "$lib/chat/duration";
  import { loadTurnCalls, turnCallsOf } from "$lib/chat/turnCalls.svelte";

  interface Props {
    /** The total written under the answer. */
    generation: TurnGeneration;
    sessionId: string;
    /** The turn, named by the identifier of its answer. */
    turnId: string;
    /** Number of tool steps the turn played. */
    stepCount?: number;
    /** Starts unfolded. */
    initiallyExpanded?: boolean;
  }

  let {
    generation,
    sessionId,
    turnId,
    stepCount = 0,
    initiallyExpanded = false,
  }: Props = $props();

  const ICONS: Record<GenerationInfoId, typeof Gauge> = {
    speed: Gauge,
    tokens: Hash,
    ttft: Clock,
    duration: Timer,
    stop: CircleStop,
  };

  const loc = $derived($locale ?? "en");
  const items = $derived(generationInfoItems(generation, loc));

  // svelte-ignore state_referenced_locally
  let expanded = $state(initiallyExpanded);
  const detail = $derived(turnCallsOf(sessionId, turnId));

  function toggle(): void {
    expanded = !expanded;
    if (expanded) void loadTurnCalls(sessionId, turnId);
  }

  const summary = $derived.by<string>(() => {
    const parts = [
      $t("chat.generation.steps_count", { values: { count: stepCount } }),
      $t("chat.generation.calls_count", { values: { count: generation.calls } }),
    ];
    if (generation.duration_ms != null) {
      parts.push(
        $t("chat.generation.seconds", {
          values: { value: formatSecondsLocalized(generation.duration_ms, loc) },
        }),
      );
    }
    return parts.join(" · ");
  });
</script>

<div class="mt-1.5 flex w-full flex-col gap-2" data-testid="chat-generation-{turnId}">
  <ul class="flex flex-wrap items-center gap-1.5" aria-label={$t("chat.generation.label")}>
    {#each items as item (item.id)}
      {@const Icon = ICONS[item.id]}
      <li>
        <Tooltip content={$t(item.helpKey)} side="bottom">
          <span
            class="inline-flex items-center gap-1 rounded-full bg-surface-2 px-2 py-0.5 text-micro-lg text-muted-foreground"
            data-testid="chat-generation-{item.id}-{turnId}"
            data-measured={item.measured}
          >
            <Icon size={12} aria-hidden="true" />
            <span>{$t(item.labelKey, { values: item.values })}</span>
          </span>
        </Tooltip>
      </li>
    {/each}
    <li>
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-micro-lg text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-expanded={expanded}
        aria-label={expanded ? $t("chat.generation.detail_hide") : $t("chat.generation.detail_show")}
        data-testid="chat-generation-detail-toggle-{turnId}"
        onclick={toggle}
      >
        <ChevronRight
          class="h-3 w-3 flex-none transition-transform duration-fast {expanded ? 'rotate-90' : ''}"
          aria-hidden="true"
        />
        <span>{$t("chat.generation.detail_toggle")}</span>
      </button>
    </li>
  </ul>

  {#if expanded}
    <div class="flex flex-col gap-2" data-testid="chat-generation-detail-{turnId}">
      <p class="text-micro-lg text-muted-foreground" data-testid="chat-generation-summary-{turnId}">
        {summary}
      </p>
      {#if detail === undefined || detail.status === "loading"}
        <p class="text-body-xs text-muted-foreground" data-testid="chat-generation-loading-{turnId}">
          {$t("chat.generation.detail_loading")}
        </p>
      {:else if detail.status === "error"}
        <p class="text-body-xs text-destructive" data-testid="chat-generation-error-{turnId}">
          {$t("chat.generation.detail_error", { values: { message: detail.message } })}
        </p>
      {:else if detail.calls.length === 0}
        <p class="text-body-xs text-muted-foreground" data-testid="chat-generation-empty-{turnId}">
          {$t("chat.generation.detail_empty")}
        </p>
      {:else}
        {#each detail.calls as call, index (index)}
          {@const fields = callFields(call, loc)}
          <section
            class="rounded-lg border border-border bg-card px-3 py-2"
            data-testid="chat-generation-call-{turnId}-{index}"
          >
            <h4 class="flex items-center gap-1.5 text-body-sm font-medium text-foreground">
              <Cpu size={14} class="flex-none text-muted-foreground" aria-hidden="true" />
              <span>{$t("chat.generation.call_title", { values: { n: index + 1 } })}</span>
            </h4>
            <dl class="mt-1 flex flex-col gap-0.5 text-body-xs">
              {#each fields as field (field.id)}
                <div class="flex items-baseline justify-between gap-3">
                  <dt class="text-muted-foreground">{$t(field.nameKey)}</dt>
                  <dd
                    class="min-w-0 truncate text-right text-foreground"
                    data-testid="chat-generation-call-{field.id}-{turnId}-{index}"
                  >
                    {callFieldText(field, $t)}
                  </dd>
                </div>
              {/each}
            </dl>
          </section>
        {/each}
      {/if}
    </div>
  {/if}
</div>
