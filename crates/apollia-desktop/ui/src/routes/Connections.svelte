<script lang="ts">
  /**
   * Connections - the OAuth + MCP connector hub (canonical v0.1.0 route).
   *
   * Thin orchestrator: owns the top-level data (installed servers, enrichments,
   * registry, native accounts) and selection, wires IPC via
   * `$lib/ipc/connections`, and composes the decomposed sub-components under
   * `components/connections/*`. Business logic lives in `$lib/connections/*`
   * (pure, testable) and inside the self-contained detail / catalogue / oauth
   * components.
   */
  import { t } from "svelte-i18n";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { SplitLayout } from "$lib/components/operator";
  import ConnectorSidebar from "../components/connections/sidebar/ConnectorSidebar.svelte";
  import NativeConnectorDetail from "../components/connections/detail/NativeConnectorDetail.svelte";
  import McpServerDetail from "../components/connections/detail/McpServerDetail.svelte";
  import CatalogueSheet from "../components/connections/catalogue/CatalogueSheet.svelte";
  import NativeOauthDialog from "../components/connections/oauth/NativeOauthDialog.svelte";
  import ConnectionErrorModal from "../components/connections/ConnectionErrorModal.svelte";
  import ConnectorWizard from "../components/integrations/ConnectorWizard.svelte";
  import McpDisclaimerDialog, {
    isDisclaimerAccepted,
  } from "../components/integrations/McpDisclaimerDialog.svelte";
  import {
    listMcpServers,
    listMcpEnrichments,
    listAgents,
    fetchMcpCurated,
    fetchMcpRegistry,
    oauthGetStatus,
    oauthDisconnect,
    restartMcpServer,
    type OauthAccountInfo,
    type ProviderId,
  } from "$lib/ipc/connections";
  import { messageOf, formatTauriError } from "$lib/connections/errors";
  import { sortServers, needsOauthSetup } from "$lib/connections/status";
  import { oauthListClientIds, type OauthClientIdStatus } from "$lib/ipc/oauthClients";
  import { navigateToSettings } from "$lib/router";
  import type {
    AgentListItem,
    ConnectorEnrichmentView,
    McpHealth,
    McpServerStatusView,
    RegistryServerView,
  } from "$lib/types";
  import type {
    Selection,
    NativeConnectorCard,
    NativeTab,
    McpTab,
    McpRowAction,
  } from "../components/connections/shared/types";

  interface Props {
    onNavigateTasks?: () => void;
  }
  let { onNavigateTasks }: Props = $props();

  // ── Top-level data ─────────────────────────────────────────────────────────
  let servers = $state<McpServerStatusView[]>([]);
  let enrichmentMap = $state(new Map<string, ConnectorEnrichmentView>());
  let registry = $state<RegistryServerView[]>([]);
  let agents = $state<AgentListItem[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);

  // ── Selection + tabs ────────────────────────────────────────────────────────
  let selection = $state<Selection>(null);
  let nativeTab = $state<NativeTab>("accounts");
  let mcpTab = $state<McpTab>("overview");

  // ── Native connectors ────────────────────────────────────────────────────────
  let nativeAccounts = $state<OauthAccountInfo[]>([]);
  let nativeLoading = $state(false);
  let nativeError = $state<string | null>(null);
  let oauthOpen = $state(false);
  let oauthProvider = $state<ProviderId | null>(null);
  let oauthClients = $state<OauthClientIdStatus[]>([]);

  const NATIVE_CONNECTORS: NativeConnectorCard[] = $derived([
    { id: "google", name: "Google Workspace", description: $t("connections.native_google_description") },
    { id: "microsoft", name: "Microsoft 365", description: $t("connections.native_microsoft_description") },
  ]);

  function accountsForProvider(provider: ProviderId): OauthAccountInfo[] {
    return nativeAccounts.filter((a) => a.provider === provider);
  }

  /**
   * Whether a native connector still lacks usable OAuth credentials.
   *
   * No published Apollia build embeds a Google or Microsoft client, so this is
   * true on a fresh install. Reading it here, rather than letting the connect
   * dialog discover it, is what lets the sidebar and the detail header say so
   * before the operator commits to a browser round-trip.
   */
  function needsSetup(provider: ProviderId): boolean {
    return needsOauthSetup(oauthClients.find((c) => c.provider === provider));
  }

  function openCredentialSettings(): void {
    navigateToSettings("integrations");
  }

  // ── Catalogue + wizard + error modal ─────────────────────────────────────────
  let catalogueOpen = $state(false);
  let registryLoading = $state(false);
  let catalogueTier = $state<"none" | "curated" | "full">("none");
  let selectedRegistryServer = $state<RegistryServerView | null>(null);
  let wizardOpen = $state(false);
  let disclaimerOpen = $state(false);
  let errorModalOpen = $state(false);
  let errorServer = $state<McpServerStatusView | null>(null);

  // ── Derived ──────────────────────────────────────────────────────────────────
  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  const installedNames = $derived(new Set(servers.map((s) => s.name)));
  const installedByPackage = $derived.by(() => {
    const map = new Map<string, string>();
    for (const s of servers) if (s.package) map.set(s.package, s.name);
    return map;
  });

  const selectedServer = $derived.by(() => {
    const sel = selection;
    if (sel?.kind !== "mcp") return null;
    return servers.find((s) => s.name === sel.name) ?? null;
  });
  const selectedNativeConnector = $derived.by(() => {
    const sel = selection;
    if (sel?.kind !== "native") return null;
    return NATIVE_CONNECTORS.find((c) => c.id === sel.provider) ?? null;
  });

  // ── Loaders ──────────────────────────────────────────────────────────────────
  async function loadAll(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [serverList, enrichmentEntries, agentList] = await Promise.all([
        listMcpServers(),
        listMcpEnrichments(),
        listAgents().catch(() => [] as AgentListItem[]),
      ]);
      servers = serverList;
      enrichmentMap = new Map(enrichmentEntries.map((e) => [e.package_identifier, e.enrichment]));
      agents = agentList;
    } catch (err: unknown) {
      loadError = messageOf(err);
    } finally {
      loading = false;
    }
  }

  async function loadCuratedCatalogue(): Promise<void> {
    if (catalogueTier !== "none") return;
    registryLoading = true;
    try {
      registry = await fetchMcpCurated().catch(() => [] as RegistryServerView[]);
      catalogueTier = "curated";
    } finally {
      registryLoading = false;
    }
  }

  async function loadFullRegistry(): Promise<void> {
    if (catalogueTier === "full" || registryLoading) return;
    registryLoading = true;
    try {
      registry = await fetchMcpRegistry().catch(() => [] as RegistryServerView[]);
      catalogueTier = "full";
    } finally {
      registryLoading = false;
    }
  }

  async function refreshNativeStatus(): Promise<void> {
    nativeLoading = true;
    nativeError = null;
    try {
      // Credential status is fetched alongside the accounts: an unconfigured
      // provider has no accounts and would otherwise look merely idle.
      const [accounts, clients] = await Promise.all([
        oauthGetStatus(),
        oauthListClientIds().catch(() => [] as OauthClientIdStatus[]),
      ]);
      nativeAccounts = accounts;
      oauthClients = clients;
    } catch (e) {
      nativeError = formatTauriError(e, $t);
    } finally {
      nativeLoading = false;
    }
  }

  // ── Handlers ─────────────────────────────────────────────────────────────────
  function handleOpenCatalogue(): void {
    catalogueOpen = true;
    void (async () => {
      await loadCuratedCatalogue();
      if (catalogueTier === "curated") void loadFullRegistry();
    })();
  }

  function handleConnect(server: RegistryServerView): void {
    catalogueOpen = false;
    selectedRegistryServer = server;
    if (!isDisclaimerAccepted()) {
      disclaimerOpen = true;
      return;
    }
    wizardOpen = true;
  }

  function handleManage(name: string): void {
    catalogueOpen = false;
    selection = { kind: "mcp", name };
  }

  function handleContext(name: string, action: McpRowAction): void {
    selection = { kind: "mcp", name };
    mcpTab = action === "test" ? "overview" : "settings";
  }

  function startNativeConnect(provider: ProviderId): void {
    oauthProvider = provider;
    oauthOpen = true;
  }

  async function disconnectNative(account: OauthAccountInfo): Promise<void> {
    if (
      !confirm(
        $t("connections.disconnect_native_confirm", {
          values: { account: account.account_id, provider: account.provider },
        }),
      )
    ) {
      return;
    }
    try {
      await oauthDisconnect(account.provider, account.account_id);
      await refreshNativeStatus();
    } catch (e) {
      nativeError = formatTauriError(e, $t);
    }
  }

  async function handleRetry(name: string): Promise<void> {
    errorModalOpen = false;
    try {
      await restartMcpServer(name);
    } catch {
      /* surfaced via reload */
    }
    await loadAll();
  }

  function handleViewLogs(name: string): void {
    errorModalOpen = false;
    handleManage(name);
  }

  // ── Effects ──────────────────────────────────────────────────────────────────
  $effect(() => {
    void loadAll();
    void refreshNativeStatus();
  });

  // Auto-select the first connector; recover when the target disappears.
  $effect(() => {
    if (loading) return;
    const cur = selection;
    if (cur?.kind === "mcp" && !servers.some((s) => s.name === cur.name)) {
      selection = null;
    }
    if (selection !== null) return;
    if (servers.length > 0) {
      selection = { kind: "mcp", name: sortServers(servers)[0]?.name ?? servers[0].name };
    } else {
      selection = { kind: "native", provider: "google" };
    }
  });

  // Reset tabs on selection kind change.
  $effect(() => {
    if (selection?.kind === "native") nativeTab = "accounts";
  });

  // Live health: reflect McpServerHealthChanged on the badge without a refresh.
  $effect(() => {
    type Envelope = { event_type: string; payload: Record<string, unknown> };
    let unlisten: UnlistenFn | null = null;
    let disposed = false;
    void listen<Envelope>("runtime-event", (event) => {
      if (event.payload.event_type !== "McpServerHealthChanged") return;
      const inner = event.payload.payload?.McpServerHealthChanged as
        | { name: string; health: McpHealth }
        | undefined;
      if (!inner) return;
      servers = servers.map((s) => (s.name === inner.name ? { ...s, health: inner.health } : s));
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  $effect(() => {
    void onNavigateTasks;
  });
</script>

<div class="flex h-full min-h-0 w-full flex-col" data-testid="connections-route">
  <SplitLayout sidebarTestid="connectors-sidebar" detailTestid="connector-detail">
    {#snippet sidebar()}
      <ConnectorSidebar
        nativeConnectors={NATIVE_CONNECTORS}
        accountCountFor={(id) => accountsForProvider(id).length}
        needsSetupFor={needsSetup}
        {servers}
        {enrichmentMap}
        {selection}
        {loading}
        {loadError}
        onselect={(sel) => (selection = sel)}
        onopencatalogue={handleOpenCatalogue}
        oncontext={handleContext}
      />
    {/snippet}

    {#if selection?.kind === "native" && selectedNativeConnector}
      <NativeConnectorDetail
        connector={selectedNativeConnector}
        accounts={accountsForProvider(selectedNativeConnector.id)}
        loading={nativeLoading}
        error={nativeError}
        needsSetup={needsSetup(selectedNativeConnector.id)}
        tab={nativeTab}
        onconnect={() => startNativeConnect(selectedNativeConnector.id)}
        onconfigure={openCredentialSettings}
        ondisconnect={disconnectNative}
        ontabchange={(tab) => (nativeTab = tab)}
      />
    {:else if selection?.kind === "mcp" && selectedServer}
      <McpServerDetail
        server={selectedServer}
        enrichment={resolveEnrichment(selectedServer)}
        tab={mcpTab}
        ontabchange={(tab) => (mcpTab = tab)}
        onchanged={() => void loadAll()}
        ondisconnected={() => {
          selection = null;
          void loadAll();
        }}
      />
    {:else}
      <div class="flex flex-1 items-center justify-center text-body-sm text-muted-foreground">
        {$t("connections.select_connector_prompt")}
      </div>
    {/if}
  </SplitLayout>
</div>

{#if selectedRegistryServer}
  <ConnectorWizard
    server={selectedRegistryServer}
    open={wizardOpen}
    onclose={() => {
      wizardOpen = false;
      selectedRegistryServer = null;
    }}
    oncomplete={() => {
      wizardOpen = false;
      selectedRegistryServer = null;
      catalogueOpen = false;
      void loadAll();
    }}
  />
{/if}

<McpDisclaimerDialog
  open={disclaimerOpen}
  onaccept={() => {
    disclaimerOpen = false;
    if (selectedRegistryServer) wizardOpen = true;
  }}
  onclose={() => {
    disclaimerOpen = false;
    selectedRegistryServer = null;
  }}
/>

<ConnectionErrorModal
  open={errorModalOpen}
  server={errorServer}
  enrichment={errorServer ? resolveEnrichment(errorServer) : null}
  onclose={() => (errorModalOpen = false)}
  onretry={handleRetry}
  onviewLogs={handleViewLogs}
/>

<NativeOauthDialog
  open={oauthOpen}
  provider={oauthProvider}
  onclose={() => (oauthOpen = false)}
  oncompleted={refreshNativeStatus}
/>

<CatalogueSheet
  open={catalogueOpen}
  {registry}
  {agents}
  {installedNames}
  {installedByPackage}
  {registryLoading}
  {catalogueTier}
  onclose={() => (catalogueOpen = false)}
  onconnect={handleConnect}
  onmanage={handleManage}
  oninstalled={() => {
    catalogueOpen = false;
    void loadAll();
  }}
/>
