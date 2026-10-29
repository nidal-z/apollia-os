<script lang="ts">
  /**
   * AddBrowseBar - what spans the add window above its list: the usages as
   * chips, then the search, the source and the order, then "no setup". Each
   * choice is held by the caller, which keeps it while a dialog of the flow
   * stands in the window's place.
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
    /** The public catalogue is being read. */
    loading: boolean;
    /** Another source was chosen: the caller reads the public catalogue when it is included. */
    onsource: (source: BrowseSource) => void;
  }

  let { query = $bindable(""), browse = $bindable(), loading, onsource }: Props = $props();

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

<!-- A narrow window keeps the usages on one line that scrolls sideways, and
     leaves its height to the list; a focused chip is scrolled into view, so
     the keyboard reaches every one. From md up they wrap. -->
<div
  class="-mx-0.5 flex flex-nowrap items-center gap-1.5 overflow-x-auto px-0.5 py-0.5 md:flex-wrap md:overflow-visible [&>button]:shrink-0 [&>button]:whitespace-nowrap"
  role="group"
  aria-label={$t("connections.add_window.browse.usages_label")}
  data-testid="catalogue-usages"
>
  {#each CHIPS as usage (usage)}
    <button
      type="button"
      class={cn(
        "rounded-full border px-3 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        browse.usage === usage
          ? "border-primary/40 bg-primary/5 font-medium text-foreground"
          : "border-border text-muted-foreground hover:bg-muted/60 hover:text-foreground",
      )}
      aria-pressed={browse.usage === usage}
      onclick={() => pickUsage(usage)}
      data-testid="catalogue-usage-chip-{usage}"
    >
      {$t(chipKey(usage))}
    </button>
  {/each}
</div>

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
</div>

<div class="flex flex-wrap items-center gap-2">
  <Checkbox
    checked={browse.keyless}
    onchange={(keyless) => (browse = { ...browse, keyless })}
    id="catalogue-keyless"
    data-testid="catalogue-keyless"
  />
  <label for="catalogue-keyless" class="text-xs text-foreground">
    {$t("connections.add_window.browse.keyless")}
    <span class="text-muted-foreground">· {$t("connections.add_window.browse.keyless_hint")}</span>
  </label>
  {#if loading}
    <span class="ml-auto flex items-center gap-1.5 text-xs text-muted-foreground" data-testid="catalogue-loading">
      <Spinner size={11} />
      {$developerMode ? $t("dev.connections.catalogue_loading") : $t("connections.catalogue_loading")}
    </span>
  {/if}
</div>
