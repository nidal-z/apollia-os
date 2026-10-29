<script lang="ts">
  /**
   * AddBrowseBar - what spans the add window above its list, laid like the
   * Connections page: the search, the source, the order and "no setup" (its
   * meaning in its tooltip) on one line, then the usages as chips, each with
   * the number of entries it would show. Each choice is held by the caller,
   * which keeps it while a dialog of the flow stands in the window's place.
   */
  import { t } from "svelte-i18n";
  import { Search } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { developerMode } from "$lib/stores/developerMode";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Spinner } from "$lib/components/ui/progress";
  import { USAGES, usageKey } from "$lib/connections/usages";
  import { effectiveSort, sortsFor } from "$lib/connections/addBrowse";
  import {
    BROWSE_SOURCES,
    type BrowseSort,
    type BrowseSource,
    type BrowseState,
    type UsageChoice,
  } from "$lib/connections/addWindow";

  interface Props {
    query: string;
    browse: BrowseState;
    /** The entries each usage chip would show, and the total for all of them. */
    counts?: Partial<Record<UsageChoice, number>>;
    /** The public catalogue is being read. */
    loading: boolean;
    /** Another source was chosen: the caller reads the public catalogue when it is included. */
    onsource: (source: BrowseSource) => void;
  }

  let { query = $bindable(""), browse = $bindable(), counts = {}, loading, onsource }: Props = $props();

  const CHIPS: readonly UsageChoice[] = ["all", ...USAGES];
  const SOURCE_KEYS: Record<BrowseSource, string> = {
    verified: "connections.add_window.browse.source.verified",
    public: "connections.add_window.browse.source.public",
    all: "connections.add_window.browse.source.all",
  };
  const SORT_KEYS: Record<BrowseSort, string> = {
    recommended: "connections.add_window.browse.sort.recommended",
    name: "connections.add_window.browse.sort.name",
  };
  const sorts = $derived(sortsFor(browse.source));
  const sort = $derived(effectiveSort(browse));

  function chipKey(usage: UsageChoice): string {
    return usage === "all" ? "connections.usage.all" : usageKey(usage);
  }

  function pickUsage(usage: UsageChoice): void {
    browse = { ...browse, usage };
  }

  function pickSort(event: Event): void {
    browse = { ...browse, sort: (event.currentTarget as HTMLSelectElement).value as BrowseSort };
  }
</script>

<div class="flex flex-wrap items-center gap-2.5">
  <div class="min-w-[12rem] flex-1">
    <Input
      icon={Search}
      size="sm"
      type="search"
      bind:value={query}
      placeholder={$t("connections.catalogue_search_placeholder")}
      aria-label={$t("connections.catalogue_search_placeholder")}
      data-testid="catalogue-search"
    />
  </div>
  <!-- The source is a choice one can see and change, as in the discovery of models. -->
  <div
    class="inline-flex items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5"
    role="group"
    aria-label={$t("connections.add_window.browse.source_label")}
  >
    {#each BROWSE_SOURCES as source (source)}
      <button
        type="button"
        class={cn(
          "rounded-md px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
          browse.source === source
            ? "bg-card font-medium text-foreground shadow-sm"
            : "text-muted-foreground hover:text-foreground",
        )}
        aria-pressed={browse.source === source}
        onclick={() => onsource(source)}
        data-testid="catalogue-source-{source}"
      >
        {$t(SOURCE_KEYS[source])}
      </button>
    {/each}
  </div>
  {#if sorts.length > 1}
    <div class="w-36 shrink-0">
      <Select
        size="sm"
        value={sort}
        onchange={pickSort}
        aria-label={$t("connections.add_window.browse.sort_label")}
        data-testid="catalogue-sort"
      >
        {#each sorts as entry (entry)}
          <option value={entry}>{$t(SORT_KEYS[entry])}</option>
        {/each}
      </Select>
    </div>
  {/if}
  <div class="flex items-center gap-2" title={$t("connections.add_window.browse.keyless_hint")}>
    <Checkbox
      checked={browse.keyless}
      onchange={(keyless) => (browse = { ...browse, keyless })}
      id="catalogue-keyless"
      data-testid="catalogue-keyless"
    />
    <label for="catalogue-keyless" class="whitespace-nowrap text-xs text-foreground">
      {$t("connections.add_window.browse.keyless")}
    </label>
  </div>
  {#if loading}
    <span class="flex items-center gap-1.5 text-xs text-muted-foreground" data-testid="catalogue-loading">
      <Spinner size={11} />
      {$developerMode ? $t("dev.connections.catalogue_loading") : $t("connections.catalogue_loading")}
    </span>
  {/if}
</div>

<!-- A narrow window keeps the usages on one line that scrolls sideways, and
     leaves its height to the list; a focused chip is scrolled into view, so
     the keyboard reaches every one; a spacer closes the line, whose end
     padding a scrolling flex row does not count. From md up they wrap. -->
<div
  class="-mx-0.5 flex flex-nowrap items-center gap-1.5 overflow-x-auto px-0.5 py-0.5 after:block after:w-px after:shrink-0 after:content-[''] md:flex-wrap md:overflow-visible md:after:hidden [&>button]:shrink-0 [&>button]:whitespace-nowrap"
  role="group"
  aria-label={$t("connections.add_window.browse.usages_label")}
  data-testid="catalogue-usages"
>
  {#each CHIPS as usage (usage)}
    {@const count = counts[usage]}
    {@const active = browse.usage === usage}
    <button
      type="button"
      class={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-caption-lg font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        active
          ? "border-primary/40 bg-primary/10 text-primary"
          : "border-border text-muted-foreground hover:border-border/80 hover:text-foreground",
        !active && count === 0 && "opacity-50",
      )}
      aria-pressed={active}
      onclick={() => pickUsage(usage)}
      data-testid="catalogue-usage-chip-{usage}"
    >
      {$t(chipKey(usage))}
      {#if count !== undefined}
        <span class={cn("tabular-nums", active ? "text-primary/80" : "text-muted-foreground/60")}>{count}</span>
      {/if}
    </button>
  {/each}
</div>
