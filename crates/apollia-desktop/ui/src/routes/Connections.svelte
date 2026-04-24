<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { EmptyState } from "$lib/components/layout";
  import { EMPTY_STATES } from "$lib/i18n/strings/empty-states";
  import type {
    AgentListItem,
    ConnectorEnrichmentView,
    McpServerStatusView,
    RegistryServerView,
  } from "$lib/types";
  import McpDisclaimerDialog, {
    isDisclaimerAccepted,
  } from "../components/integrations/McpDisclaimerDialog.svelte";
  import ConnectorWizard from "../components/integrations/ConnectorWizard.svelte";
  import OperatorServerManage from "../components/integrations/OperatorServerManage.svelte";
  import ConnectionHeroSection, {
    type ConnectionSegment,
  } from "../components/connections/ConnectionHeroSection.svelte";
  import ConnectionFilterBar, {
    type StatusFilter,
  } from "../components/connections/ConnectionFilterBar.svelte";
  import ConnectionAppCard from "../components/connections/ConnectionAppCard.svelte";
  import ConnectionErrorModal from "../components/connections/ConnectionErrorModal.svelte";
  import { rankSuggestions } from "../components/connections/ConnectionSuggestions.svelte";

  interface Props {
    onNavigateTasks?: () => void;
  }

  let { onNavigateTasks }: Props = $props();

  let servers = $state<McpServerStatusView[]>([]);
  let enrichmentMap = $state(new Map<string, ConnectorEnrichmentView>());
  let registry = $state<RegistryServerView[]>([]);
  let agents = $state<AgentListItem[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);

  let activeSegment = $state<ConnectionSegment>("active");
  let query = $state("");
  let category = $state("all");
  let status = $state<StatusFilter>("all");

  let disclaimerOpen = $state(false);
  let selectedRegistryServer = $state<RegistryServerView | null>(null);
  let wizardOpen = $state(false);
  let managedServerName = $state<string | null>(null);
  let manageOpen = $state(false);
  let errorModalOpen = $state(false);
  let errorServer = $state<McpServerStatusView | null>(null);

  async function loadAll(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [serverList, enrichmentEntries, registryEntries, agentList] = await Promise.all([
        invoke<McpServerStatusView[]>("list_mcp_servers"),
        invoke<Array<{ package_identifier: string; enrichment: ConnectorEnrichmentView }>>(
          "list_mcp_enrichments",
        ),
        invoke<RegistryServerView[]>("fetch_mcp_registry").catch(() => [] as RegistryServerView[]),
        invoke<AgentListItem[]>("list_agents").catch(() => [] as AgentListItem[]),
      ]);
      servers = serverList;
      enrichmentMap = new Map(enrichmentEntries.map((e) => [e.package_identifier, e.enrichment]));
      registry = registryEntries;
      agents = agentList;
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  function registryEnrichment(server: RegistryServerView): ConnectorEnrichmentView | null {
    return server.enrichment;
  }

  function serverCategory(server: McpServerStatusView): string | null {
    const e = resolveEnrichment(server);
    return e?.category ?? null;
  }

  // ── derived lists ──────────────────────────────────────────────────────────

  const suggestions = $derived(rankSuggestions(registry, agents, 6));

  const installedNames = $derived(new Set(servers.map((s) => s.name)));

  /** Categories present in the current segment, "all" always first. */
  const availableCategories = $derived.by(() => {
    const seen = new Set<string>();
    const add = (c: string | null | undefined) => {
      if (c) seen.add(c);
    };
    if (activeSegment === "active") {
      for (const s of servers) add(serverCategory(s));
    } else if (activeSegment === "suggested") {
      for (const s of suggestions) add(s.enrichment?.category ?? s.category);
    } else {
      for (const s of registry) add(s.enrichment?.category ?? s.category);
    }
    return ["all", ...Array.from(seen).sort()];
  });

  function matchesQuery(title: string, description: string | null | undefined): boolean {
    const q = query.trim().toLowerCase();
    if (q.length === 0) return true;
    if (title.toLowerCase().includes(q)) return true;
    if (description?.toLowerCase().includes(q)) return true;
    return false;
  }

  const activeServers = $derived.by(() => {
    return servers.filter((s) => {
      if (category !== "all" && serverCategory(s) !== category) return false;
      const e = resolveEnrichment(s);
      const title = e?.operator_label ?? s.name;
      if (!matchesQuery(title, s.error)) return false;
      if (status === "not_installed") return false;
      return true;
    });
  });

  const catalogueEntries = $derived.by(() => {
    return registry.filter((s) => {
      if (!s) return false;
      const cat = s.enrichment?.category ?? s.category;
      if (category !== "all" && cat !== category) return false;
      const title = s.enrichment?.operator_label ?? s.title ?? s.name;
      if (!matchesQuery(title, s.description)) return false;
      if (status === "installed" && !s.is_installed) return false;
      if (status === "not_installed" && s.is_installed) return false;
      return true;
    });
  });

  const suggestedEntries = $derived.by(() => {
    return suggestions.filter((s) => {
      if (!s) return false;
      const cat = s.enrichment?.category ?? s.category;
      if (category !== "all" && cat !== category) return false;
      const title = s.enrichment?.operator_label ?? s.title ?? s.name;
      if (!matchesQuery(title, s.description)) return false;
      return true;
    });
  });

  const hasActiveFilters = $derived(
    query.trim().length > 0 || category !== "all" || status !== "all",
  );

  function clearAll() {
    query = "";
    category = "all";
    status = "all";
  }

  // ── CTAs & flows ───────────────────────────────────────────────────────────

  function openWizardFor(server: RegistryServerView) {
    selectedRegistryServer = server;
    wizardOpen = true;
  }

  function handleConnect(server: RegistryServerView) {
    if (!isDisclaimerAccepted()) {
      // Defer: open disclaimer first, then wizard.
      selectedRegistryServer = server;
      disclaimerOpen = true;
      return;
    }
    openWizardFor(server);
  }

  function handleDisclaimerAccept() {
    disclaimerOpen = false;
    if (selectedRegistryServer) openWizardFor(selectedRegistryServer);
  }

  function handleWizardClose() {
    wizardOpen = false;
    selectedRegistryServer = null;
  }

  function handleWizardComplete() {
    wizardOpen = false;
    selectedRegistryServer = null;
    loadAll();
  }

  function handleManage(name: string) {
    managedServerName = name;
    manageOpen = true;
  }

  function handleManageClose() {
    manageOpen = false;
    managedServerName = null;
  }

  function handleManageDisconnect() {
    manageOpen = false;
    managedServerName = null;
    loadAll();
  }

  function handleHealthClick(server: McpServerStatusView) {
    if (server.error || !server.connected) {
      errorServer = server;
      errorModalOpen = true;
    }
  }

  async function handleRetry(name: string) {
    errorModalOpen = false;
    try {
      await invoke("restart_mcp_server", { name });
    } catch {
      /* surfaced via reload */
    }
    loadAll();
  }

  function handleViewLogs(name: string) {
    errorModalOpen = false;
    handleManage(name);
  }

  function handleReconnect(name: string) {
    const server = servers.find((s) => s.name === name);
    if (server) {
      errorServer = server;
      errorModalOpen = true;
    }
  }

  // ── routing browse catalogue CTA ───────────────────────────────────────────

  function handleBrowseCatalogue() {
    query = "";
    category = "all";
    status = "all";
    activeSegment = "catalogue";
  }

  $effect(() => {
    loadAll();
  });

  // Avoid unused-binding warning from prop.
  $effect(() => {
    void onNavigateTasks;
  });
</script>

<div class="mx-auto w-full max-w-7xl flex flex-col gap-6" data-testid="connections-route">
  <div class="flex items-start justify-between gap-3">
    <div class="flex-1 min-w-0">
      <ConnectionHeroSection
        active={activeSegment}
        activeCount={servers.length}
        suggestedCount={suggestedEntries.length}
        catalogueCount={catalogueEntries.length}
        onchange={(seg) => (activeSegment = seg)}
      />
    </div>
    <Button size="sm" onclick={handleBrowseCatalogue} data-testid="connections-add-btn">
      <Plus size={16} class="mr-1.5" />
      {$t("connections.add_connection")}
    </Button>
  </div>

  <ConnectionFilterBar
    {query}
    {category}
    availableCategories={availableCategories}
    {status}
    hasActive={hasActiveFilters}
    onquery={(v) => (query = v)}
    oncategory={(v) => (category = v)}
    onstatus={(v) => (status = v)}
    onclearall={clearAll}
  />

  {#if loading}
    <div
      class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
      data-testid="connections-loading"
    >
      {#each Array(6) as _, i (i)}
        <Skeleton class="h-40" />
      {/each}
    </div>
  {:else if loadError}
    <p class="text-sm text-destructive" data-testid="connections-error">{loadError}</p>
  {:else if activeSegment === "active"}
    {#if servers.length === 0}
      <EmptyState
        icon={EMPTY_STATES.integrations.icon}
        title={$t("connections.empty.no_connections_title")}
        description={$t("connections.empty.no_connections_description")}
        primaryLabel={$t("connections.empty.browse_catalogue")}
        primaryAction={handleBrowseCatalogue}
        page="connections"
      />
    {:else if activeServers.length === 0}
      <EmptyState
        icon={EMPTY_STATES.integrations.icon}
        title={$t("connections.empty.no_results_title", { values: { query } })}
        description={$t("connections.empty.no_results_description")}
        primaryLabel={$t("connections.filters.clear_all")}
        primaryAction={clearAll}
        page="connections"
      />
    {:else}
      <div
        class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
        data-testid="connections-active-grid"
      >
        {#each activeServers as server (server.name)}
          {@const enrichment = resolveEnrichment(server)}
          <ConnectionAppCard
            {server}
            {enrichment}
            toolsCount={server.tools_count}
            installed={true}
            onmanage={handleManage}
            onreconnect={handleReconnect}
            onhealthClick={handleHealthClick}
          />
        {/each}
      </div>
    {/if}
  {:else if activeSegment === "suggested"}
    {#if suggestedEntries.length === 0}
      <EmptyState
        icon={EMPTY_STATES.integrations.icon}
        title={$t("connections.empty.no_suggestions_title")}
        description={$t("connections.empty.no_suggestions_description")}
        primaryLabel={$t("connections.empty.browse_catalogue")}
        primaryAction={handleBrowseCatalogue}
        page="connections"
      />
    {:else}
      <div
        class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
        data-testid="connections-suggested-grid"
      >
        {#each suggestedEntries as entry (entry?.name)}
          {#if entry}
            <ConnectionAppCard
              registry={entry}
              enrichment={registryEnrichment(entry)}
              toolsCount={(entry.packages ?? [])[0]?.package_arguments.length ?? 0}
              installed={installedNames.has(entry.name)}
              onconnect={handleConnect}
              onmanage={handleManage}
            />
          {/if}
        {/each}
      </div>
    {/if}
  {:else}
    {#if catalogueEntries.length === 0}
      <EmptyState
        icon={EMPTY_STATES.integrations.icon}
        title={$t("connections.empty.no_results_title", { values: { query } })}
        description={$t("connections.empty.no_results_description")}
        primaryLabel={$t("connections.filters.clear_all")}
        primaryAction={clearAll}
        page="connections"
      />
    {:else}
      <div
        class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
        data-testid="connections-catalogue-grid"
      >
        {#each catalogueEntries as entry (entry?.name)}
          {#if entry}
            <ConnectionAppCard
              registry={entry}
              enrichment={registryEnrichment(entry)}
              toolsCount={(entry.packages ?? [])[0]?.package_arguments.length ?? 0}
              installed={entry.is_installed || installedNames.has(entry.name)}
              onconnect={handleConnect}
              onmanage={handleManage}
            />
          {/if}
        {/each}
      </div>
    {/if}
  {/if}
</div>

{#if selectedRegistryServer}
  <ConnectorWizard
    server={selectedRegistryServer}
    open={wizardOpen}
    onclose={handleWizardClose}
    oncomplete={handleWizardComplete}
  />
{/if}

<McpDisclaimerDialog
  open={disclaimerOpen}
  onaccept={handleDisclaimerAccept}
  onclose={() => {
    disclaimerOpen = false;
    selectedRegistryServer = null;
  }}
/>

{#if managedServerName !== null}
  <OperatorServerManage
    serverName={managedServerName}
    open={manageOpen}
    onclose={handleManageClose}
    onDisconnect={handleManageDisconnect}
  />
{/if}

<ConnectionErrorModal
  open={errorModalOpen}
  server={errorServer}
  enrichment={errorServer ? resolveEnrichment(errorServer) : null}
  onclose={() => (errorModalOpen = false)}
  onretry={handleRetry}
  onviewLogs={handleViewLogs}
/>
