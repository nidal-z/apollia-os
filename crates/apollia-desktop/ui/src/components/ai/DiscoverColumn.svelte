<script lang="ts">
  /**
   * DiscoverColumn - the left column of the discovery window: the search
   * field, the two kinds of model, the mark of the Apollia selection, the
   * format, the sort with "only what fits this computer", and the results.
   */
  import { locale, t } from "svelte-i18n";
  import { RefreshCw, Search, WifiOff } from "lucide-svelte";
  import type { ModelFormat, SearchSort } from "$lib/ipc/modelHub";
  import { cn, formatRelativeTime } from "$lib/utils";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import { capabilitiesOf } from "./explanations";
  import {
    DISCOVER_CATEGORIES,
    SEARCH_SORTS,
    discoverCategoryKey,
    publisherKindKeys,
    sortLabelKey,
    type DiscoverCategory,
    type DiscoverEntry,
  } from "./discover";

  interface Props {
    query: string;
    category: DiscoverCategory;
    format: ModelFormat;
    /** The formats this machine can run. */
    formats: readonly ModelFormat[];
    sort: SearchSort;
    fitsOnly: boolean;
    /** What a model can occupy on this machine, in GB; null until probed. */
    capacityGb: number | null;
    entries: readonly DiscoverEntry[];
    selectedKey: string | null;
    /** The words typed are being searched rather than the selection shown. */
    searching: boolean;
    loading: boolean;
    offline: boolean;
    /** The search failed for another reason than the network, in its words. */
    searchError: string | null;
    /** Why the selection has no card to show, in the words of the registry. */
    selectionNote: string | null;
    onselect: (entry: DiscoverEntry) => void;
    oncategory: (category: DiscoverCategory) => void;
    onrefresh: () => void;
  }

  let {
    query = $bindable(""),
    category,
    format = $bindable("gguf"),
    formats,
    sort = $bindable("best_match"),
    fitsOnly = $bindable(false),
    capacityGb,
    entries,
    selectedKey,
    searching,
    loading,
    offline,
    searchError,
    selectionNote,
    onselect,
    oncategory,
    onrefresh,
  }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const capacity = $derived(
    capacityGb === null
      ? null
      : new Intl.NumberFormat(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(capacityGb),
  );
</script>

<div class="flex min-h-0 w-80 shrink-0 flex-col gap-3 border-r border-border p-4" data-testid="ai-discover-column">
  <Input
    size="sm"
    icon={Search}
    bind:value={query}
    placeholder={$t("ai.discover.search_placeholder")}
    aria-label={$t("ai.discover.search_label")}
    data-testid="ai-discover-search"
  />

  <div class="flex flex-wrap gap-1.5" role="group" aria-label={$t("ai.discover.categories_label")}>
    {#each DISCOVER_CATEGORIES as entry (entry)}
      <button
        type="button"
        class={cn(
          "rounded-full border px-3 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
          category === entry
            ? "border-border bg-muted font-medium text-foreground"
            : "border-border text-muted-foreground hover:bg-muted/60 hover:text-foreground",
        )}
        aria-pressed={category === entry}
        onclick={() => oncategory(entry)}
        data-testid="ai-discover-category-{entry}"
      >
        {$t(discoverCategoryKey(entry))}
      </button>
    {/each}
  </div>

  <div class="flex flex-wrap items-center gap-2">
    {#if !searching}
      <ExplainedBadge
        label={$t("ai.discover.selection")}
        explanation={$t("ai.discover.selection_explanation")}
        variant="primary"
        testid="ai-discover-selection-mark"
      />
      <Tooltip
        content={$t("ai.discover.refresh")}
        onclick={onrefresh}
        triggerLabel={$t("ai.discover.refresh")}
        triggerTestid="ai-discover-refresh"
        triggerClass="h-7 w-7 justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <RefreshCw size={13} aria-hidden="true" />
      </Tooltip>
    {:else}
      <span class="text-xs text-muted-foreground" data-testid="ai-discover-count">
        {$t("ai.discover.result_count", { values: { count: entries.length } })}
      </span>
    {/if}
    {#if category === "language"}
      <div class="ml-auto w-24">
        <Select
          size="sm"
          bind:value={format}
          disabled={formats.length < 2}
          aria-label={$t("ai.discover.format_label")}
          data-testid="ai-discover-format"
        >
          {#each formats as entry (entry)}
            <option value={entry}>{entry.toUpperCase()}</option>
          {/each}
        </Select>
      </div>
    {/if}
  </div>

  {#if category === "language"}
    <Select size="sm" bind:value={sort} aria-label={$t("ai.discover.sort_label")} data-testid="ai-discover-sort">
      {#each SEARCH_SORTS as entry (entry)}
        <option value={entry}>{$t(sortLabelKey(entry))}</option>
      {/each}
    </Select>
    <div class="flex items-start gap-2">
      <Checkbox bind:checked={fitsOnly} id="ai-discover-fits" class="mt-0.5" data-testid="ai-discover-fits" />
      <label for="ai-discover-fits" class="text-xs text-foreground">
        {$t("ai.discover.fits_only")}
        {#if capacity !== null}
          <span class="block text-muted-foreground" data-testid="ai-discover-capacity">
            {$t("ai.discover.capacity", { values: { size: capacity } })}
          </span>
        {/if}
      </label>
    </div>
  {/if}

  {#if offline}
    <p
      class="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/5 p-2 text-xs text-foreground"
      data-testid="ai-discover-offline"
    >
      <WifiOff size={13} class="mt-0.5 shrink-0 text-warning-a11y" aria-hidden="true" />
      <span>{$t(searching ? "ai.discover.offline_search" : "ai.discover.offline_selection")}</span>
    </p>
  {/if}

  <div class="min-h-0 flex-1 overflow-auto" data-testid="ai-discover-results">
    {#if loading && entries.length === 0}
      <p class="p-2 text-xs text-muted-foreground" data-testid="ai-discover-loading">{$t("common.loading")}</p>
    {:else if searching && searchError}
      <p class="p-2 text-xs text-foreground" data-testid="ai-discover-search-error">
        {$t("ai.discover.search_error", { values: { detail: searchError } })}
      </p>
    {:else if entries.length === 0}
      <p class="p-2 text-xs text-muted-foreground" data-testid="ai-discover-empty">
        {#if searching}
          {offline ? "" : $t("ai.discover.no_result", { values: { query: query.trim() } })}
        {:else if fitsOnly}
          {$t("ai.discover.none_fits")}
        {:else}
          {selectionNote ?? $t("ai.discover.selection_empty")}
        {/if}
      </p>
    {:else}
      <ul class="flex flex-col gap-0.5">
        {#each entries as entry (entry.key)}
          {@const kind = publisherKindKeys(entry.publisherKind)}
          <li>
            <div
              class={cn(
                "rounded-md px-2 py-2 transition-colors",
                selectedKey === entry.key ? "bg-muted" : "hover:bg-muted/60",
              )}
              data-testid="ai-discover-result"
              data-entry-key={entry.key}
            >
              <div class="flex items-center gap-1.5">
                <button
                  type="button"
                  class="min-w-0 flex-1 truncate text-left text-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                  aria-current={selectedKey === entry.key ? "true" : undefined}
                  onclick={() => onselect(entry)}
                  data-testid="ai-discover-result-open"
                >
                  {entry.name}
                </button>
                {#each capabilitiesOf(entry.capabilities) as capability (capability)}
                  <CapabilityIcon {capability} />
                {/each}
              </div>
              <div class="mt-1 flex flex-wrap items-center gap-1.5">
                {#if entry.recommendedHere}
                  <ExplainedBadge
                    label={$t("ai.discover.recommended_here")}
                    explanation={$t("ai.discover.recommended_here_explanation")}
                    variant="primary"
                    testid="ai-discover-result-recommended"
                  />
                {/if}
                {#if searching && entry.inSelection}
                  <ExplainedBadge
                    label={$t("ai.discover.selection")}
                    explanation={$t("ai.discover.selection_explanation")}
                    variant="primary"
                    testid="ai-discover-result-selection"
                  />
                {/if}
                {#if kind}
                  <ExplainedBadge
                    label={$t(kind.label)}
                    explanation={$t(kind.explanation)}
                    testid="ai-discover-result-publisher-kind"
                  />
                {/if}
              </div>
              <p class="mt-1 flex items-baseline gap-2 text-xs text-muted-foreground">
                <span class="shrink-0" data-testid="ai-discover-result-publisher">{entry.publisher}</span>
                <span class="min-w-0 flex-1 truncate">{entry.description}</span>
                {#if entry.updatedAt}
                  <span class="shrink-0">{formatRelativeTime(entry.updatedAt, lang)}</span>
                {/if}
              </p>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
