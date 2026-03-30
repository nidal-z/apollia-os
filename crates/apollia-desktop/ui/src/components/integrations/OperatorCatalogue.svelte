<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { RegistryServerView, TrustLevel } from "$lib/types";
  import CatalogueSearchBar from "./CatalogueSearchBar.svelte";
  import CatalogueCategoryTabs from "./CatalogueCategoryTabs.svelte";
  import CatalogueCard from "./CatalogueCard.svelte";
  import CatalogueSortDropdown, { type SortOption } from "./CatalogueSortDropdown.svelte";
  import CatalogueFilterBar, { type InstallFilter } from "./CatalogueFilterBar.svelte";
  import type { TrustFilterOption } from "./CatalogueTrustFilter.svelte";

  interface Props {
    onSelectServer: (server: RegistryServerView) => void;
  }

  let { onSelectServer }: Props = $props();

  let servers = $state<RegistryServerView[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let searchQuery = $state("");
  let selectedCategory = $state("all");
  let sortBy = $state<SortOption>("trust");
  let selectedTrust = $state<TrustFilterOption[]>([]);
  let installFilter = $state<InstallFilter>("all");

  const TRUST_ORDER: Record<TrustLevel, number> = {
    verified_official: 0,
    community_verified: 1,
    community: 2,
    custom: 3,
  };

  function trustPriority(server: RegistryServerView): number {
    return TRUST_ORDER[server.trust_level];
  }

  /** Ordered list of categories derived from server enrichments, with "all" first. */
  const availableCategories = $derived.by(() => {
    const seen = new Set<string>();
    for (const s of servers) {
      if (s.enrichment) seen.add(s.enrichment.category);
    }
    return ["all", ...Array.from(seen).sort()];
  });

  /** Servers after applying search and category filters. */
  const filteredServers = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    return servers.filter((s) => {
      if (selectedCategory !== "all") {
        if (!s.enrichment || s.enrichment.category !== selectedCategory) return false;
      }
      if (query.length === 0) return true;
      const inName = s.name.toLowerCase().includes(query);
      const inTitle = (s.title ?? "").toLowerCase().includes(query);
      const inDescription = (s.description ?? "").toLowerCase().includes(query);
      return inName || inTitle || inDescription;
    });
  });

  /** Servers after applying the trust-level filter (no selection = show all). */
  const filteredByTrust = $derived.by(() => {
    if (selectedTrust.length === 0) return filteredServers;
    return filteredServers.filter((s) =>
      selectedTrust.includes(s.trust_level as TrustFilterOption),
    );
  });

  /** Servers after applying the install-status filter. */
  const filteredByInstall = $derived.by(() => {
    if (installFilter === "all") return filteredByTrust;
    if (installFilter === "installed") return filteredByTrust.filter((s) => s.is_installed);
    return filteredByTrust.filter((s) => !s.is_installed);
  });

  /** True when at least one non-default filter is active. */
  const hasActiveFilters = $derived(
    selectedTrust.length > 0 || installFilter !== "all" || searchQuery.trim().length > 0,
  );

  /** Filtered servers sorted by the active sort option. */
  const sortedServers = $derived.by(() => {
    const list = [...filteredByInstall];
    switch (sortBy) {
      case "name_asc":
        return list.sort((a, b) =>
          a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
        );
      case "name_desc":
        return list.sort((a, b) =>
          b.name.toLowerCase().localeCompare(a.name.toLowerCase()),
        );
      case "installed_first":
        return list.sort((a, b) => {
          const installedDiff = (a.is_installed ? 0 : 1) - (b.is_installed ? 0 : 1);
          return installedDiff !== 0 ? installedDiff : trustPriority(a) - trustPriority(b);
        });
      case "trust":
        return list.sort((a, b) => trustPriority(a) - trustPriority(b));
    }
  });

  async function loadRegistry(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      servers = await invoke<RegistryServerView[]>("fetch_mcp_registry");
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    loadRegistry();
  });
</script>

<div class="flex flex-col gap-4" data-testid="operator-catalogue">
  <div>
    <h2 class="text-lg font-semibold text-foreground">
      {$t("integrations.catalogue.title")}
    </h2>
    <p class="mt-0.5 text-sm text-muted-foreground">
      {$t("integrations.catalogue.subtitle")}
    </p>
  </div>

  {#if loading}
    <p class="text-sm text-muted-foreground" data-testid="catalogue-loading">
      {$t("common.loading")}
    </p>
  {:else if loadError}
    <p class="text-sm text-destructive" data-testid="catalogue-error">{loadError}</p>
  {:else}
    <div class="flex flex-col gap-3">
      <div class="flex items-center gap-2">
        <div class="flex-1">
          <CatalogueSearchBar
            value={searchQuery}
            onchange={(v) => { searchQuery = v; }}
          />
        </div>
        <CatalogueSortDropdown
          value={sortBy}
          onchange={(v) => { sortBy = v; }}
        />
      </div>

      {#if availableCategories.length > 1}
        <CatalogueCategoryTabs
          categories={availableCategories}
          selected={selectedCategory}
          onchange={(cat) => { selectedCategory = cat; }}
        />
      {/if}

      <CatalogueFilterBar
        trustSelected={selectedTrust}
        {installFilter}
        resultCount={sortedServers.length}
        {hasActiveFilters}
        ontrustchange={(v) => { selectedTrust = v; }}
        oninstallchange={(v) => { installFilter = v; }}
        onclearall={() => {
          selectedTrust = [];
          installFilter = "all";
          searchQuery = "";
        }}
      />
    </div>

    {#if sortedServers.length === 0}
      <p
        class="text-sm text-muted-foreground py-6 text-center"
        data-testid="catalogue-no-results"
      >
        {$t("integrations.catalogue.no_results")}
      </p>
    {:else}
      <div
        class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3"
        data-testid="catalogue-grid"
      >
        {#each sortedServers as server (server.name)}
          <CatalogueCard
            {server}
            {searchQuery}
            onclick={() => onSelectServer(server)}
          />
        {/each}
      </div>
    {/if}
  {/if}
</div>
