<script lang="ts">
  /**
   * ModelFilterBar - search box, token toggle, and the filter chip row.
   *
   * Sort and language are server-side (they re-run the query via `onSearch`);
   * license / type / gated / incompatible are client-side and only re-derive
   * the filtered list in the parent.
   */
  import { t } from "svelte-i18n";
  import { Search, Key, SlidersHorizontal, Eye, EyeOff } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";
  import type { LicenseFilter, ModelTypeFilter, GatedFilter } from "./helpers";

  interface Props {
    searchQuery: string;
    sortBy: string;
    langFilter: string;
    licenseFilter: LicenseFilter;
    modelTypeFilter: ModelTypeFilter;
    gatedFilter: GatedFilter;
    showIncompatible: boolean;
    searching: boolean;
    tokenSet: boolean;
    filteredCount: number;
    totalCount: number;
    onSearch: () => void;
    onToggleToken: () => void;
  }

  let {
    searchQuery = $bindable(),
    sortBy = $bindable(),
    langFilter = $bindable(),
    licenseFilter = $bindable(),
    modelTypeFilter = $bindable(),
    gatedFilter = $bindable(),
    showIncompatible = $bindable(),
    searching,
    tokenSet,
    filteredCount,
    totalCount,
    onSearch,
    onToggleToken,
  }: Props = $props();

  const filterSelect =
    "h-7 rounded-md border border-border bg-background px-2 text-caption text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40";
</script>

<div class="flex flex-col gap-3">
  <div class="flex gap-2">
    <Input
      id="model-hub-search-input"
      icon={Search}
      size="sm"
      bind:value={searchQuery}
      onkeydown={(e) => e.key === "Enter" && onSearch()}
      placeholder={$t("settings.model_hub.search.placeholder")}
      aria-label={$t("settings.model_hub.search.placeholder")}
      class="flex-1"
      data-testid="model-hub-search-input"
    />
    <Button
      variant="default"
      size="sm"
      loading={searching}
      onclick={onSearch}
      data-testid="model-hub-search-btn"
    >
      {#snippet icon()}<Search size={14} />{/snippet}
      {$t("settings.model_hub.search.button")}
    </Button>
    <Button
      variant="outline"
      size="sm"
      onclick={onToggleToken}
      title={$t("settings.model_hub.token.title_button")}
      data-testid="hf-token-toggle"
    >
      {#snippet icon()}<Key size={14} />{/snippet}
      {tokenSet ? $t("settings.model_hub.token.set") : $t("settings.model_hub.token.btn")}
    </Button>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <SlidersHorizontal size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />

    <Select bind:value={sortBy} onchange={onSearch} class={filterSelect} data-testid="model-filter-sort" aria-label={$t("settings.model_hub.filters.sort_downloads")}>
      <option value="downloads">{$t("settings.model_hub.filters.sort_downloads")}</option>
      <option value="likes">{$t("settings.model_hub.filters.sort_likes")}</option>
      <option value="trending">{$t("settings.model_hub.filters.sort_trending")}</option>
      <option value="createdAt">{$t("settings.model_hub.filters.sort_newest")}</option>
    </Select>

    <Select bind:value={langFilter} onchange={onSearch} class={filterSelect} data-testid="model-filter-language" aria-label={$t("settings.model_hub.filters.lang_any")}>
      <option value="">{$t("settings.model_hub.filters.lang_any")}</option>
      <option value="en">{$t("settings.model_hub.filters.lang_en")}</option>
      <option value="fr">{$t("settings.model_hub.filters.lang_fr")}</option>
      <option value="zh">{$t("settings.model_hub.filters.lang_zh")}</option>
      <option value="de">{$t("settings.model_hub.filters.lang_de")}</option>
      <option value="es">{$t("settings.model_hub.filters.lang_es")}</option>
      <option value="ja">{$t("settings.model_hub.filters.lang_ja")}</option>
      <option value="ko">{$t("settings.model_hub.filters.lang_ko")}</option>
      <option value="ar">{$t("settings.model_hub.filters.lang_ar")}</option>
      <option value="ru">{$t("settings.model_hub.filters.lang_ru")}</option>
      <option value="pt">{$t("settings.model_hub.filters.lang_pt")}</option>
    </Select>

    <Select bind:value={licenseFilter} class={filterSelect} data-testid="model-filter-license" aria-label={$t("settings.model_hub.filters.license_any")}>
      <option value="any">{$t("settings.model_hub.filters.license_any")}</option>
      <option value="open">{$t("settings.model_hub.filters.license_open")}</option>
      <option value="restricted">{$t("settings.model_hub.filters.license_restricted")}</option>
    </Select>

    <Select bind:value={modelTypeFilter} class={filterSelect} data-testid="model-filter-type" aria-label={$t("settings.model_hub.filters.type_any")}>
      <option value="any">{$t("settings.model_hub.filters.type_any")}</option>
      <option value="instruct">{$t("settings.model_hub.filters.type_instruct")}</option>
      <option value="base">{$t("settings.model_hub.filters.type_base")}</option>
      <option value="reasoning">{$t("settings.model_hub.filters.type_reasoning")}</option>
    </Select>

    <Select bind:value={gatedFilter} class={filterSelect} data-testid="model-filter-gated" aria-label={$t("settings.model_hub.filters.gated_any")}>
      <option value="any">{$t("settings.model_hub.filters.gated_any")}</option>
      <option value="open">{$t("settings.model_hub.filters.gated_open_only")}</option>
      <option value="gated">{$t("settings.model_hub.filters.gated_only")}</option>
    </Select>

    <Button
      variant={showIncompatible ? "soft" : "outline"}
      size="sm"
      class="h-7 px-2 text-caption {showIncompatible ? 'text-warning-a11y' : 'text-muted-foreground'}"
      onclick={() => (showIncompatible = !showIncompatible)}
      title={showIncompatible ? $t("settings.model_hub.filters.hide_incompatible_title") : $t("settings.model_hub.filters.show_incompatible_title")}
      data-testid="model-filter-incompatible"
    >
      {#snippet icon()}
        {#if showIncompatible}<Eye size={12} />{:else}<EyeOff size={12} />{/if}
      {/snippet}
      {$t("settings.model_hub.filters.incompatible")}
    </Button>

    {#if totalCount > 0}
      <span class="ml-auto text-caption tabular-nums text-muted-foreground">
        {$t("settings.model_hub.filters.result_count", { values: { filtered: filteredCount, total: totalCount } })}
      </span>
    {/if}
  </div>
</div>
