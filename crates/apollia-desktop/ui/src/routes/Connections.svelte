<script lang="ts">
  /**
   * Connections - every connection of this machine, in one list.
   *
   * Thin orchestrator: owns the top-level data (the connection list served by
   * the loader, installed servers, enrichments, registry, native accounts, the
   * count of built-in tools) and
   * selection, wires IPC via
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
  import ConnectionSummaryDetail from "../components/connections/detail/ConnectionSummaryDetail.svelte";
  import BuiltinToolsDetail from "../components/connections/detail/BuiltinToolsDetail.svelte";
  import { tools as builtinTools, loadTools as loadBuiltinTools } from "$lib/stores/toolGovernance";
  import CatalogueSheet from "../components/connections/catalogue/CatalogueSheet.svelte";
  import NativeOauthDialog from "../components/connections/oauth/NativeOauthDialog.svelte";
  import ConnectorWizard from "../components/integrations/ConnectorWizard.svelte";
  import MailboxDialog from "../components/integrations/MailboxDialog.svelte";
  import FolderConnectionDialog from "../components/connections/catalogue/FolderConnectionDialog.svelte";
  import { chooseAndAddFolder, type FolderOutcome } from "$lib/connections/folderConnection";
  import {
    listMcpServers,
    listMcpEnrichments,
    listAgents,
    getMcpServerDetail,
    oauthGetStatus,
    oauthDisconnect,
    type OauthAccountInfo,
    type ProviderId,
  } from "$lib/ipc/connections";
  import { messageOf, formatTauriError } from "$lib/connections/errors";
  import { needsOauthSetup } from "$lib/connections/status";
  import { loadConnections, forbiddenConnections } from "$lib/connections/connectionList";
  import { createCatalogueSource, type CatalogueTier } from "$lib/connections/catalogueSource";
  import {
    NATIVE_PROVIDER_KEYS,
    connectionRows,
    isSelected,
  } from "$lib/connections/connectionRows";
  import { oauthListClientIds, type OauthClientIdStatus } from "$lib/ipc/oauthClients";
  import { navigateTo } from "$lib/stores/navigation";
  import {
    findSelectionEntry,
    pendingConnectionAdd,
    takePendingConnectionAdd,
  } from "$lib/stores/connectionAdd";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { createChatSession } from "$lib/ipc/agents";
  import { sendChatMessage } from "$lib/ipc/chat";
  import { triggerAutoName } from "$lib/chat/autoName";
  import { reportError } from "$lib/errors/reportError";
  import { deriveServerName } from "$lib/integrations/connectorConfig";
  import { tryConnectorPrompt } from "$lib/connections/tryConnectorPrompt";
  import type {
    AgentListItem,
    Connection,
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
  let connections = $state<Connection[]>([]);
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
  let oauthClientsError = $state<string | null>(null);

  /** Name and description of a native provider's page. */
  function nativeCard(provider: ProviderId): NativeConnectorCard {
    return {
      id: provider,
      name: $t(NATIVE_PROVIDER_KEYS[provider].name),
      description: $t(NATIVE_PROVIDER_KEYS[provider].description),
    };
  }

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

  /**
   * Bring the OAuth client of the selected native connection into view: it
   * sits on the connection's page, under its accounts.
   */
  function openClientSection(provider: ProviderId): void {
    nativeTab = "accounts";
    queueMicrotask(() => {
      document
        .getElementById(`oauth-client-${provider}`)
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }

  // ── Catalogue + wizard ───────────────────────────────────────────────────────
  let catalogueOpen = $state(false);
  let registryLoading = $state(false);
  let catalogueTier = $state<CatalogueTier>("none");
  // Opening reads the selection only; the public catalogue is asked by
  // "Explore all" and nothing else.
  const catalogueSource = createCatalogueSource();
  let selectedRegistryServer = $state<RegistryServerView | null>(null);
  let wizardOpen = $state(false);
  let mailboxOpen = $state(false);

  // ── Folder connections ───────────────────────────────────────────────────────
  let folderOutcome = $state<Exclude<FolderOutcome, { kind: "cancelled" }> | null>(null);
  let folderDialogOpen = $state(false);

  // ── Derived ──────────────────────────────────────────────────────────────────
  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  /** The connections the organisation policy forbade at the last start, listed apart. */
  const forbidden = $derived(forbiddenConnections(connections));
  /** The rows of the list: the loader's connections, a server named by its catalogue label. */
  const rows = $derived(
    connectionRows(connections.filter((c) => c.state !== "forbidden_by_policy"), (c) => {
      if (c.kind !== "mcp") return c.label;
      const server = servers.find((s) => `mcp:${s.name}` === c.id);
      return (server && resolveEnrichment(server)?.operator_label) ?? c.label;
    }),
  );
  const selectedRow = $derived(
    rows.find((r) => isSelected(r, selection)) ?? null,
  );
  const selectedConnection = $derived.by(() => {
    const sel = selection;
    if (sel?.kind !== "connection") return null;
    return connections.find((c) => c.id === sel.id) ?? null;
  });

  const builtinCount = $derived({
    active: $builtinTools.filter((tool) => tool.enabled).length,
    total: $builtinTools.length,
  });

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
    return nativeCard(sel.provider);
  });

  // ── Loaders ──────────────────────────────────────────────────────────────────
  async function loadAll(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [listState, serverList, enrichmentEntries, agentList] = await Promise.all([
        loadConnections(),
        listMcpServers(),
        listMcpEnrichments(),
        listAgents().catch(() => [] as AgentListItem[]),
      ]);
      if (listState.status === "error") loadError = listState.message;
      else if (listState.status === "ready") connections = listState.connections;
      servers = serverList;
      enrichmentMap = new Map(enrichmentEntries.map((e) => [e.package_identifier, e.enrichment]));
      agents = agentList;
    } catch (err: unknown) {
      loadError = messageOf(err);
    } finally {
      loading = false;
    }
  }

  async function showSelection(): Promise<void> {
    registryLoading = true;
    try {
      const view = await catalogueSource.selection();
      catalogueTier = view.tier;
      registry = view.entries;
    } finally {
      registryLoading = false;
    }
  }

  async function handleExploreAll(): Promise<void> {
    if (registryLoading) return;
    registryLoading = true;
    try {
      const view = await catalogueSource.exploreAll();
      catalogueTier = view.tier;
      registry = view.entries;
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
      oauthClientsError = null;
      const [accounts, clients] = await Promise.all([
        oauthGetStatus(),
        oauthListClientIds().catch((e: unknown) => {
          oauthClientsError = formatTauriError(e, $t);
          return [] as OauthClientIdStatus[];
        }),
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
    void showSelection();
  }

  /** Choosing an entry opens the add dialog on its warning. */
  function handleConnect(server: RegistryServerView): void {
    catalogueOpen = false;
    selectedRegistryServer = server;
    wizardOpen = true;
  }

  /**
   * Open the add dialog on the selection entry another screen asked for; an
   * entry the selection does not hold opens the catalogue instead.
   */
  async function openPendingEntry(entryId: string): Promise<void> {
    await showSelection();
    const entry = findSelectionEntry(registry, entryId);
    if (entry) handleConnect(entry);
    else catalogueOpen = true;
  }

  /**
   * "Folder" in the selection, or "Choose another folder" after one: the
   * system picker, then the folder added. A cancelled picker changes nothing.
   */
  async function addFolder(): Promise<void> {
    catalogueOpen = false;
    const outcome = await chooseAndAddFolder($t("connections.folder.picker_title"));
    if (outcome.kind === "cancelled") return;
    folderOutcome = outcome;
    folderDialogOpen = true;
    if (outcome.kind === "added") {
      await loadAll();
      selection = { kind: "connection", id: outcome.connection.id };
    }
  }

  function closeFolderDialog(): void {
    folderDialogOpen = false;
    folderOutcome = null;
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

  // ── Effects ──────────────────────────────────────────────────────────────────
  $effect(() => {
    if ($pendingConnectionAdd === null) return;
    const entryId = takePendingConnectionAdd();
    if (entryId !== null) void openPendingEntry(entryId);
  });

  $effect(() => {
    void loadAll();
    void refreshNativeStatus();
    void loadBuiltinTools();
  });

  // Auto-select the first connection; recover when the target disappears. A
  // native provider stays selectable without an account: its page is where its
  // client is entered and its first account connected.
  $effect(() => {
    if (loading) return;
    const cur = selection;
    if (cur !== null && cur.kind !== "native" && cur.kind !== "builtin" && selectedRow === null) {
      selection = null;
    }
    if (selection !== null) return;
    selection = rows[0]?.selection ?? { kind: "builtin" };
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
      void loadConnections().then((next) => {
        if (next.status === "ready") connections = next.connections;
      });
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
  /** "Try" on a coaching card: a chat limited to the tools of the connector just installed. */
  async function tryPrompt(prompt: string): Promise<void> {
    const server = selectedRegistryServer;
    if (!server) return;
    const name = deriveServerName(server);
    const opened = await tryConnectorPrompt(prompt, {
      loadToolNames: async () =>
        (await getMcpServerDetail(name)).tools.map((tool) => tool.full_name),
      createSession: (tools) => createChatSession({ mode: "libre", tools }),
      sendMessage: sendChatMessage,
      nameSession: triggerAutoName,
      rememberSession: (sessionId) => pendingChatSessionId.set(sessionId),
      report: (err) => {
        reportError(err, { surface: "toast", testid: "connector-try-error" });
      },
      navigate: () => navigateTo("chat"),
      noToolsMessage: $t("integrations.wizard.coaching.no_tools"),
    });
    if (!opened) return;
    wizardOpen = false;
    selectedRegistryServer = null;
    catalogueOpen = false;
    void loadAll();
  }
</script>

<div class="flex h-full min-h-0 w-full flex-col" data-testid="connections-route">
  <SplitLayout sidebarTestid="connectors-sidebar" detailTestid="connector-detail">
    {#snippet sidebar()}
      <ConnectorSidebar
        {rows}
        {forbidden}
        {selection}
        {loading}
        {loadError}
        onselect={(sel) => (selection = sel)}
        onopencatalogue={handleOpenCatalogue}
        oncontext={handleContext}
        builtinTools={builtinCount}
      />
    {/snippet}

    {#if selection?.kind === "native" && selectedNativeConnector}
      <NativeConnectorDetail
        connector={selectedNativeConnector}
        remote={selectedRow?.remote ?? false}
        accounts={accountsForProvider(selectedNativeConnector.id)}
        loading={nativeLoading}
        error={nativeError}
        needsSetup={needsSetup(selectedNativeConnector.id)}
        tab={nativeTab}
        clientStatus={oauthClients.find((c) => c.provider === selectedNativeConnector.id)}
        clientError={oauthClientsError}
        onclientretry={() => void refreshNativeStatus()}
        onconnect={() => startNativeConnect(selectedNativeConnector.id)}
        onconfigure={() => openClientSection(selectedNativeConnector.id)}
        ondisconnect={disconnectNative}
        ontabchange={(tab) => (nativeTab = tab)}
      />
    {:else if selection?.kind === "mcp" && selectedServer}
      <McpServerDetail
        server={selectedServer}
        enrichment={resolveEnrichment(selectedServer)}
        remote={selectedRow?.remote ?? false}
        tab={mcpTab}
        ontabchange={(tab) => (mcpTab = tab)}
        onchanged={() => void loadAll()}
        ondisconnected={() => {
          selection = null;
          void loadAll();
        }}
      />
    {:else if selection?.kind === "builtin"}
      <BuiltinToolsDetail />
    {:else if selection?.kind === "connection" && selectedConnection}
      <ConnectionSummaryDetail
        connection={selectedConnection}
        label={selectedRow?.label ?? selectedConnection.label}
        onremoved={() => {
          selection = null;
          void loadAll();
        }}
      />
    {:else}
      <div
        class="flex flex-1 items-center justify-center text-body-sm text-muted-foreground"
        data-testid="connections-select-prompt"
      >
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
    ontryprompt={(prompt) => void tryPrompt(prompt)}
  />
{/if}

<MailboxDialog
  open={mailboxOpen}
  onclose={() => (mailboxOpen = false)}
  onadded={(connectionId) => {
    selection = { kind: "connection", id: connectionId };
    void loadAll();
  }}
/>

<FolderConnectionDialog
  open={folderDialogOpen}
  outcome={folderOutcome}
  onclose={closeFolderDialog}
  onchooseanother={() => void addFolder()}
  onopenrule={() => {
    closeFolderDialog();
    navigateTo("authorizations");
  }}
/>

<NativeOauthDialog
  open={oauthOpen}
  provider={oauthProvider}
  onclose={() => (oauthOpen = false)}
  oncompleted={refreshNativeStatus}
  onopenclient={(provider) => openClientSection(provider)}
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
  onexploreall={handleExploreAll}
  onbacktoselection={showSelection}
  onconnect={handleConnect}
  onmanage={handleManage}
  onaddfolder={() => void addFolder()}
  onaddnative={(provider) => {
    catalogueOpen = false;
    selection = { kind: "native", provider };
  }}
  onaddmailbox={() => {
    catalogueOpen = false;
    mailboxOpen = true;
  }}
  oninstalled={() => {
    catalogueOpen = false;
    void loadAll();
  }}
/>
