<script module lang="ts">
  /** Three-state filter for the installed status of catalogue servers. */
  export type InstallFilter = "all" | "installed" | "not_installed";
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { cn } from "$lib/utils";
  import Button from "$lib/components/ui/button/Button.svelte";
  import CatalogueTrustFilter, {
    type TrustFilterOption,
  } from "./CatalogueTrustFilter.svelte";

  interface Props {
    /** Currently selected trust-level chips (empty = no filter). */
    trustSelected: TrustFilterOption[];
    /** Currently active install-status segment. */
    installFilter: InstallFilter;
    /** Number of servers after all active filters are applied. */
    resultCount: number;
    /** True when at least one filter (trust or install) is active. */
    hasActiveFilters: boolean;
    ontrustchange: (selected: TrustFilterOption[]) => void;
    oninstallchange: (filter: InstallFilter) => void;
    onclearall: () => void;
  }

  let {
    trustSelected,
    installFilter,
    resultCount,
    hasActiveFilters,
    ontrustchange,
    oninstallchange,
    onclearall,
  }: Props = $props();

  const SEGMENTS: { value: InstallFilter; labelKey: string }[] = [
    { value: "all", labelKey: "integrations.catalogue.filter_all" },
    { value: "installed", labelKey: "integrations.catalogue.filter_installed" },
    { value: "not_installed", labelKey: "integrations.catalogue.filter_not_installed" },
  ];

  const resultLabel = $derived(
    resultCount === 1
      ? $t("integrations.catalogue.result_count_one")
      : $t("integrations.catalogue.result_count", { values: { count: resultCount } }),
  );
</script>

<div class="flex flex-wrap items-center gap-x-4 gap-y-2" data-testid="catalogue-filter-bar">
  <!-- Trust-level chips (delegated to CatalogueTrustFilter) -->
  <CatalogueTrustFilter selected={trustSelected} onchange={ontrustchange} />

  <!-- Install status segmented control -->
  <div
    role="group"
    aria-label={$t("integrations.catalogue.filter_installed")}
    class="flex items-center rounded-md border border-border/50 bg-muted/50 p-0.5"
    data-testid="catalogue-install-filter"
  >
    {#each SEGMENTS as seg (seg.value)}
      <button
        type="button"
        role="radio"
        aria-checked={installFilter === seg.value}
        class={cn(
          "rounded px-2.5 py-1 text-xs font-medium transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-1",
          installFilter === seg.value
            ? "bg-background text-foreground shadow-sm"
            : "text-muted-foreground hover:text-foreground",
        )}
        onclick={() => oninstallchange(seg.value)}
        data-testid={`install-filter-${seg.value}`}
      >
        {$t(seg.labelKey)}
      </button>
    {/each}
  </div>

  <!-- Result count + Clear all -->
  <div class="ml-auto flex items-center gap-3">
    <span class="text-xs text-muted-foreground" data-testid="catalogue-result-count">
      {resultLabel}
    </span>

    {#if hasActiveFilters}
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-2 text-xs"
        onclick={onclearall}
        data-testid="catalogue-clear-all"
      >
        {$t("integrations.catalogue.filter_clear_all")}
      </Button>
    {/if}
  </div>
</div>
