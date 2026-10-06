<script lang="ts">
  /**
   * BatchHistoryScreen - every batch, newest first, whatever the agent, with
   * no ceiling on their number: the list reads on as the user scrolls. A
   * batch opened from here is read, exported and retried like a running one.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Layers, Search } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Card, EmptyState, ErrorBanner, PageLayout, SkeletonList } from "$lib/components/operator";
  import type { BatchSummary } from "$lib/ipc/batches";
  import { openBatch } from "$lib/stores/batchView";
  import { navigateTo } from "$lib/stores/navigation";
  import BatchHistoryRow from "./BatchHistoryRow.svelte";
  import {
    NO_HISTORY_FILTER,
    OUTCOMES,
    agentsOf,
    filterBatches,
    groupByPeriod,
    type BatchOutcome,
  } from "./batchView";
  import { createBatchHistory } from "./useBatchHistory.svelte";

  const history = createBatchHistory();

  let agent = $state(NO_HISTORY_FILTER.agent);
  let outcome = $state<string>(NO_HISTORY_FILTER.outcome);
  let query = $state(NO_HISTORY_FILTER.query);
  let now = $state(new Date());
  let sentinel = $state<HTMLElement | null>(null);

  onMount(() => {
    now = new Date();
    void history.load();
  });

  const shown = $derived(
    filterBatches(history.batches, { agent, outcome: outcome as BatchOutcome | "", query }),
  );
  const groups = $derived(groupByPeriod(shown, now));
  const filtered = $derived(agent !== "" || outcome !== "" || query.trim() !== "");

  // The list reads on while its end is in sight, so nothing caps the history.
  $effect(() => {
    const end = sentinel;
    if (end === null || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) void history.loadMore();
    });
    observer.observe(end);
    return () => observer.disconnect();
  });

  function open(batch: BatchSummary): void {
    openBatch(batch.batchId, "history");
  }
</script>

{#snippet headerActions()}
  <Button variant="outline" size="sm" onclick={() => navigateTo("agents")} data-testid="batch-history-to-agents">
    {$t("batch_history.to_agents")}
  </Button>
{/snippet}

<PageLayout
  kicker={$t("batch_history.kicker")}
  title={$t("batch_history.title")}
  subtitle={$t("batch_history.subtitle")}
  actions={headerActions}
  class="space-y-4 p-6"
  data-testid="batch-history-page"
>
  <div class="flex flex-wrap items-center gap-3">
    <Select bind:value={agent} size="sm" class="w-52" aria-label={$t("batch_history.filter.agent")} data-testid="batch-history-agent">
      <option value="">{$t("batch_history.filter.all_agents")}</option>
      {#each agentsOf(history.batches) as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </Select>
    <Select bind:value={outcome} size="sm" class="w-52" aria-label={$t("batch_history.filter.outcome")} data-testid="batch-history-outcome">
      <option value="">{$t("batch_history.filter.all_outcomes")}</option>
      {#each OUTCOMES as option (option)}
        <option value={option}>{$t(`batch_history.filter.outcome_${option}`)}</option>
      {/each}
    </Select>
    <Input
      bind:value={query}
      icon={Search}
      size="sm"
      class="ml-auto w-72"
      placeholder={$t("batch_history.search")}
      aria-label={$t("batch_history.search")}
      data-testid="batch-history-search"
    />
  </div>

  {#if history.error && history.batches.length === 0}
    <ErrorBanner
      tone="danger"
      message={$t("batch_history.load_failed")}
      onretry={() => void history.load()}
      loading={history.loading}
      data-testid="batch-history-load-error"
    />
  {:else if history.loading && history.batches.length === 0}
    <SkeletonList count={5} />
  {:else if history.batches.length === 0}
    <Card data-testid="batch-history-empty">
      <EmptyState tone="neutral" title={$t("batch_history.empty_title")} desc={$t("batch_history.empty_body")}>
        {#snippet icon()}
          <Layers size={22} strokeWidth={1.5} />
        {/snippet}
      </EmptyState>
    </Card>
  {:else}
    {#if shown.length === 0 && !history.hasMore}
      <Card data-testid="batch-history-empty-filtered">
        <EmptyState
          tone="neutral"
          title={$t("batch_history.empty_filtered_title")}
          desc={$t("batch_history.empty_filtered_body")}
        >
          {#snippet icon()}
            <Layers size={22} strokeWidth={1.5} />
          {/snippet}
        </EmptyState>
      </Card>
    {/if}

    {#each groups as group (group.period)}
      <section data-testid="batch-history-period" data-period={group.period}>
        <h3 class="m-0 mb-2 font-mono text-micro-lg font-semibold uppercase tracking-wider text-muted-foreground/70">
          {$t(`batch_history.period.${group.period}`)}
        </h3>
        <ul class="m-0 flex list-none flex-col gap-3 p-0">
          {#each group.batches as batch (batch.batchId)}
            <BatchHistoryRow {batch} {now} onOpen={open} />
          {/each}
        </ul>
      </section>
    {/each}

    <p class="m-0 text-caption text-muted-foreground" data-testid="batch-history-count" data-more={history.hasMore}>
      {#if history.hasMore}
        {$t("batch_history.count_more", { values: { count: history.batches.length } })}
      {:else if filtered}
        {$t("batch_history.count_filtered", { values: { shown: shown.length, count: history.batches.length } })}
      {:else}
        {$t("batch_history.count", { values: { count: history.batches.length } })}
      {/if}
    </p>

    {#if history.hasMore}
      <div bind:this={sentinel}>
        <Button
          variant="outline"
          class="w-full"
          disabled={history.loadingMore}
          onclick={() => void history.loadMore()}
          data-testid="batch-history-load-older"
        >
          {$t("batch_history.load_older")}
        </Button>
      </div>
    {/if}
  {/if}
</PageLayout>
