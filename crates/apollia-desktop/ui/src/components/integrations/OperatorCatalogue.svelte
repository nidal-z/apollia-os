<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { RegistryServerView } from "$lib/types";
  import CatalogueSearchBar from "./CatalogueSearchBar.svelte";
  import CatalogueCategoryTabs from "./CatalogueCategoryTabs.svelte";
  import CatalogueCard from "./CatalogueCard.svelte";

  interface Props {
    onSelectServer: (server: RegistryServerView) => void;
  }

  let { onSelectServer }: Props = $props();

  let servers = $state<RegistryServerView[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let searchQuery = $state("");
  let selectedCategory = $state("all");

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
      <CatalogueSearchBar
        value={searchQuery}
        onchange={(v) => { searchQuery = v; }}
      />

      {#if availableCategories.length > 1}
        <CatalogueCategoryTabs
          categories={availableCategories}
          selected={selectedCategory}
          onchange={(cat) => { selectedCategory = cat; }}
        />
      {/if}
    </div>

    {#if filteredServers.length === 0}
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
        {#each filteredServers as server (server.name)}
          <CatalogueCard
            {server}
            onclick={() => onSelectServer(server)}
          />
        {/each}
      </div>
    {/if}
  {/if}
</div>
