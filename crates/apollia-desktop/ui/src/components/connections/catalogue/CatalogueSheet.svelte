<script lang="ts">
  /**
   * CatalogueSheet - the "add a connection" Sheet: the catalogue and a custom
   * connection.
   *
   * The catalogue and the custom form share one Sheet (same audience). The
   * catalogue opens on the native accounts Apollia ships (Google, Microsoft),
   * whose page is where their client is entered and their accounts connected,
   * then the selection Apollia ships, ranked. The public catalogue is read only
   * when "Explore all" is pressed; a search field narrows whatever is loaded,
   * trust-level chips filter it and IntersectionObserver paginates it. The
   * technical names of the catalogue and of the custom form are shown in
   * developer mode only.
   */
  import { t } from "svelte-i18n";
  import { Search, Sparkles, UserRound } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import { developerMode } from "$lib/stores/developerMode";
  import { NATIVE_PROVIDERS, NATIVE_PROVIDER_KEYS } from "$lib/connections/connectionRows";
  import type { ProviderId } from "$lib/connections/capabilities";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import { EmptyState, FilterChipBar } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Spinner } from "$lib/components/ui/progress";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { TabBar } from "$lib/components/ui/tabs";
  import { Sheet, SheetHeader, SheetContent } from "$lib/components/ui/sheet";
  import CatalogueEntryRow from "./CatalogueEntryRow.svelte";
  import CustomMcpForm from "./CustomMcpForm.svelte";
  import { rankSuggestions } from "../ConnectionSuggestions.svelte";
  import { installedNameFor, isInstalled, isOfficial } from "$lib/connections/vendor";
  import { matchesCatalogueQuery, type CatalogueTier } from "$lib/connections/catalogueSource";
  import type { AgentListItem, RegistryServerView } from "$lib/types";

  type CatalogueTab = "discover" | "custom";
  type McpFilter = "all" | "installed" | "official" | "community";

  interface Props {
    open: boolean;
    registry: RegistryServerView[];
    agents: AgentListItem[];
    installedNames: Set<string>;
    installedByPackage: Map<string, string>;
    registryLoading: boolean;
    catalogueTier: CatalogueTier;
    onclose: () => void;
    /** Read the public catalogue: the only gesture that asks it. */
    onexploreall: () => void;
    /** Show the selection alone again. */
    onbacktoselection: () => void;
    onconnect: (entry: RegistryServerView) => void;
    onmanage: (name: string) => void;
    oninstalled: () => void;
    /** Open the page of a native provider, to set its client or connect an account. */
    onaddnative: (provider: ProviderId) => void;
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
    onexploreall,
    onbacktoselection,
    onconnect,
    onmanage,
    oninstalled,
    onaddnative,
  }: Props = $props();

  const CATALOG_PAGE = 48;
  let tab = $state<CatalogueTab>("discover");
  let filter = $state<McpFilter>("all");
  let limit = $state(CATALOG_PAGE);
  let query = $state("");

  // Each opening starts on an empty search.
  $effect(() => {
    if (open) query = "";
  });

  const loaded = $derived.by(() => {
    const ranked = rankSuggestions(registry, agents, 12);
    const seen = new Set(ranked.map((r) => r.name));
    return [...ranked, ...registry.filter((r) => !seen.has(r.name))];
  });

  const entries = $derived(loaded.filter((e) => matchesCatalogueQuery(e, query)));

  const officialCount = $derived(entries.filter((e) => isOfficial(e)).length);
  const communityCount = $derived(entries.filter((e) => !isOfficial(e)).length);
  const installedCount = $derived(
    entries.filter((e) => isInstalled(e, installedNames, installedByPackage)).length,
  );

  const filtered = $derived(
    entries.filter((e) => {
      if (filter === "installed") return isInstalled(e, installedNames, installedByPackage);
      if (filter === "official") return isOfficial(e);
      if (filter === "community") return !isOfficial(e);
      return true;
    }),
  );

  // Reset the visible slice when the filter or the search changes.
  $effect(() => {
    void filter;
    void query;
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
    title={$t("connections.add_connection")}
    subtitle={$developerMode
      ? $t("dev.connections.catalogue_subtitle")
      : $t("connections.catalogue_subtitle")}
    {onclose}
  />
  <SheetContent padding="flush" class="flex flex-col">
    <div class="shrink-0 px-6 pt-3">
      <TabBar
        variant="underline"
        testidPrefix="catalogue"
        activeTab={tab}
        items={[
          {
            key: "discover",
            label: $developerMode
              ? $t("dev.connections.catalogue_tab_discover")
              : $t("connections.catalogue_tab_discover"),
            count: filtered.length,
          },
          {
            key: "custom",
            label: $developerMode
              ? $t("dev.connections.catalogue_tab_custom")
              : $t("connections.catalogue_tab_custom"),
          },
        ]}
        ontabchange={(k) => (tab = k as CatalogueTab)}
      />
    </div>

    {#if tab === "discover"}
      <div class="shrink-0 px-6 pt-4">
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
        <ul class="mb-3 space-y-1.5" data-testid="catalogue-native-list">
          {#each NATIVE_PROVIDERS as provider (provider)}
            <li>
              <ListRow
                variant="row"
                border={false}
                pad="snug"
                class="rounded-lg border border-border/60 bg-card elev-interactive hover:border-primary/40"
                onclick={() => onaddnative(provider)}
                data-testid="catalogue-native-{provider}"
              >
                <ConnectorTile size="md" accent="hsl(var(--primary))">
                  {#snippet icon()}<UserRound size={14} />{/snippet}
                </ConnectorTile>
                <div class="min-w-0 flex-1">
                  <div class="truncate text-body-sm font-medium text-foreground">
                    {$t(NATIVE_PROVIDER_KEYS[provider].name)}
                  </div>
                  <div class="truncate text-caption text-muted-foreground">
                    {$t(NATIVE_PROVIDER_KEYS[provider].description)}
                  </div>
                </div>
              </ListRow>
            </li>
          {/each}
        </ul>
        {#if registryLoading && loaded.length === 0}
          <div class="space-y-2" data-testid="catalogue-loading">
            <div class="flex items-center gap-2 text-body-xs text-muted-foreground">
              <Spinner size={12} />
              {$developerMode
                ? $t("dev.connections.catalogue_loading")
                : $t("connections.catalogue_loading")}
            </div>
            {#each Array(6) as _, i (i)}
              <Skeleton class="h-16 rounded-lg border border-border bg-surface-1" />
            {/each}
          </div>
        {:else if loaded.length === 0}
          <EmptyState
            title={$t("connections.empty.no_suggestions_title")}
            desc={$t("connections.empty.no_suggestions_description")}
            tone="neutral"
          >
            {#snippet icon()}<Sparkles size={22} />{/snippet}
          </EmptyState>
        {:else if filtered.length === 0}
          <EmptyState
            title={$t("connections.empty.no_results_title", { values: { query: query.trim() } })}
            desc={$t("connections.empty.no_results_description")}
            tone="neutral"
          >
            {#snippet action()}
              <Button
                variant="outline"
                size="sm"
                onclick={() => {
                  filter = "all";
                  query = "";
                }}
              >
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

          {#if registryLoading && catalogueTier === "curated"}
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

        {#if catalogueTier === "curated" && !registryLoading}
          <div
            class="mt-6 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4"
            data-testid="catalogue-explore-footer"
          >
            <p class="text-caption text-muted-foreground">{$t("connections.catalogue_explore_note")}</p>
            <Button variant="outline" size="sm" onclick={onexploreall} data-testid="catalogue-explore-all">
              {$t("connections.catalogue_explore_all")}
            </Button>
          </div>
        {:else if catalogueTier === "full"}
          <div class="mt-6 flex justify-end border-t border-border pt-4">
            <Button
              variant="ghost"
              size="sm"
              onclick={onbacktoselection}
              data-testid="catalogue-back-to-selection"
            >
              {$t("connections.catalogue_back_to_selection")}
            </Button>
          </div>
        {/if}
      </div>
    {:else}
      <CustomMcpForm {oninstalled} />
    {/if}
  </SheetContent>
</Sheet>
