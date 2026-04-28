<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Plus, Sparkles, Link as LinkIcon } from "lucide-svelte";
  import {
    PageHeader,
    SectionTitle,
    Chip,
    BtnPrimary,
    BtnSecondary,
    EmptyState,
    ConnectionCard,
    type ConnectionStatus,
  } from "$lib/components/operator";
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
  import ConnectionErrorModal from "../components/connections/ConnectionErrorModal.svelte";
  import { rankSuggestions } from "../components/connections/ConnectionSuggestions.svelte";

  interface Props {
    onNavigateTasks?: () => void;
  }

  let { onNavigateTasks }: Props = $props();

  // ── State (preserved from old Connections.svelte + Integrations.svelte) ────
  let servers = $state<McpServerStatusView[]>([]);
  let enrichmentMap = $state(new Map<string, ConnectorEnrichmentView>());
  let registry = $state<RegistryServerView[]>([]);
  let agents = $state<AgentListItem[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);

  // Filter state
  type StatusFilter = "all" | "active" | "error" | "idle";
  let statusFilter = $state<StatusFilter>("all");
  let mcpFilter = $state<"all" | "installed" | "official" | "community">("all");

  // Modal / wizard state
  let disclaimerOpen = $state(false);
  let selectedRegistryServer = $state<RegistryServerView | null>(null);
  let wizardOpen = $state(false);
  let managedServerName = $state<string | null>(null);
  let manageOpen = $state(false);
  let errorModalOpen = $state(false);
  let errorServer = $state<McpServerStatusView | null>(null);

  // ── Loaders ────────────────────────────────────────────────────────────────

  async function loadAll(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [serverList, enrichmentEntries, registryEntries, agentList] =
        await Promise.all([
          invoke<McpServerStatusView[]>("list_mcp_servers"),
          invoke<Array<{ package_identifier: string; enrichment: ConnectorEnrichmentView }>>(
            "list_mcp_enrichments",
          ),
          invoke<RegistryServerView[]>("fetch_mcp_registry").catch(
            () => [] as RegistryServerView[],
          ),
          invoke<AgentListItem[]>("list_agents").catch(() => [] as AgentListItem[]),
        ]);
      servers = serverList;
      enrichmentMap = new Map(
        enrichmentEntries.map((e) => [e.package_identifier, e.enrichment]),
      );
      registry = registryEntries;
      agents = agentList;
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  function statusOf(server: McpServerStatusView): ConnectionStatus {
    if (server.error) return "error";
    if (server.connected) return "active";
    return "idle";
  }

  function syncLabel(server: McpServerStatusView): string | undefined {
    if (!server.last_call_at) return undefined;
    const diffMs = Date.now() - new Date(server.last_call_at).getTime();
    if (Number.isNaN(diffMs) || diffMs < 0) return undefined;
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 1) return "à l'instant";
    if (mins < 60) return `il y a ${mins} min`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `il y a ${hours} h`;
    const days = Math.floor(hours / 24);
    return `il y a ${days} j`;
  }

  /** Stable color for the logo tile, derived from the server name. */
  function logoColorFor(name: string): string {
    const palette = [
      "#4285f4",
      "#611f69",
      "#1c1d2b",
      "#ea4335",
      "#0ea5e9",
      "#a855f7",
      "#16a34a",
      "#f59e0b",
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i += 1) hash = (hash * 31 + name.charCodeAt(i)) | 0;
    return palette[Math.abs(hash) % palette.length];
  }

  // ── Derived sets ───────────────────────────────────────────────────────────

  const installedNames = $derived(new Set(servers.map((s) => s.name)));

  const activeCount = $derived(servers.filter((s) => s.connected && !s.error).length);
  const errorCount = $derived(servers.filter((s) => !!s.error).length);
  const idleCount = $derived(
    servers.filter((s) => !s.connected && !s.error).length,
  );

  const filteredServers = $derived(
    servers.filter((s) => {
      const st = statusOf(s);
      if (statusFilter === "all") return true;
      return st === statusFilter;
    }),
  );

  /** MCP catalogue — combine ranked suggestions + remaining registry, dedup. */
  const mcpEntries = $derived.by(() => {
    const ranked = rankSuggestions(registry, agents, 12);
    const seen = new Set(ranked.map((r) => r.name));
    const rest = registry.filter((r) => !seen.has(r.name));
    return [...ranked, ...rest];
  });

  const officialCount = $derived(
    mcpEntries.filter(
      (e) =>
        e.trust_level === "verified_official" || e.trust_level === "community_verified",
    ).length,
  );
  const communityCount = $derived(
    mcpEntries.filter((e) => e.trust_level === "community" || e.trust_level === "custom")
      .length,
  );
  const installedRegCount = $derived(
    mcpEntries.filter((e) => e.is_installed || installedNames.has(e.name)).length,
  );

  const filteredMcp = $derived(
    mcpEntries.filter((e) => {
      const installed = e.is_installed || installedNames.has(e.name);
      if (mcpFilter === "installed") return installed;
      if (mcpFilter === "official")
        return (
          e.trust_level === "verified_official" || e.trust_level === "community_verified"
        );
      if (mcpFilter === "community")
        return e.trust_level === "community" || e.trust_level === "custom";
      return true;
    }),
  );

  const heroKicker = $derived(
    `INTÉGRATIONS · ${activeCount} ACTIVE${activeCount > 1 ? "S" : ""}` +
      (errorCount > 0 ? ` · ${errorCount} ERREUR${errorCount > 1 ? "S" : ""}` : ""),
  );

  // ── CTAs & flows (preserved from old Connections.svelte) ───────────────────

  function openWizardFor(server: RegistryServerView) {
    selectedRegistryServer = server;
    wizardOpen = true;
  }

  function handleConnect(server: RegistryServerView) {
    if (!isDisclaimerAccepted()) {
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

  function handleApolliaCardClick(server: McpServerStatusView) {
    if (server.error || !server.connected) {
      errorServer = server;
      errorModalOpen = true;
    } else {
      handleManage(server.name);
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

  function handleAddCustomMcp() {
    // Open wizard with no preselected registry entry — handled by ConnectorWizard
    // when fed a synthetic entry; for now, surface the disclaimer + scroll to
    // catalogue. Custom URL entry is exposed inside OperatorServerManage flows.
    if (!isDisclaimerAccepted()) {
      disclaimerOpen = true;
    }
  }

  $effect(() => {
    loadAll();
  });

  // Avoid unused-binding warning from prop.
  $effect(() => {
    void onNavigateTasks;
  });
</script>

<div class="flex flex-col" data-testid="connections-route">
  <PageHeader
    kicker={heroKicker}
    title={$t("connections.hero.title")}
    subtitle="Les services externes qu'Apollia peut utiliser avec votre accord. Deux familles : les Connecteurs Apollia (curés, premium) et le Catalogue MCP (protocole ouvert, communauté)."
  >
    {#snippet actions()}
      <BtnPrimary onclick={handleAddCustomMcp}>
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("connections.add_connection")}
      </BtnPrimary>
    {/snippet}
  </PageHeader>

  {#if loadError}
    <p class="text-sm text-destructive px-8 py-4" data-testid="connections-error">
      {loadError}
    </p>
  {/if}

  <!-- ============ SECTION APOLLIA ============ -->
  <SectionTitle count={`${activeCount} actif${activeCount > 1 ? "s" : ""} · ${servers.length} total`}>
    Connecteurs Apollia
  </SectionTitle>

  <div class="px-8 pb-2 flex items-center gap-2" data-testid="connections-status-filters">
    <Chip
      tone={statusFilter === "all" ? "primary" : "neutral"}
      size="sm"
      outline={statusFilter !== "all"}
    >
      <button
        type="button"
        class="bg-transparent border-0 p-0 cursor-pointer"
        onclick={() => (statusFilter = "all")}
      >
        Tous · {servers.length}
      </button>
    </Chip>
    <Chip
      tone={statusFilter === "active" ? "success" : "neutral"}
      size="sm"
      outline={statusFilter !== "active"}
    >
      <button
        type="button"
        class="bg-transparent border-0 p-0 cursor-pointer"
        onclick={() => (statusFilter = "active")}
      >
        Actifs · {activeCount}
      </button>
    </Chip>
    <Chip
      tone={statusFilter === "error" ? "danger" : "neutral"}
      size="sm"
      outline={statusFilter !== "error"}
    >
      <button
        type="button"
        class="bg-transparent border-0 p-0 cursor-pointer"
        onclick={() => (statusFilter = "error")}
      >
        Erreur · {errorCount}
      </button>
    </Chip>
    <Chip
      tone={statusFilter === "idle" ? "neutral" : "neutral"}
      size="sm"
      outline={statusFilter !== "idle"}
    >
      <button
        type="button"
        class="bg-transparent border-0 p-0 cursor-pointer"
        onclick={() => (statusFilter = "idle")}
      >
        Inactif · {idleCount}
      </button>
    </Chip>
  </div>

  <div class="px-8 pt-3 pb-2">
    {#if loading}
      <div class="grid grid-cols-3 gap-3.5" data-testid="connections-loading">
        {#each Array(6) as _, i (i)}
          <div class="h-24 rounded-xl bg-surface-1 border border-border animate-pulse"></div>
        {/each}
      </div>
    {:else if servers.length === 0}
      <EmptyState
        title={$t("connections.empty.no_connections_title")}
        desc={$t("connections.empty.no_connections_description")}
        tone="primary"
      >
        {#snippet icon()}<LinkIcon size={22} />{/snippet}
        {#snippet action()}
          <BtnPrimary onclick={handleAddCustomMcp}>
            {#snippet icon()}<Plus size={12} />{/snippet}
            {$t("connections.add_connection")}
          </BtnPrimary>
        {/snippet}
      </EmptyState>
    {:else if filteredServers.length === 0}
      <EmptyState
        title={$t("connections.empty.no_results_title", { values: { query: "" } })}
        desc={$t("connections.empty.no_results_description")}
        tone="neutral"
      >
        {#snippet action()}
          <BtnSecondary onclick={() => (statusFilter = "all")}>
            {$t("connections.filters.clear_all")}
          </BtnSecondary>
        {/snippet}
      </EmptyState>
    {:else}
      <div
        class="grid grid-cols-3 gap-3.5"
        data-testid="connections-apollia-grid"
      >
        {#each filteredServers as server (server.name)}
          {@const enrichment = resolveEnrichment(server)}
          {@const label = enrichment?.operator_label ?? server.name}
          <ConnectionCard
            variant="apollia"
            name={label}
            description={enrichment?.category ?? server.server_info ?? undefined}
            status={statusOf(server)}
            capabilities={server.tools_count}
            logoColor={logoColorFor(label)}
            error={server.error ?? undefined}
            sync={syncLabel(server)}
            onclick={() => handleApolliaCardClick(server)}
          />
        {/each}
      </div>
    {/if}
  </div>

  <!-- ============ SECTION MCP ============ -->
  <div class="mt-4 border-t border-border/40">
    <SectionTitle count={`${mcpEntries.length} serveurs`}>
      Catalogue MCP
      {#snippet action()}
        <Chip tone="neutral" size="sm">protocole ouvert · communauté</Chip>
      {/snippet}
    </SectionTitle>

    <div class="px-8 pb-3 flex items-center gap-2" data-testid="connections-mcp-filters">
      <Chip
        tone={mcpFilter === "all" ? "primary" : "neutral"}
        size="sm"
        outline={mcpFilter !== "all"}
      >
        <button
          type="button"
          class="bg-transparent border-0 p-0 cursor-pointer"
          onclick={() => (mcpFilter = "all")}
        >
          Tous · {mcpEntries.length}
        </button>
      </Chip>
      <Chip
        tone={mcpFilter === "installed" ? "success" : "neutral"}
        size="sm"
        outline={mcpFilter !== "installed"}
      >
        <button
          type="button"
          class="bg-transparent border-0 p-0 cursor-pointer"
          onclick={() => (mcpFilter = "installed")}
        >
          Installés · {installedRegCount}
        </button>
      </Chip>
      <Chip
        tone={mcpFilter === "official" ? "primary" : "neutral"}
        size="sm"
        outline={mcpFilter !== "official"}
      >
        <button
          type="button"
          class="bg-transparent border-0 p-0 cursor-pointer"
          onclick={() => (mcpFilter = "official")}
        >
          Officiels · {officialCount}
        </button>
      </Chip>
      <Chip
        tone={mcpFilter === "community" ? "info" : "neutral"}
        size="sm"
        outline={mcpFilter !== "community"}
      >
        <button
          type="button"
          class="bg-transparent border-0 p-0 cursor-pointer"
          onclick={() => (mcpFilter = "community")}
        >
          Communauté · {communityCount}
        </button>
      </Chip>
    </div>

    <div class="px-8 pb-8">
      {#if loading}
        <div class="grid grid-cols-4 gap-3" data-testid="connections-mcp-loading">
          {#each Array(8) as _, i (i)}
            <div class="h-20 rounded-xl bg-surface-1 border border-border animate-pulse"></div>
          {/each}
        </div>
      {:else if mcpEntries.length === 0}
        <EmptyState
          title={$t("connections.empty.no_suggestions_title")}
          desc={$t("connections.empty.no_suggestions_description")}
          tone="neutral"
        >
          {#snippet icon()}<Sparkles size={22} />{/snippet}
        </EmptyState>
      {:else if filteredMcp.length === 0}
        <EmptyState
          title={$t("connections.empty.no_results_title", { values: { query: "" } })}
          desc={$t("connections.empty.no_results_description")}
          tone="neutral"
        >
          {#snippet action()}
            <BtnSecondary onclick={() => (mcpFilter = "all")}>
              {$t("connections.filters.clear_all")}
            </BtnSecondary>
          {/snippet}
        </EmptyState>
      {:else}
        <div class="grid grid-cols-4 gap-3" data-testid="connections-mcp-grid">
          {#each filteredMcp as entry (entry.name)}
            {@const installed = entry.is_installed || installedNames.has(entry.name)}
            {@const isOfficial =
              entry.trust_level === "verified_official" ||
              entry.trust_level === "community_verified"}
            {@const vendor = isOfficial ? "officiel" : "communauté"}
            {@const url = entry.remotes?.[0]?.url ?? entry.repository_url ?? ""}
            <div class="flex flex-col gap-1.5">
              <ConnectionCard
                variant="mcp"
                name={entry.enrichment?.operator_label ?? entry.title ?? entry.name}
                vendor={vendor}
                description={entry.description ?? undefined}
                official={isOfficial}
                installed={installed}
                onclick={() =>
                  installed ? handleManage(entry.name) : handleConnect(entry)}
              />
              {#if url}
                <span
                  class="px-3 text-[10px] font-mono text-muted-foreground/60 truncate"
                  title={url}
                >
                  {url}
                </span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
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
