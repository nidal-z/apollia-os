<script lang="ts">
  /**
   * JournalScreen - what the agents did and every access change, one line per
   * entry, in plain language and without content, newest first and grouped by
   * day. Five filters narrow the list; the route applies them. "See more"
   * opens the full detail of an entry in the detail sidebar, for every user.
   * In developer mode every entry arrives unfolded in the list, with its
   * technical details, and the header folds them all at once.
   */
  import { onDestroy, untrack } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { ScrollText } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Card, EmptyState, ErrorBanner, PageLayout, SkeletonList } from "$lib/components/operator";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import type { JournalLine } from "$lib/ipc/journal";
  import { agents } from "$lib/stores/agents";
  import { detailSidebar } from "$lib/stores/detailSidebar";
  import { developerMode } from "$lib/stores/developerMode";
  import { journalSelection } from "$lib/stores/journalSelection";
  import JournalFilterBar from "./JournalFilterBar.svelte";
  import JournalRow from "./JournalRow.svelte";
  import {
    NO_FILTER,
    activeFilterCount,
    anyUnfolded,
    expansionFor,
    flip,
    foldAll,
    groupByDay,
    isUnfolded,
    lineKey,
    visibleLines,
    type DayGroup,
    type Expansion,
    type JournalFilters,
  } from "./journal";
  import { createJournal } from "./useJournal.svelte";

  const journal = createJournal();

  let filters = $state<JournalFilters>({ ...NO_FILTER });
  let expansion = $state<Expansion>(expansionFor(false));
  let selectedKey = $state<string | null>(null);
  let readAt = $state(new Date());

  // The filters and the mode decide what the route is asked for.
  $effect(() => {
    const asked = { ...filters };
    const developer = $developerMode;
    untrack(() => {
      expansion = expansionFor(developer);
      closeEntry();
      readAt = new Date();
      void journal.load(asked, developer);
    });
  });

  const shown = $derived(visibleLines(journal.lines, $developerMode));
  const groups = $derived(groupByDay(shown, readAt));
  const filterCount = $derived(activeFilterCount(filters));
  const unfoldedKeys = $derived(
    $developerMode ? shown.map(lineKey).filter((key) => isUnfolded(expansion, key)) : [],
  );

  // An unfolded entry reads its detail once.
  $effect(() => {
    const wanted = new Set(unfoldedKeys);
    const lines = shown;
    untrack(() => {
      for (const line of lines) if (wanted.has(lineKey(line))) void journal.ensureDetail(line);
    });
  });

  const selected = $derived(
    selectedKey === null ? null : (shown.find((line) => lineKey(line) === selectedKey) ?? null),
  );

  // The detail sidebar shows the selected entry, and its detail as it arrives.
  $effect(() => {
    if (selected === null) {
      journalSelection.set(null);
      return;
    }
    journalSelection.set({ line: selected, state: journal.detailOf(selected) ?? { status: "loading" } });
  });

  const sidebarOnEntry = $derived($detailSidebar.open && $detailSidebar.tab === "entry");

  function closeEntry(): void {
    selectedKey = null;
    journalSelection.set(null);
    detailSidebar.closeEntry();
  }

  onDestroy(closeEntry);

  /** "See more" and "see less": in the list in developer mode, in the detail sidebar otherwise. */
  function toggle(line: JournalLine): void {
    const key = lineKey(line);
    if ($developerMode) {
      expansion = flip(expansion, key);
      return;
    }
    if (selectedKey === key && sidebarOnEntry) {
      closeEntry();
      return;
    }
    selectedKey = key;
    void journal.ensureDetail(line);
    detailSidebar.openEntry();
  }

  function agentName(agent: string | null): string {
    if (agent === null) return $t("journal.agent_unknown");
    return agent === APOLLIA_AGENT_ID ? $t("authorizations.apollia_name") : agent;
  }

  const filterAgents = $derived([
    { id: APOLLIA_AGENT_ID, name: $t("authorizations.apollia_name") },
    ...$agents
      .filter((agent) => agent.agent_type !== "worker")
      .map((agent) => ({ id: agent.name, name: agent.name })),
  ]);

  function dayLabel(group: DayGroup): string {
    if (group.date === null) return $t("journal.day_unknown");
    const date = group.date.toLocaleDateString($locale ?? "en", {
      weekday: "long",
      day: "numeric",
      month: "long",
    });
    if (group.relative === "today") return $t("journal.day_today", { values: { date } });
    if (group.relative === "yesterday") return $t("journal.day_yesterday", { values: { date } });
    return date.charAt(0).toLocaleUpperCase($locale ?? "en") + date.slice(1);
  }
</script>

{#snippet headerActions()}
  {#if $developerMode && shown.length > 0}
    {@const someUnfolded = anyUnfolded(expansion, shown.map(lineKey))}
    <Button
      variant="ghost"
      size="sm"
      onclick={() => (expansion = foldAll(!someUnfolded))}
      data-testid="journal-fold-all"
    >
      {$t(someUnfolded ? "dev.journal.fold_all" : "dev.journal.unfold_all")}
    </Button>
  {/if}
{/snippet}

<PageLayout
  title={$t("journal.title")}
  subtitle={$t("journal.subtitle")}
  actions={headerActions}
  class="space-y-4 p-6"
  data-testid="journal-page"
>
  <JournalFilterBar
    {filters}
    agents={filterAgents}
    developerMode={$developerMode}
    onChange={(next) => (filters = next)}
  />

  <div class="flex flex-wrap items-center gap-3 text-caption text-muted-foreground">
    <span data-testid="journal-count">
      {#if filterCount > 0}
        {$t("journal.count_filtered", { values: { filters: filterCount, count: shown.length } })}
      {:else}
        {$t("journal.count", { values: { count: shown.length } })}
      {/if}
      {#if $developerMode}· {$t("dev.journal.unfolded_note")}{/if}
    </span>
    {#if filterCount > 0}
      <Button
        variant="link"
        size="sm"
        onclick={() => (filters = { ...NO_FILTER })}
        data-testid="journal-filters-clear"
      >
        {$t("journal.filter.clear")}
      </Button>
    {/if}
  </div>

  {#if journal.error}
    <ErrorBanner
      tone="danger"
      message={$t("journal.load_failed")}
      onretry={() => void journal.load(filters, $developerMode)}
      loading={journal.loading}
      data-testid="journal-load-error"
    />
  {:else if journal.loading && journal.lines.length === 0}
    <SkeletonList count={5} />
  {:else if shown.length === 0 && filterCount > 0}
    <Card data-testid="journal-empty-filtered">
      <EmptyState
        tone="neutral"
        title={$t("journal.empty_filtered_title")}
        desc={$t("journal.empty_filtered_body")}
      >
        {#snippet icon()}
          <ScrollText size={22} strokeWidth={1.5} />
        {/snippet}
      </EmptyState>
    </Card>
  {:else if shown.length === 0}
    <Card data-testid="journal-empty">
      <EmptyState tone="neutral" title={$t("journal.empty_title")} desc={$t("journal.empty_body")}>
        {#snippet icon()}
          <ScrollText size={22} strokeWidth={1.5} />
        {/snippet}
      </EmptyState>
    </Card>
  {:else}
    <Card class="overflow-hidden" data-testid="journal-list">
      {#each groups as group (group.day)}
        <section data-testid="journal-day" data-day={group.day}>
          <h3 class="m-0 border-b border-t border-border bg-muted/40 px-4 py-2 text-caption font-normal text-muted-foreground first:border-t-0">
            {dayLabel(group)}
          </h3>
          <ul class="m-0 list-none p-0">
            {#each group.lines as line (lineKey(line))}
              {@const key = lineKey(line)}
              <JournalRow
                {line}
                {agentName}
                unfolded={$developerMode && isUnfolded(expansion, key)}
                selected={!$developerMode && selectedKey === key && sidebarOnEntry}
                detail={journal.detailOf(line)}
                onToggle={toggle}
              />
            {/each}
          </ul>
        </section>
      {/each}
    </Card>

    {#if journal.hasMore}
      <Button
        variant="outline"
        class="w-full"
        disabled={journal.loadingMore}
        onclick={() => void journal.loadMore()}
        data-testid="journal-load-older"
      >
        {$t("journal.load_older")}
      </Button>
    {/if}
  {/if}
</PageLayout>
