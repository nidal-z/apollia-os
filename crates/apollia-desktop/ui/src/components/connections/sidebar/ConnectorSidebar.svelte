<script lang="ts">
  /**
   * ConnectorSidebar - the split-screen sidebar for Connections.
   *
   * SidebarHeader (search + status FilterChipBar) over two sections: native
   * Apollia connectors, then installed MCP servers sorted by status. Owns its
   * own search + status-filter state; selection is driven by the orchestrator.
   */
  import { t } from "svelte-i18n";
  import { Plus, Search } from "lucide-svelte";
  import { SidebarHeader, FilterChipBar, ErrorBanner } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { statusOf, sortServers } from "$lib/connections/status";
  import type { McpServerStatusView, ConnectorEnrichmentView } from "$lib/types";
  import type {
    NativeConnectorCard,
    Selection,
    StatusFilter,
    McpRowAction,
  } from "../shared/types";
  import type { ProviderId } from "$lib/connections/capabilities";
  import NativeConnectorRow from "./NativeConnectorRow.svelte";
  import McpServerRow from "./McpServerRow.svelte";

  interface Props {
    nativeConnectors: NativeConnectorCard[];
    accountCountFor: (id: ProviderId) => number;
    servers: McpServerStatusView[];
    enrichmentMap: Map<string, ConnectorEnrichmentView>;
    selection: Selection;
    loading: boolean;
    loadError: string | null;
    onselect: (sel: Selection) => void;
    onopencatalogue: () => void;
    oncontext: (name: string, action: McpRowAction) => void;
  }

  let {
    nativeConnectors,
    accountCountFor,
    servers,
    enrichmentMap,
    selection,
    loading,
    loadError,
    onselect,
    onopencatalogue,
    oncontext,
  }: Props = $props();

  let sidebarFilter = $state("");
  let statusFilter = $state<StatusFilter>("all");

  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  const activeCount = $derived(servers.filter((s) => statusOf(s) === "active").length);
  const attentionCount = $derived(servers.filter((s) => statusOf(s) === "attention").length);
  const errorCount = $derived(servers.filter((s) => statusOf(s) === "error").length);
  const idleCount = $derived(servers.filter((s) => statusOf(s) === "idle").length);

  const filteredByStatus = $derived(
    statusFilter === "all" ? servers : servers.filter((s) => statusOf(s) === statusFilter),
  );
  const sorted = $derived(sortServers(filteredByStatus));

  const visibleServers = $derived.by(() => {
    const q = sidebarFilter.trim().toLowerCase();
    if (q === "") return sorted;
    return sorted.filter((s) => {
      const label = resolveEnrichment(s)?.operator_label ?? s.name;
      return label.toLowerCase().includes(q) || s.name.toLowerCase().includes(q);
    });
  });
</script>

<SidebarHeader
  title={$t("connections.sidebar_my_connectors", {
    values: { count: servers.length + nativeConnectors.length },
  })}
>
  {#snippet search()}
    <div class="flex items-center gap-2 rounded-md border border-border bg-surface-1 px-2 py-1.5">
      <Search size={11} class="text-muted-foreground" />
      <Input
        type="search"
        unstyled
        bind:value={sidebarFilter}
        placeholder={$t("connections.sidebar_filter_placeholder")}
        class="flex-1 text-body-xs text-foreground"
      />
    </div>
  {/snippet}
  {#snippet filters()}
    <FilterChipBar
      size="compact"
      chips={[
        { key: "all", label: $t("connections.status_chip_all"), color: "hsl(var(--muted-foreground))", count: servers.length },
        { key: "active", label: $t("connections.status_chip_active"), color: "hsl(var(--success))", count: activeCount, glowWhenActive: true },
        { key: "attention", label: $t("connections.status_chip_attention"), color: "hsl(var(--warning))", count: attentionCount },
        { key: "error", label: $t("connections.status_chip_error"), color: "hsl(var(--destructive))", count: errorCount },
        { key: "idle", label: $t("connections.status_chip_idle"), color: "hsl(var(--muted-foreground))", count: idleCount },
      ]}
      activeKey={statusFilter}
      onchange={(key) => (statusFilter = key as StatusFilter)}
      aria-label={$t("connections.status_filter_label")}
      testidPrefix="connections-sidebar-filter"
    />
  {/snippet}
</SidebarHeader>

<div class="flex-1 overflow-auto px-2.5 pb-3" data-testid="connections-sidebar-list" use:listNavigation>
  {#if loadError}
    <ErrorBanner message={loadError} class="mx-1.5 my-2" data-testid="connections-error" />
  {/if}

  <div class="section-meta mb-1.5 mt-1 px-2 text-overline">
    {$t("connections.sidebar_native_section")}
  </div>
  {#each nativeConnectors as connector (connector.id)}
    <NativeConnectorRow
      {connector}
      accountCount={accountCountFor(connector.id)}
      selected={selection?.kind === "native" && selection.provider === connector.id}
      onselect={() => onselect({ kind: "native", provider: connector.id })}
    />
  {/each}

  <div class="section-meta mb-1.5 mt-4 px-2 text-overline">
    {$t("connections.sidebar_mcp_section", { values: { count: sorted.length } })}
  </div>
  {#if loading && servers.length === 0}
    {#each Array(4) as _, i (i)}
      <div class="flex items-center gap-2.5 px-2.5 py-2.5">
        <Skeleton class="h-8 w-0.5 rounded-sm" />
        <div class="flex-1 space-y-1.5">
          <Skeleton class="h-3 w-3/5 rounded" />
          <Skeleton class="h-2 w-2/5 rounded" />
        </div>
      </div>
    {/each}
  {:else if visibleServers.length === 0}
    <p class="px-2 py-3 text-caption text-muted-foreground/70">
      {servers.length === 0
        ? $t("connections.sidebar_no_servers")
        : sidebarFilter
          ? $t("connections.sidebar_no_results")
          : $t("connections.sidebar_no_servers_status")}
    </p>
  {:else}
    {#each visibleServers as server (server.name)}
      <McpServerRow
        {server}
        label={resolveEnrichment(server)?.operator_label ?? server.name}
        status={statusOf(server)}
        selected={selection?.kind === "mcp" && selection.name === server.name}
        onselect={() => onselect({ kind: "mcp", name: server.name })}
        oncontext={(action) => oncontext(server.name, action)}
      />
    {/each}
  {/if}
</div>

<div class="border-t border-border px-3 py-2">
  <Button
    variant="outline"
    size="sm"
    onclick={onopencatalogue}
    class="w-full justify-center"
    data-testid="connections-open-catalogue"
  >
    {#snippet icon()}<Plus size={12} />{/snippet}
    {$t("connections.add_connector")}
  </Button>
</div>
