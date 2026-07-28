<script lang="ts">
  /**
   * ModelResults - the search-results region: the humanized error banner, the
   * keyboard-navigable list of result cards (cascade in, FLIP reorder), the
   * "load more" pager, and the idle / no-match / no-results empty states.
   */
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { Search, EyeOff } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import { listFlip, rowIn } from "$lib/design/listMotion";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import type { HumanizedError } from "$lib/errors/humanize";
  import type { DownloadProgress, HfFile, HfModelCard } from "$lib/ipc/modelHub";
  import type { GgufGroup } from "./helpers";
  import ModelResultCard from "./ModelResultCard.svelte";

  interface Props {
    results: HfModelCard[];
    totalCount: number;
    searching: boolean;
    hasSearched: boolean;
    searchQuery: string;
    searchError: HumanizedError | null;
    hasMore: boolean;
    expandedModel: string | null;
    detail: HfModelCard | null;
    detailLoading: boolean;
    downloads: DownloadProgress[];
    onToggle: (repoId: string) => void;
    onDownloadGroup: (group: GgufGroup) => void;
    onDownloadFile: (file: HfFile) => void;
    onLoadMore: () => void;
    onRetry: () => void;
    onClearFilters: () => void;
  }

  let {
    results,
    totalCount,
    searching,
    hasSearched,
    searchQuery,
    searchError,
    hasMore,
    expandedModel,
    detail,
    detailLoading,
    downloads,
    onToggle,
    onDownloadGroup,
    onDownloadFile,
    onLoadMore,
    onRetry,
    onClearFilters,
  }: Props = $props();

  const emptyClass =
    "flex flex-col items-center gap-3 rounded-xl border border-border bg-muted/30 px-6 py-10 text-center";
</script>

{#if searchError}
  <ErrorBanner
    message={searchError.friendly_message}
    onretry={onRetry}
    retryLabel={$t("settings.model_hub.results.retry")}
    loading={searching}
    data-testid="model-search-error"
  />
{/if}

{#if results.length > 0}
  <div class="flex flex-col gap-2.5" use:listNavigation={{ rowSelector: "button[aria-expanded]" }} data-testid="model-results">
    {#each results as model (model.repo_id)}
      <div animate:flip={listFlip()} in:fly={rowIn()}>
        <ModelResultCard
          {model}
          expanded={expandedModel === model.repo_id}
          {detail}
          {detailLoading}
          {downloads}
          {onToggle}
          {onDownloadGroup}
          {onDownloadFile}
        />
      </div>
    {/each}

    {#if hasMore}
      <div class="flex justify-center pt-2">
        <Button variant="outline" size="sm" loading={searching} onclick={onLoadMore} data-testid="model-load-more">
          {searching ? $t("settings.model_hub.results.loading") : $t("settings.model_hub.results.load_more")}
        </Button>
      </div>
    {/if}
  </div>
{:else if !searching && totalCount > 0}
  <div class={emptyClass} data-testid="model-results-no-match">
    <EyeOff size={22} class="text-muted-foreground" aria-hidden="true" />
    <div>
      <div class="text-body-sm font-medium text-foreground">{$t("settings.model_hub.results.no_match")}</div>
      <div class="mt-1 text-caption text-muted-foreground">{$t("settings.model_hub.results.no_match_desc", { values: { total: totalCount } })}</div>
    </div>
    <Button variant="ghost" size="sm" onclick={onClearFilters} data-testid="model-results-clear-filters">
      {$t("settings.model_hub.results.clear_filters")}
    </Button>
  </div>
{:else if !searching && hasSearched && searchQuery}
  <div class={emptyClass} data-testid="model-results-empty">
    <Search size={22} class="text-muted-foreground" aria-hidden="true" />
    <div>
      <div class="text-body-sm font-medium text-foreground">{$t("settings.model_hub.results.no_results", { values: { query: searchQuery } })}</div>
      <div class="mt-1 text-caption text-muted-foreground">{$t("settings.model_hub.results.no_results_desc")}</div>
    </div>
  </div>
{:else if !searching && !hasSearched}
  <div class={emptyClass} data-testid="model-results-idle">
    <Search size={22} class="text-muted-foreground" aria-hidden="true" />
    <div>
      <div class="text-body-sm font-medium text-foreground">{$t("settings.model_hub.results.idle_title")}</div>
      <div class="mt-1 max-w-md text-caption text-muted-foreground">{$t("settings.model_hub.results.idle_desc")}</div>
    </div>
  </div>
{/if}
