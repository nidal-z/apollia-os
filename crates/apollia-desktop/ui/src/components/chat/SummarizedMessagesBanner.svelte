<script lang="ts">
  /**
   * Banner affichant les événements de summarization accumulés pour une session
   * (US-SP42-047 Pattern P11 — refactor).
   *
   * Deux modes supportés pour rester rétro-compatible :
   * - `events` : liste structurée venant de `SessionMetrics.summarization_events`.
   * - `summarizedCount` + `summaryText` : variante legacy (un seul résumé).
   */
  import { slide } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { ChevronDown, ChevronUp } from "lucide-svelte";
  import type { SummarizationEvent } from "$lib/types";

  interface Props {
    /** Liste structurée des événements de summarization. */
    events?: SummarizationEvent[];
    /** Legacy : count pour compat avec anciens appelants. */
    summarizedCount?: number;
    /** Legacy : unique résumé concaténé. */
    summaryText?: string;
  }

  let { events = [], summarizedCount, summaryText }: Props = $props();

  let isExpanded = $state(false);

  const totalCount = $derived(
    events.reduce((acc, ev) => acc + ev.messages_summarized_count, 0) +
      (summarizedCount ?? 0),
  );

  const totalTokensSaved = $derived(
    events.reduce((acc, ev) => acc + ev.tokens_saved, 0),
  );

  const hasContent = $derived(totalCount > 0 || (summaryText?.length ?? 0) > 0);
</script>

{#if hasContent}
  <div
    class="rounded-lg border border-secondary/20 glass-inset px-4 py-2"
    data-testid="summarized-banner"
  >
    <button
      class="flex w-full items-center justify-between text-left"
      onclick={() => (isExpanded = !isExpanded)}
      aria-expanded={isExpanded}
    >
      <span class="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span>
          {$t("chat.summarized_messages", {
            values: { count: totalCount },
            default: `${totalCount} messages résumés`,
          })}
        </span>
        {#if totalTokensSaved > 0}
          <span
            class="rounded bg-muted/40 px-1.5 py-0.5 font-mono text-[10px]"
            title="Tokens économisés"
          >
            −{totalTokensSaved.toLocaleString()} tok
          </span>
        {/if}
      </span>
      <span class="flex items-center gap-1 text-[10px] text-muted-foreground/70">
        <span>Show what was summarized</span>
        {#if isExpanded}
          <ChevronUp size={12} />
        {:else}
          <ChevronDown size={12} />
        {/if}
      </span>
    </button>

    {#if isExpanded}
      <div
        class="mt-2 flex flex-col gap-2"
        transition:slide={{ duration: 300 }}
        data-testid="summarized-banner-content"
      >
        {#if summaryText}
          <div class="rounded glass-inset px-3 py-2">
            <p class="text-[11px] italic leading-relaxed text-muted-foreground/70">
              {summaryText}
            </p>
          </div>
        {/if}
        {#each events as ev, i (i)}
          <div class="rounded glass-inset px-3 py-2" data-testid="summarization-event">
            <header class="mb-1 flex justify-between text-[10px] text-muted-foreground/60 tabular-nums">
              <span>{ev.messages_summarized_count} messages</span>
              <span>−{ev.tokens_saved.toLocaleString()} tokens</span>
            </header>
            <p class="text-[11px] italic leading-relaxed text-muted-foreground/70">
              {ev.summary_excerpt}
            </p>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
