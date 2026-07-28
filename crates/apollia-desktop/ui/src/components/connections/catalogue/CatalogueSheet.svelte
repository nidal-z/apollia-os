<script lang="ts">
  /**
   * CatalogueSheet - the "add a connector" Sheet: discover + custom MCP.
   *
   * The public MCP catalogue and the custom-server form share one Sheet (same
   * audience). Discover renders ranked suggestions + the rest of the registry
   * with trust-level filter chips and IntersectionObserver auto-pagination.
   */
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import { EmptyState, FilterChipBar } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { TabBar } from "$lib/components/ui/tabs";
  import { Sheet, SheetHeader, SheetContent } from "$lib/components/ui/sheet";
  import CatalogueEntryRow from "./CatalogueEntryRow.svelte";
  import CustomMcpForm from "./CustomMcpForm.svelte";
  import { rankSuggestions } from "../ConnectionSuggestions.svelte";
  import { installedNameFor, isInstalled, isOfficial } from "$lib/connections/vendor";
  import type { AgentListItem, RegistryServerView } from "$lib/types";

  type CatalogueTab = "discover" | "custom";
  type McpFilter = "all" | "installed" | "official" | "community";
  type CatalogueTier = "none" | "curated" | "full";

  interface Props {
    open: boolean;
    registry: RegistryServerView[];
    agents: AgentListItem[];
    installedNames: Set<string>;
    installedByPackage: Map<string, string>;
    registryLoading: boolean;
    catalogueTier: CatalogueTier;
    onclose: () => void;
    onconnect: (entry: RegistryServerView) => void;
    onmanage: (name: string) => void;
    oninstalled: () => void;
  }

  let {
    open,
    registry,
    agents,
    installedNames,
    installedByPackage,
    registryLoading,
    catalogueTier,
    onclose,
    onconnect,
    onmanage,
    oninstalled,
  }: Props = $props();

  const CATALOG_PAGE = 48;
  let tab = $state<CatalogueTab>("discover");
  let filter = $state<McpFilter>("all");
  let limit = $state(CATALOG_PAGE);

  const entries = $derived.by(() => {
    const ranked = rankSuggestions(registry, agents, 12);
    const seen = new Set(ranked.map((r) => r.name));
    return [...ranked, ...registry.filter((r) => !seen.has(r.name))];
  });

  const officialCount = $derived(entries.filter((e) => isOfficial(e)).length);
  const communityCount = $derived(
    entries.filter((e) => e.trust_level === "community" || e.trust_level === "custom").length,
  );
  const installedCount = $derived(
    entries.filter((e) => isInstalled(e, installedNames, installedByPackage)).length,
  );

  const filtered = $derived(
    entries.filter((e) => {
      if (filter === "installed") return isInstalled(e, installedNames, installedByPackage);
      if (filter === "official") return isOfficial(e);
      if (filter === "community") return e.trust_level === "community" || e.trust_level === "custom";
      return true;
    }),
  );

  // Reset the visible slice when the filter changes.
  $effect(() => {
    void filter;
    limit = CATALOG_PAGE;
  });

  const visible = $derived(filtered.slice(0, limit));
  const hasMore = $derived(filtered.length > limit);

  let sentinel = $state<HTMLDivElement | null>(null);
  $effect(() => {
    const el = sentinel;
    if (!el || !open || tab !== "discover") return;
    const observer = new IntersectionObserver(
      (obs) => {
        if (obs[0]?.isIntersecting && limit < filtered.length) {
          limit = Math.min(limit + CATALOG_PAGE, filtered.length);
        }
      },
      { rootMargin: "200px 0px" },
    );
    observer.observe(el);
    return () => observer.disconnect();
  });

  function activate(entry: RegistryServerView): void {
    const installedName = installedNameFor(entry, installedNames, installedByPackage);
    if (entry.is_installed || installedName !== null) {
      onmanage(installedName ?? entry.name);
    } else {
      onconnect(entry);
    }
  }
</script>

<Sheet {open} {onclose} class="w-full sm:max-w-[920px]">
  <SheetHeader
    title={$t("connections.add_connector")}
    subtitle={$t("connections.catalogue_subtitle")}
    {onclose}
  />
  <SheetContent padding="flush" class="flex flex-col">
    <div class="shrink-0 px-6 pt-3">
      <TabBar
        variant="underline"
        testidPrefix="catalogue"
        activeTab={tab}
        items={[
          { key: "discover", label: $t("connections.catalogue_tab_discover"), count: filtered.length },
          { key: "custom", label: $t("connections.catalogue_tab_custom") },
        ]}
        ontabchange={(k) => (tab = k as CatalogueTab)}
      />
    </div>

    {#if tab === "discover"}
      <FilterChipBar
        chips={[
          { key: "all", label: $t("connections.mcp_chip_all"), color: "hsl(var(--muted-foreground))", count: entries.length },
          { key: "installed", label: $t("connections.mcp_chip_installed"), color: "hsl(var(--success))", count: installedCount },
          { key: "official", label: $t("connections.mcp_chip_official"), color: "hsl(var(--primary))", count: officialCount },
          { key: "community", label: $t("connections.mcp_chip_community"), color: "hsl(var(--info))", count: communityCount },
        ]}
        activeKey={filter}
        onchange={(key) => (filter = key as McpFilter)}
        aria-label={$t("connections.mcp_filter_label")}
        testidPrefix="catalogue-filter"
        class="px-6 pb-3 pt-4"
      />

      <div class="flex-1 overflow-auto px-6 pb-6">
        {#if registryLoading && entries.length === 0}
          <div class="space-y-2" data-testid="catalogue-loading">
            <div class="flex items-center gap-2 text-body-xs text-muted-foreground">
              <Spinner size={12} />
              {$t("connections.catalogue_loading")}
            </div>
            {#each Array(6) as _, i (i)}
              <Skeleton class="h-16 rounded-lg border border-border bg-surface-1" />
            {/each}
          </div>
        {:else if entries.length === 0}
          <EmptyState
            title={$t("connections.empty.no_suggestions_title")}
            desc={$t("connections.empty.no_suggestions_description")}
            tone="neutral"
          >
            {#snippet icon()}<Sparkles size={22} />{/snippet}
          </EmptyState>
        {:else if filtered.length === 0}
          <EmptyState
            title={$t("connections.empty.no_results_title", { values: { query: "" } })}
            desc={$t("connections.empty.no_results_description")}
            tone="neutral"
          >
            {#snippet action()}
              <Button variant="outline" size="sm" onclick={() => (filter = "all")}>
                {$t("connections.filters.clear_all")}
              </Button>
            {/snippet}
          </EmptyState>
        {:else}
          <ul class="space-y-1.5" data-testid="catalogue-list">
            {#each visible as entry (entry.name)}
              <li>
                <CatalogueEntryRow
                  {entry}
                  installed={isInstalled(entry, installedNames, installedByPackage)}
                  official={isOfficial(entry)}
                  onactivate={() => activate(entry)}
                />
              </li>
            {/each}
          </ul>

          <div bind:this={sentinel} class="h-px" aria-hidden="true"></div>

          {#if registryLoading && catalogueTier !== "full"}
            <div
              class="mt-4 flex items-center justify-center gap-2 text-caption text-muted-foreground"
              data-testid="catalogue-bg-load"
            >
              <Spinner size={11} />
              {$t("connections.catalogue_loading_public", { values: { count: entries.length } })}
            </div>
          {:else if hasMore}
            <div
              class="mt-4 flex items-center justify-center text-caption text-muted-foreground/70"
              data-testid="catalogue-more-hint"
            >
              {$t("connections.catalogue_more_hint", { values: { count: filtered.length - limit } })}
            </div>
          {:else if catalogueTier === "full" && filtered.length > 0}
            <div class="mt-4 flex items-center justify-center text-caption text-muted-foreground/60">
              {$t("connections.catalogue_end", { values: { count: filtered.length } })}
            </div>
          {/if}
        {/if}
      </div>
    {:else}
      <CustomMcpForm {oninstalled} />
    {/if}
  </SheetContent>
</Sheet>
