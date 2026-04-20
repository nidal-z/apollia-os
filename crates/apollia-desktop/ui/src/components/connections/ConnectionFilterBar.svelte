<script lang="ts" module>
  export type StatusFilter = "all" | "installed" | "not_installed";
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { Search, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    query: string;
    category: string;
    availableCategories: string[];
    status: StatusFilter;
    hasActive: boolean;
    onquery: (v: string) => void;
    oncategory: (v: string) => void;
    onstatus: (v: StatusFilter) => void;
    onclearall: () => void;
  }

  let {
    query,
    category,
    availableCategories,
    status,
    hasActive,
    onquery,
    oncategory,
    onstatus,
    onclearall,
  }: Props = $props();

  const STATUSES: StatusFilter[] = ["all", "installed", "not_installed"];

  function categoryLabel(cat: string): string {
    const key = `integrations.categories.${cat}`;
    const translated = $t(key);
    return translated === key ? cat : translated;
  }
</script>

<div class="flex flex-col gap-2" data-testid="connections-filter-bar">
  <div class="flex flex-col gap-2 md:flex-row md:items-center">
    <div class="relative flex-1">
      <Search
        size={14}
        class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
      />
      <input
        type="search"
        class="h-9 w-full rounded-md border border-border bg-background pl-9 pr-8 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
        placeholder={$t("connections.filters.search_placeholder")}
        value={query}
        oninput={(e) => onquery(e.currentTarget.value)}
        data-testid="connections-search"
        aria-label={$t("connections.filters.search_label")}
      />
      {#if query.length > 0}
        <button
          type="button"
          class="absolute right-2 top-1/2 -translate-y-1/2 rounded-sm p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          onclick={() => onquery("")}
          aria-label={$t("connections.filters.clear_search")}
          data-testid="connections-search-clear"
        >
          <X size={12} />
        </button>
      {/if}
    </div>

    <label class="flex items-center gap-2 text-xs text-muted-foreground">
      <span class="sr-only">{$t("connections.filters.category_label")}</span>
      <select
        class="h-9 rounded-md border border-border bg-background px-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
        value={category}
        onchange={(e) => oncategory(e.currentTarget.value)}
        data-testid="connections-category"
        aria-label={$t("connections.filters.category_label")}
      >
        {#each availableCategories as cat}
          <option value={cat}>{categoryLabel(cat)}</option>
        {/each}
      </select>
    </label>

    <div
      class="flex items-center gap-1 rounded-md border border-border bg-background p-0.5"
      role="radiogroup"
      aria-label={$t("connections.filters.status_label")}
    >
      {#each STATUSES as s}
        {@const isActive = status === s}
        <button
          type="button"
          role="radio"
          aria-checked={isActive}
          class="rounded-sm px-2.5 py-1 text-xs font-medium transition-colors {isActive
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:text-foreground'}"
          data-testid="connections-status-{s}"
          onclick={() => onstatus(s)}
        >
          {$t(`connections.filters.status.${s}`)}
        </button>
      {/each}
    </div>
  </div>

  {#if hasActive}
    <div class="flex flex-wrap items-center gap-2" data-testid="connections-active-chips">
      {#if query.length > 0}
        <span
          class="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground"
        >
          {$t("connections.filters.chip_query", { values: { q: query } })}
          <button
            type="button"
            class="rounded-full hover:text-foreground"
            onclick={() => onquery("")}
            aria-label={$t("connections.filters.clear_search")}
          >
            <X size={10} />
          </button>
        </span>
      {/if}
      {#if category !== "all"}
        <span
          class="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground"
        >
          {categoryLabel(category)}
          <button
            type="button"
            class="rounded-full hover:text-foreground"
            onclick={() => oncategory("all")}
            aria-label={$t("connections.filters.clear_category")}
          >
            <X size={10} />
          </button>
        </span>
      {/if}
      {#if status !== "all"}
        <span
          class="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground"
        >
          {$t(`connections.filters.status.${status}`)}
          <button
            type="button"
            class="rounded-full hover:text-foreground"
            onclick={() => onstatus("all")}
            aria-label={$t("connections.filters.clear_status")}
          >
            <X size={10} />
          </button>
        </span>
      {/if}
      <Button
        size="sm"
        variant="ghost"
        onclick={onclearall}
        data-testid="connections-clear-all"
        class="h-6 text-[11px]"
      >
        {$t("connections.filters.clear_all")}
      </Button>
    </div>
  {/if}
</div>
