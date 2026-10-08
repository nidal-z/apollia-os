<script lang="ts">
  /**
   * DiscoverColumn - the gallery of the discovery window, across its whole
   * width: what the model is for, the search field, the mark of the Apollia
   * selection, the format, the sort with "only what fits this computer", and
   * the results as rows of one list, those that fit this computer first.
   */
  import { locale, t } from "svelte-i18n";
  import { ChevronRight, FileSearch, MessageSquare, Mic, RefreshCw, Search, WifiOff, X } from "lucide-svelte";
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
    onclose: () => void;
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
    onclose,
  }: Props = $props();

  const USAGE_ICONS = { language: MessageSquare, embeddings: FileSearch, dictation: Mic } as const;
  const USAGE_KEYS: Record<DiscoverCategory, string> = {
    language: "ai.discover.usage.language",
    embeddings: "ai.discover.usage.embeddings",
    dictation: "ai.discover.usage.dictation",
  };

  /** Whether the entry is known to fit what this computer can give a model. */
  function fitsHere(entry: DiscoverEntry): boolean {
    return capacityGb === null || entry.minMemoryGb === null || entry.minMemoryGb <= capacityGb;
  }

  // The cards in two groups, what fits this computer first. One group only
  // while the capacity is unknown, or when everything falls on the same side.
  const groups = $derived.by(() => {
    const fitting = entries.filter(fitsHere);
    const heavier = entries.filter((entry) => !fitsHere(entry));
    if (capacityGb === null || heavier.length === 0 || fitting.length === 0) {
      return [{ key: "all", title: null as string | null, entries: [...entries] }];
    }
    return [
      { key: "fits", title: "ai.discover.group_fits" as string | null, entries: fitting },
      { key: "heavier", title: "ai.discover.group_heavier" as string | null, entries: heavier },
    ];
  });

  const lang = $derived($locale ?? "fr");
  const capacity = $derived(
    capacityGb === null
      ? null
      : new Intl.NumberFormat(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(capacityGb),
  );
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col" data-testid="ai-discover-column">
  <header class="flex flex-col gap-4 border-b border-border px-6 pb-4 pt-5">
    <div class="flex items-start gap-3">
      <div class="min-w-0 flex-1">
        <h2 class="text-lg font-semibold text-foreground">{$t("ai.discover.title")}</h2>
        <p class="mt-0.5 text-sm text-muted-foreground" data-testid="ai-discover-card-empty">
          {$t("ai.discover.gallery_subtitle")}
        </p>
      </div>
      <Tooltip
        content={$t("common.close")}
        onclick={onclose}
        triggerLabel={$t("common.close")}
        triggerTestid="ai-discover-close-empty"
        triggerClass="h-7 w-7 justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <X size={16} aria-hidden="true" />
      </Tooltip>
    </div>

    <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label={$t("ai.discover.categories_label")}>
      {#each DISCOVER_CATEGORIES as entry (entry)}
        {@const Icon = USAGE_ICONS[entry]}
        <button
          type="button"
          class={cn(
            "flex items-center gap-2 rounded-full border px-3 py-1.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            category === entry
              ? "border-primary/40 bg-primary/5 font-medium text-foreground"
              : "border-border text-muted-foreground hover:bg-muted/60 hover:text-foreground",
          )}
          aria-pressed={category === entry}
          onclick={() => oncategory(entry)}
          data-testid="ai-discover-category-{entry}"
        >
          <Icon size={14} aria-hidden="true" />
          {$t(USAGE_KEYS[entry])}
        </button>
      {/each}
    </div>

    <div class="flex items-center gap-2.5">
      <div class="min-w-0 flex-1">
        <Input
          size="sm"
          icon={Search}
          bind:value={query}
          placeholder={$t("ai.discover.search_placeholder")}
          aria-label={$t("ai.discover.search_label")}
          data-testid="ai-discover-search"
        />
      </div>
      {#if category === "language"}
        <div class="w-44 shrink-0">
          <Select size="sm" bind:value={sort} aria-label={$t("ai.discover.sort_label")} data-testid="ai-discover-sort">
            {#each SEARCH_SORTS as entry (entry)}
              <option value={entry}>{$t(sortLabelKey(entry))}</option>
            {/each}
          </Select>
        </div>
        <div class="w-24 shrink-0">
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

    <div class="flex flex-wrap items-center gap-2.5">
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
        <div class="ml-auto flex items-center gap-2">
          <Checkbox bind:checked={fitsOnly} id="ai-discover-fits" data-testid="ai-discover-fits" />
          <label for="ai-discover-fits" class="text-xs text-foreground">
            {$t("ai.discover.fits_only")}
            {#if capacity !== null}
              <span class="text-muted-foreground" data-testid="ai-discover-capacity">
                · {$t("ai.discover.capacity", { values: { size: capacity } })}
              </span>
            {/if}
          </label>
        </div>
      {/if}
    </div>

    {#if offline}
      <p
        class="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/5 p-2 text-xs text-foreground"
        data-testid="ai-discover-offline"
      >
        <WifiOff size={13} class="mt-0.5 shrink-0 text-warning-a11y" aria-hidden="true" />
        <span>{$t(searching ? "ai.discover.offline_search" : "ai.discover.offline_selection")}</span>
      </p>
    {/if}
  </header>

  <div class="min-h-0 flex-1 overflow-auto px-6 py-5" data-testid="ai-discover-results">
    {#if loading && entries.length === 0}
      <p class="text-sm text-muted-foreground" data-testid="ai-discover-loading">{$t("common.loading")}</p>
    {:else if searching && searchError}
      <p class="text-sm text-foreground" data-testid="ai-discover-search-error">
        {$t("ai.discover.search_error", { values: { detail: searchError } })}
      </p>
    {:else if entries.length === 0}
      <p class="text-sm text-muted-foreground" data-testid="ai-discover-empty">
        {#if searching}
          {offline ? "" : $t("ai.discover.no_result", { values: { query: query.trim() } })}
        {:else if fitsOnly}
          {$t("ai.discover.none_fits")}
        {:else}
          {selectionNote ?? $t("ai.discover.selection_empty")}
        {/if}
      </p>
    {:else}
      {#each groups as group (group.key)}
        {#if group.title}
          <h3
            class="mb-2 mt-1 text-overline uppercase text-muted-foreground first:mt-0"
            data-testid="ai-discover-group-{group.key}"
          >
            {$t(group.title)} · {group.entries.length}
          </h3>
        {/if}
        <ul class="mb-5 divide-y divide-border/60 overflow-hidden rounded-xl border border-border bg-card shadow-sm">
          {#each group.entries as entry (entry.key)}
            {@const kind = publisherKindKeys(entry.publisherKind)}
            <li class="min-w-0">
              <div
                class={cn(
                  "relative flex items-center gap-4 px-4 py-2.5 transition-colors",
                  selectedKey === entry.key ? "bg-primary/5" : "hover:bg-muted/40",
                )}
                data-testid="ai-discover-result"
                data-entry-key={entry.key}
              >
                <div class="flex w-56 shrink-0 items-center gap-1.5">
                  <button
                    type="button"
                    class="min-w-0 truncate text-left text-sm font-semibold text-foreground after:absolute after:inset-0 focus-visible:outline-none focus-visible:after:ring-2 focus-visible:after:ring-inset focus-visible:after:ring-ring/60"
                    aria-current={selectedKey === entry.key ? "true" : undefined}
                    onclick={() => onselect(entry)}
                    data-testid="ai-discover-result-open"
                  >
                    {entry.name}
                  </button>
                  <span class="relative flex shrink-0 items-center gap-1">
                    {#each capabilitiesOf(entry.capabilities) as capability (capability)}
                      <CapabilityIcon {capability} />
                    {/each}
                  </span>
                </div>
                <div class="min-w-0 flex-1">
                  <p class="truncate text-sm text-foreground/80">{entry.description}</p>
                  <div class="relative mt-0.5 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                    <span data-testid="ai-discover-result-publisher">{entry.publisher}</span>
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
                    {#if entry.updatedAt}
                      <span>· {formatRelativeTime(entry.updatedAt, lang)}</span>
                    {/if}
                  </div>
                </div>
                <div class="w-40 shrink-0 text-right">
                  {#if entry.minMemoryGb !== null}
                    <p class="text-sm tabular-nums text-foreground" data-testid="ai-discover-result-memory">
                      {$t("ai.discover.min_memory", { values: { size: entry.minMemoryGb } })}
                    </p>
                    {#if capacityGb !== null}
                      <p class="text-xs {fitsHere(entry) ? 'text-success' : 'text-muted-foreground'}">
                        {$t(fitsHere(entry) ? "ai.discover.fit_yes" : "ai.discover.fit_no")}
                      </p>
                    {/if}
                  {/if}
                </div>
                <ChevronRight size={15} class="shrink-0 text-muted-foreground" aria-hidden="true" />
              </div>
            </li>
          {/each}
        </ul>
      {/each}
    {/if}
  </div>
</div>
