<script lang="ts">
  /**
   * DiscoverColumn - the discovery window: across its width, what the model
   * is for, the search field, the source, the format and the sort with "only
   * what fits this computer"; under them the models as a list gathered by who
   * made them, and beside it the card of the one that is chosen.
   */
  import type { Snippet } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { ChevronDown, ChevronRight, FileSearch, MessageSquare, Mic, RefreshCw, Search, WifiOff, X } from "lucide-svelte";
  import type { ModelFormat, SearchSort } from "$lib/ipc/modelHub";
  import type { FormatChoice } from "./discover";
  import { cn, formatRelativeTime } from "$lib/utils";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import PublisherMark from "./PublisherMark.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import { capabilitiesOf } from "./explanations";
  import {
    DISCOVER_CATEGORIES,
    SEARCH_SORTS,
    makerGroups,
    publisherKindKeys,
    sortLabelKey,
    type DiscoverCategory,
    type DiscoverEntry,
  } from "./discover";

  interface Props {
    query: string;
    category: DiscoverCategory;
    format: FormatChoice;
    /** The formats this machine can run. */
    formats: readonly ModelFormat[];
    sort: SearchSort;
    fitsOnly: boolean;
    /** Where the language models come from. */
    source: "selection" | "hub";
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
    /** The rows are gathered by who made the model, rather than listed in the order of a search. */
    grouped: boolean;
    /** The card of the chosen model, shown beside the list. */
    card?: Snippet;
    onselect: (entry: DiscoverEntry) => void;
    oncategory: (category: DiscoverCategory) => void;
    onrefresh: () => void;
    onclose: () => void;
  }

  let {
    query = $bindable(""),
    category,
    format = $bindable("all"),
    formats,
    sort = $bindable("best_match"),
    fitsOnly = $bindable(false),
    source = $bindable("selection"),
    capacityGb,
    entries,
    selectedKey,
    searching,
    loading,
    offline,
    searchError,
    selectionNote,
    grouped,
    card,
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

  // Under each maker its models; a search by words keeps its own order in
  // one list, since the order is then the answer.
  const makers = $derived(grouped ? makerGroups(entries) : []);

  // One maker kept by the row of makers above the list; null shows them all.
  let onlyMaker = $state<string | null>(null);
  const kept = $derived(makers.some((group) => group.maker === onlyMaker) ? onlyMaker : null);

  const groups = $derived(
    grouped
      ? makers.filter((group) => kept === null || group.maker === kept)
      : [{ maker: "", entries: [...entries] }],
  );

  // The makers the user folded, for the time the window stays open.
  let folded = $state<Set<string>>(new Set());

  function toggle(maker: string): void {
    const next = new Set(folded);
    if (!next.delete(maker)) next.add(maker);
    folded = next;
  }

  // With a card open the list keeps a narrow column and its rows one line of facts.
  const compact = $derived(selectedKey !== null);

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
        <div class="w-40 shrink-0">
          <Select
            size="sm"
            bind:value={format}
            disabled={formats.length < 2}
            aria-label={$t("ai.discover.format_label")}
            data-testid="ai-discover-format"
          >
            {#if formats.length > 1}
              <option value="all">{$t("ai.discover.format_all")}</option>
              {#each formats as entry (entry)}
                <option value={entry}>{entry.toUpperCase()}</option>
              {/each}
            {:else}
              <option value={format}>{(formats[0] ?? "gguf").toUpperCase()}</option>
            {/if}
          </Select>
        </div>
      {/if}
    </div>

    <div class="flex flex-wrap items-center gap-2.5">
      {#if category === "language"}
        <!-- The source is a choice one can see and change, not a mark. -->
        <div class="inline-flex items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5" role="group">
          <button
            type="button"
            class={cn(
              "rounded-md px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
              source === "selection" ? "bg-card font-medium text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
            )}
            aria-pressed={source === "selection"}
            onclick={() => {
              source = "selection";
              query = "";
            }}
            data-testid="ai-discover-selection-mark"
          >
            {$t("ai.discover.selection")}
          </button>
          <button
            type="button"
            class={cn(
              "rounded-md px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
              source === "hub" ? "bg-card font-medium text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
            )}
            aria-pressed={source === "hub"}
            onclick={() => (source = "hub")}
            data-testid="ai-discover-source-hub"
          >
            {$t("ai.discover.source_hub")}
          </button>
        </div>
      {/if}
      {#if !searching}
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

  <div class="flex min-h-0 flex-1">
  <div
    class={cn(
      "min-h-0 overflow-auto py-4",
      compact ? "w-[360px] shrink-0 border-r border-border px-3" : "flex-1 px-6",
    )}
    data-testid="ai-discover-results"
  >
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
      {#if grouped && makers.length > 1}
        <div
          class={cn(
            "mb-4 flex items-center gap-1.5",
            compact ? "overflow-x-auto pb-1 [&>button]:shrink-0" : "flex-wrap",
          )}
          role="group"
          aria-label={$t("ai.discover.makers_label")}
          data-testid="ai-discover-makers"
        >
          <button
            type="button"
            class={cn(
              "rounded-full border px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
              kept === null
                ? "border-primary/40 bg-primary/5 font-medium text-foreground"
                : "border-border text-muted-foreground hover:bg-muted/40 hover:text-foreground",
            )}
            aria-pressed={kept === null}
            onclick={() => (onlyMaker = null)}
            data-testid="ai-discover-maker-all"
          >
            {$t("ai.discover.makers_all")} · {entries.length}
          </button>
          {#each makers as group (group.maker)}
            <button
              type="button"
              class={cn(
                "flex items-center gap-1.5 rounded-full border py-0.5 pl-0.5 pr-2.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                kept === group.maker
                  ? "border-primary/40 bg-primary/5 font-medium text-foreground"
                  : "border-border text-muted-foreground hover:bg-muted/40 hover:text-foreground",
              )}
              aria-pressed={kept === group.maker}
              onclick={() => (onlyMaker = kept === group.maker ? null : group.maker)}
              data-testid="ai-discover-maker-pick"
              data-maker={group.maker}
            >
              <PublisherMark name={group.maker} size="sm" />
              {group.maker === "" ? $t("ai.discover.maker_unknown") : group.maker}
              <span class="tabular-nums">{group.entries.length}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#each groups as group (group.maker)}
        {#if grouped}
          <button
            type="button"
            class="mb-2 flex w-full items-center gap-2 rounded-md px-1 py-1 text-left hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            aria-expanded={!folded.has(group.maker)}
            onclick={() => toggle(group.maker)}
            data-testid="ai-discover-maker"
            data-maker={group.maker}
          >
            <PublisherMark name={group.maker} />
            <span class="min-w-0 truncate text-sm font-semibold text-foreground">
              {group.maker === "" ? $t("ai.discover.maker_unknown") : group.maker}
            </span>
            <span class="text-xs tabular-nums text-muted-foreground">{group.entries.length}</span>
            {#if folded.has(group.maker)}
              <ChevronRight size={14} class="ml-auto shrink-0 text-muted-foreground" aria-hidden="true" />
            {:else}
              <ChevronDown size={14} class="ml-auto shrink-0 text-muted-foreground" aria-hidden="true" />
            {/if}
          </button>
        {/if}
        {#if !grouped || !folded.has(group.maker)}
        <ul class="mb-4 divide-y divide-border/60 overflow-hidden rounded-xl border border-border bg-card shadow-sm">
          {#each group.entries as entry (entry.key)}
            {@const kind = publisherKindKeys(entry.publisherKind)}
            <li class="min-w-0">
              <!-- The name is the keyboard path; the whole row answers a click. -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class={cn(
                  "flex cursor-pointer items-center transition-colors",
                  compact ? "gap-2 px-3 py-2" : "gap-4 px-4 py-2.5",
                  selectedKey === entry.key ? "bg-primary/5" : "hover:bg-muted/40",
                )}
                onclick={() => onselect(entry)}
                data-testid="ai-discover-result"
                data-entry-key={entry.key}
              >
                <div class={cn("flex items-center gap-1.5", compact ? "min-w-0 flex-1" : "w-56 shrink-0")}>
                  <button
                    type="button"
                    class="min-w-0 truncate rounded-sm text-left text-sm font-semibold text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                    aria-current={selectedKey === entry.key ? "true" : undefined}
                    onclick={(event) => {
                      event.stopPropagation();
                      onselect(entry);
                    }}
                    data-testid="ai-discover-result-open"
                  >
                    {entry.name}
                  </button>
                  <span class="flex shrink-0 items-center gap-1">
                    {#each capabilitiesOf(entry.capabilities) as capability (capability)}
                      <CapabilityIcon {capability} />
                    {/each}
                  </span>
                </div>
                <div class={cn("min-w-0 flex-1", compact && "hidden")}>
                  <p class="truncate text-sm text-foreground/80">{entry.description}</p>
                  <div class="mt-0.5 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                    {#if !grouped || entry.publisher !== group.maker}
                      <span data-testid="ai-discover-result-publisher">{entry.publisher}</span>
                    {/if}
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
                <div class={cn("shrink-0 text-right", compact ? "w-auto" : "w-40")}>
                  {#if entry.minMemoryGb !== null}
                    <p class="text-sm tabular-nums text-foreground" data-testid="ai-discover-result-memory">
                      {$t("ai.discover.min_memory", { values: { size: entry.minMemoryGb } })}
                    </p>
                    {#if capacityGb !== null && !compact}
                      <p class="text-xs {fitsHere(entry) ? 'text-success' : 'text-muted-foreground'}">
                        {$t(fitsHere(entry) ? "ai.discover.fit_yes" : "ai.discover.fit_no")}
                      </p>
                    {/if}
                  {/if}
                </div>
                {#if !compact}
                  <ChevronRight size={15} class="shrink-0 text-muted-foreground" aria-hidden="true" />
                {/if}
              </div>
            </li>
          {/each}
        </ul>
        {/if}
      {/each}
    {/if}
  </div>
  {@render card?.()}
  </div>
</div>
