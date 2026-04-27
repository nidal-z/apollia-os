<script lang="ts">
  /**
   * Filter controls above the template grid.
   *
   * Category / difficulty / source chips toggle individually; search is
   * full-text across title + description. All filters are two-way bound
   * so the parent owns the URL persistence.
   */
  import { t } from "svelte-i18n";
  import { Search, X } from "lucide-svelte";
  import {
    CATEGORIES,
    DIFFICULTIES,
    SOURCES,
    type TemplateCategory,
    type TemplateDifficulty,
    type TemplateSource,
  } from "$lib/templates/registry";

  interface Props {
    search: string;
    categories: Set<TemplateCategory>;
    difficulties: Set<TemplateDifficulty>;
    sources: Set<TemplateSource>;
    showCommunity: boolean;
    onchange: () => void;
    onreset: () => void;
  }

  let {
    search = $bindable(""),
    categories = $bindable(new Set()),
    difficulties = $bindable(new Set()),
    sources = $bindable(new Set()),
    showCommunity,
    onchange,
    onreset,
  }: Props = $props();

  function toggle<T>(set: Set<T>, value: T): Set<T> {
    const next = new Set(set);
    if (next.has(value)) next.delete(value);
    else next.add(value);
    return next;
  }

  function toggleCategory(c: TemplateCategory) {
    categories = toggle(categories, c);
    onchange();
  }
  function toggleDifficulty(d: TemplateDifficulty) {
    difficulties = toggle(difficulties, d);
    onchange();
  }
  function toggleSource(s: TemplateSource) {
    sources = toggle(sources, s);
    onchange();
  }

  const hasAnyFilter = $derived(
    search.trim().length > 0 ||
      categories.size > 0 ||
      difficulties.size > 0 ||
      sources.size > 0,
  );

  // Source filter only shows "community" chip if the user opted in.
  const visibleSources = $derived(
    showCommunity ? SOURCES : SOURCES.filter((s) => s !== "community"),
  );
</script>

<div class="space-y-3" data-testid="template-filters">
  <div class="relative">
    <Search
      size={14}
      strokeWidth={1.75}
      class="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
    />
    <input
      type="search"
      bind:value={search}
      oninput={() => onchange()}
      placeholder={$t("templates.filters.search_placeholder")}
      class="h-9 w-full rounded-md border border-border bg-background pl-8 pr-3 text-sm placeholder:text-muted-foreground focus:border-primary focus:outline-none"
      data-testid="template-filters-search"
    />
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <span class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground/70"
      >{$t("templates.filters.category_label")}</span
    >
    {#each CATEGORIES as c}
      {@const active = categories.has(c)}
      <button
        type="button"
        class="rounded-full border px-2.5 py-0.5 text-[11px] transition-colors {active
          ? 'border-primary bg-primary/10 text-primary'
          : 'border-border text-muted-foreground hover:border-muted-foreground'}"
        onclick={() => toggleCategory(c)}
        data-testid="template-filter-category-{c}"
      >
        {$t(`templates.category.${c}`)}
      </button>
    {/each}
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <span class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground/70"
      >{$t("templates.filters.difficulty_label")}</span
    >
    {#each DIFFICULTIES as d}
      {@const active = difficulties.has(d)}
      <button
        type="button"
        class="rounded-full border px-2.5 py-0.5 text-[11px] transition-colors {active
          ? 'border-primary bg-primary/10 text-primary'
          : 'border-border text-muted-foreground hover:border-muted-foreground'}"
        onclick={() => toggleDifficulty(d)}
        data-testid="template-filter-difficulty-{d}"
      >
        {$t(`templates.difficulty.${d}`)}
      </button>
    {/each}
  </div>

  {#if visibleSources.length > 1}
    <div class="flex flex-wrap items-center gap-2">
      <span class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground/70"
        >{$t("templates.filters.source_label")}</span
      >
      {#each visibleSources as s}
        {@const active = sources.has(s)}
        <button
          type="button"
          class="rounded-full border px-2.5 py-0.5 text-[11px] transition-colors {active
            ? 'border-primary bg-primary/10 text-primary'
            : 'border-border text-muted-foreground hover:border-muted-foreground'}"
          onclick={() => toggleSource(s)}
          data-testid="template-filter-source-{s}"
        >
          {s === "official"
            ? $t("templates.source.official")
            : $t("templates.source.community")}
        </button>
      {/each}
    </div>
  {/if}

  {#if hasAnyFilter}
    <button
      type="button"
      onclick={onreset}
      class="inline-flex items-center gap-1 text-[11px] text-muted-foreground underline decoration-dotted underline-offset-2 hover:text-foreground"
      data-testid="template-filters-reset"
    >
      <X size={11} strokeWidth={1.75} />
      {$t("templates.filters.reset")}
    </button>
  {/if}
</div>
