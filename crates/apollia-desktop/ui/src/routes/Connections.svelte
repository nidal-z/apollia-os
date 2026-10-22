<script lang="ts">
  /**
   * Connections - every connection of this machine, in one list.
   *
   * Thin orchestrator on the shared browse layout: the list column (search,
   * kind chips, one row per connection) and the card of the chosen connection
   * beside it. It owns the top-level data (the connections, installed
   * servers, enrichments, registry, native accounts, the count of built-in
   * tools), each source read on its own so one that fails keeps the others;
   * the place of the screen (card, search, chip) lives in the
   * `connectionsView` store and outlives the screen. Business logic lives in
   * `$lib/connections/*` (pure, testable) and inside the self-contained
   * detail / catalogue / oauth components.
   */
  import { untrack } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { X } from "lucide-svelte";
  import InactiveServerDetail from "../components/connections/detail/InactiveServerDetail.svelte";
  import { BrowseLayout } from "$lib/components/operator";
  import ConnectionListHeader from "../components/connections/list/ConnectionListHeader.svelte";
  import ConnectionList from "../components/connections/list/ConnectionList.svelte";
  import NativeConnectorDetail from "../components/connections/detail/NativeConnectorDetail.svelte";
  import McpServerDetail from "../components/connections/detail/McpServerDetail.svelte";
  import ConnectionSummaryDetail from "../components/connections/detail/ConnectionSummaryDetail.svelte";
  import BuiltinToolsDetail from "../components/connections/detail/BuiltinToolsDetail.svelte";
  import { tools as builtinTools, loadTools as loadBuiltinTools } from "$lib/stores/toolGovernance";
  import AddConnectionWindow from "../components/connections/catalogue/AddConnectionWindow.svelte";
  import NativeOauthDialog from "../components/connections/oauth/NativeOauthDialog.svelte";
  import ConnectorWizard from "../components/integrations/ConnectorWizard.svelte";
  import MailboxDialog from "../components/integrations/MailboxDialog.svelte";
  import FolderConnectionDialog from "../components/connections/catalogue/FolderConnectionDialog.svelte";
  import { chooseAndAddFolder, type FolderOutcome } from "$lib/connections/folderConnection";
  import {
    listConnections,
    listMcpServers,
    listMcpEnrichments,
    listAgents,
    oauthGetStatus,
    oauthDisconnect,
    type OauthAccountInfo,
    type ProviderId,
  } from "$lib/ipc/connections";
  import { formatTauriError } from "$lib/connections/errors";
  import { needsOauthSetup } from "$lib/connections/status";
  import { loadConnections } from "$lib/connections/connectionList";
  import {
    EMPTY_PAGE,
    applyStateChange,
    loadPageSources,
    type SourceFailure,
    type StateChange,
  } from "$lib/connections/pageSources";
  import {
    createCatalogueSource,
    type CatalogueFailure,
    type CatalogueTier,
    type CatalogueView,
  } from "$lib/connections/catalogueSource";
  import {
    ADD_WINDOW_CLOSED,
    backToSearch,
    flowCancelled,
    flowDone,
    leaveForFlow,
    openAddWindow,
    type AddEntry,
  } from "$lib/connections/addWindow";
  import {
    NATIVE_PROVIDER_KEYS,
    connectionRows,
    nativeProviderOf,
    serverNameOf,
  } from "$lib/connections/connectionRows";
  import { connectionsView } from "$lib/stores/connectionsView";
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
  import { enrichmentLabel } from "$lib/connections/catalogueText";
  import {
    tryConnectorPrompt,
    tryQuestions,
    type TryQuestion,
  } from "$lib/connections/tryConnectorPrompt";
  import TryConnectionDialog from "../components/connections/shared/TryConnectionDialog.svelte";
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
  let publicEntries = $state<RegistryServerView[]>([]);
  let catalogueFailures = $state<CatalogueFailure[]>([]);
  let agents = $state<AgentListItem[]>([]);
  let loading = $state(true);
  let failures = $state<SourceFailure[]>([]);

  // ── Selection + tabs ────────────────────────────────────────────────────────
  // The place of the screen is kept in a store, so coming back finds it.
  const selection: Selection = $derived($connectionsView.selection);
  const select = (next: Selection): void => connectionsView.select(next);
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

  // ── Add window + wizard ──────────────────────────────────────────────────────
  // The window keeps its search while a flow it opened stands in its place, so
  // cancelling that flow gives the search back rather than the list.
  let addWindow = $state(ADD_WINDOW_CLOSED);
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

  // ── Try ─────────────────────────────────────────────────────────────────────
  let tryTarget = $state<{ connectionId: string; name: string } | null>(null);
  let tryLaunching = $state(false);

  // ── Derived ──────────────────────────────────────────────────────────────────
  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  /** The rows of the list, one per connection, a server named by its catalogue label. */
  const rows = $derived(
    connectionRows(connections, (c) => {
      if (c.kind !== "mcp") return c.label;
      const server = servers.find((s) => `mcp:${s.name}` === c.id);
      return (server && enrichmentLabel(resolveEnrichment(server), $locale)) ?? c.label;
    }),
  );
  const selectedConnection = $derived.by(() => {
    const sel = selection;
    if (sel?.kind !== "connection") return null;
    return connections.find((c) => c.id === sel.id) ?? null;
  });
  /** The name the list gives the chosen connection. */
  const selectedLabel = $derived(rows.find((r) => r.key === selectedConnection?.id)?.label ?? selectedConnection?.label ?? "");

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

  /** The server of the chosen service, by its name; null for another kind. */
  const selectedServerName = $derived(selectedConnection ? serverNameOf(selectedConnection.id) : null);
  const selectedServer = $derived(servers.find((s) => s.name === selectedServerName) ?? null);
  /** The first account connection of a native provider, the one its page tries. */
  function nativeConnectionOf(provider: ProviderId): Connection | null {
    return connections.find((c) => c.kind === "native" && c.id.startsWith(`native:${provider}:`)) ?? null;
  }

  /** "Try" on a native provider's page: its first account, or nothing without one. */
  function nativeTry(card: NativeConnectorCard): (() => void) | undefined {
    const connection = nativeConnectionOf(card.id);
    return connection ? () => openTry(connection.id, card.name) : undefined;
  }

  /** The questions offered to try the connection `tryTarget` names. */
  const tryQuestionList = $derived.by((): TryQuestion[] => {
    const target = tryTarget;
    if (!target) return [];
    const general = {
      label: $t("connections.try.general_label"),
      question: $t("connections.try.general_question", { values: { name: target.name } }),
    };
    const connection = connections.find((c) => c.id === target.connectionId);
    if (!connection) return [{ id: "general", ...general, general: true }];
    const server =
      connection.kind === "mcp" ? servers.find((s) => `mcp:${s.name}` === connection.id) : undefined;
    const examples = (server && resolveEnrichment(server)?.examples) ?? [];
    return tryQuestions(connection, examples, $locale ?? "en", general);
  });

  /**
   * The provider page the card shows: the provider of the chosen account, or
   * a provider opened with no account yet to connect its first one.
   */
  const selectedNativeConnector = $derived.by(() => {
    const sel = selection;
    if (sel?.kind === "native") return nativeCard(sel.provider);
    const provider = selectedConnection ? nativeProviderOf(selectedConnection) : null;
    return provider ? nativeCard(provider) : null;
  });

  // ── Loaders ──────────────────────────────────────────────────────────────────
  /** Read every source of the page; one that fails keeps what it showed and gets its banner. */
  async function loadAll(): Promise<void> {
    loading = true;
    const loaded = await loadPageSources(
      {
        connections: listConnections,
        servers: listMcpServers,
        enrichments: listMcpEnrichments,
        agents: listAgents,
      },
      // Read without tracking: the effect that loads on mount must not run
      // again each time a load replaces what it read.
      untrack(() => ({ ...EMPTY_PAGE, connections, servers, enrichments: enrichmentMap, agents })),
    );
    connections = loaded.data.connections;
    servers = loaded.data.servers;
    enrichmentMap = loaded.data.enrichments;
    agents = loaded.data.agents;
    failures = loaded.failures;
    loading = false;
  }

  function showView(view: CatalogueView): void {
    catalogueTier = view.tier;
    registry = view.selection;
    publicEntries = view.publicEntries;
    catalogueFailures = view.failures;
  }

  async function showSelection(): Promise<void> {
    registryLoading = true;
    try {
      showView(await catalogueSource.selection());
    } finally {
      registryLoading = false;
    }
  }

  async function handleExploreAll(): Promise<void> {
    if (registryLoading) return;
    registryLoading = true;
    try {
      showView(await catalogueSource.exploreAll());
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
    addWindow = openAddWindow();
    void showSelection();
  }

  /** Add on a catalogue card opens the add dialog on its warning, in place of the window. */
  function handleConnect(server: RegistryServerView): void {
    addWindow = leaveForFlow(addWindow);
    selectedRegistryServer = server;
    wizardOpen = true;
  }

  /** Add on the card of `entry`: the flow of its kind. */
  function addEntry(entry: AddEntry): void {
    switch (entry.kind) {
      case "catalogue":
        handleConnect(entry.entry);
        return;
      case "native":
        addWindow = flowDone();
        select({ kind: "native", provider: entry.provider });
        return;
      case "mailbox":
        addWindow = leaveForFlow(addWindow);
        mailboxOpen = true;
        return;
      case "folder":
        void addFolder();
        return;
      case "address":
        return;
    }
  }

  /**
   * Open the add dialog on the selection entry another screen asked for; an
   * entry the selection does not hold opens the catalogue instead.
   */
  async function openPendingEntry(entryId: string): Promise<void> {
    await showSelection();
    const entry = findSelectionEntry(registry, entryId);
    if (entry) handleConnect(entry);
    else addWindow = openAddWindow();
  }

  /**
   * "Folder" in the selection, or "Choose another folder" after one: the
   * system picker, then the folder added. A cancelled picker changes nothing.
   */
  async function addFolder(): Promise<void> {
    addWindow = leaveForFlow(addWindow);
    const outcome = await chooseAndAddFolder($t("connections.folder.picker_title"));
    if (outcome.kind === "cancelled") {
      addWindow = flowCancelled(addWindow);
      return;
    }
    addWindow = flowDone();
    folderOutcome = outcome;
    folderDialogOpen = true;
    if (outcome.kind === "added") {
      await loadAll();
      select({ kind: "connection", id: outcome.connection.id });
    }
  }

  function closeFolderDialog(): void {
    folderDialogOpen = false;
    folderOutcome = null;
  }

  function handleManage(name: string): void {
    addWindow = flowDone();
    select({ kind: "connection", id: `mcp:${name}` });
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

  // A card whose connection disappeared gives the list back, once the list is
  // read. A native provider stays selectable without an account: its page is
  // where its client is entered and its first account connected.
  $effect(() => {
    if (loading || failures.some((f) => f.source === "connections")) return;
    if (selection?.kind === "connection" && selectedConnection === null) select(null);
  });

  // Reset tabs on selection kind change.
  $effect(() => {
    if (selectedNativeConnector !== null) nativeTab = "accounts";
  });

  // Live state: a connection found in a new state, and the health of a server
  // on its card, change without a refresh.
  $effect(() => {
    type Envelope = { event_type: string; payload: Record<string, unknown> };
    let unlisten: UnlistenFn | null = null;
    let disposed = false;
    void listen<Envelope>("runtime-event", (event) => {
      const inner = event.payload.payload?.[event.payload.event_type];
      if (event.payload.event_type === "ConnectionStateChanged" && inner) {
        connections = applyStateChange(connections, inner as StateChange);
        return;
      }
      if (event.payload.event_type !== "McpServerHealthChanged" || !inner) return;
      const health = inner as { name: string; health: McpHealth };
      servers = servers.map((s) => (s.name === health.name ? { ...s, health: health.health } : s));
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
  /** Open the "Try" dialog on `connectionId`, shown as `name`. */
  function openTry(connectionId: string, name: string): void {
    tryTarget = { connectionId, name };
  }

  /**
   * Try `prompt` on `connectionId`: a conversation with Apollia limited to the
   * tools of that connection, then the chat. The dialogs that led here close
   * once the conversation exists.
   */
  async function launchTry(connectionId: string, prompt: string): Promise<void> {
    tryLaunching = true;
    const opened = await tryConnectorPrompt(connectionId, prompt, {
      loadConnections: () => loadConnections(),
      createSession: createChatSession,
      sendMessage: sendChatMessage,
      nameSession: triggerAutoName,
      rememberSession: (sessionId) => pendingChatSessionId.set(sessionId),
      report: (err) => {
        reportError(err, { surface: "toast", testid: "connector-try-error" });
      },
      navigate: () => navigateTo("chat"),
      noToolsMessage: $t("connections.try.no_tools"),
      notFoundMessage: $t("connections.try.not_found"),
    });
    tryLaunching = false;
    if (!opened) return;
    tryTarget = null;
    wizardOpen = false;
    selectedRegistryServer = null;
    addWindow = flowDone();
    void loadAll();
  }

  /** "Try" on a coaching card at the end of a server's addition. */
  function tryAddedServer(prompt: string): void {
    const server = selectedRegistryServer;
    if (server) void launchTry(`mcp:${deriveServerName(server)}`, prompt);
  }
</script>

<div class="flex h-full min-h-0 w-full flex-col" data-testid="connections-route">
  <BrowseLayout
    detailOpen={selection !== null}
    data-testid="connectors-sidebar"
    listTestid="connections-sidebar-list"
  >
    {#snippet header()}
      <ConnectionListHeader
        {rows}
        query={$connectionsView.query}
        kind={$connectionsView.kind}
        {failures}
        onsearch={(query) => connectionsView.search(query)}
        onkind={(kind) => connectionsView.filter(kind)}
        onadd={handleOpenCatalogue}
        onretry={() => void loadAll()}
      />
    {/snippet}

    <ConnectionList
      {rows}
      query={$connectionsView.query}
      kind={$connectionsView.kind}
      {selection}
      {loading}
      builtinTools={builtinCount}
      onselect={select}
      onadd={handleOpenCatalogue}
    />

    {#snippet detail()}
      {#if selection !== null}
        <section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background" data-testid="connector-detail">
          <div class="flex shrink-0 justify-end px-4 pt-2">
            <button
              type="button"
              class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              onclick={() => select(null)}
              aria-label={$t("connections.list.close_card")}
              title={$t("connections.list.close_card")}
              data-testid="connections-back"
            >
              <X size={15} aria-hidden="true" />
            </button>
          </div>
          {#if selectedNativeConnector}
            <NativeConnectorDetail
              connector={selectedNativeConnector}
              remote={selectedConnection?.remote ?? false}
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
              ontry={nativeTry(selectedNativeConnector)}
            />
          {:else if selectedConnection && selectedServer}
            <McpServerDetail
              server={selectedServer}
              connection={selectedConnection}
              enrichment={resolveEnrichment(selectedServer)}
              remote={selectedConnection.remote}
              tab={mcpTab}
              ontabchange={(tab) => (mcpTab = tab)}
              ontry={(name) => openTry(`mcp:${selectedServer.name}`, name)}
              onchanged={() => void loadAll()}
              ondisconnected={() => {
                select(null);
                void loadAll();
              }}
            />
          {:else if selectedConnection && selectedServerName !== null}
            <!-- A server the runtime does not run keeps a page: it is started
                 again or removed from there. -->
            <InactiveServerDetail
              name={selectedServerName}
              label={selectedLabel}
              onchanged={() => void loadAll()}
              onremoved={() => {
                select(null);
                void loadAll();
              }}
            />
          {:else if selection?.kind === "builtin"}
            <BuiltinToolsDetail />
          {:else if selectedConnection}
            <ConnectionSummaryDetail
              connection={selectedConnection}
              label={selectedLabel}
              ontry={() => openTry(selectedConnection.id, selectedLabel)}
              onremoved={() => {
                select(null);
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
        </section>
      {/if}
    {/snippet}
  </BrowseLayout>
</div>

{#if selectedRegistryServer}
  <ConnectorWizard
    server={selectedRegistryServer}
    open={wizardOpen}
    onclose={() => {
      wizardOpen = false;
      selectedRegistryServer = null;
      addWindow = flowCancelled(addWindow);
    }}
    oncomplete={() => {
      wizardOpen = false;
      selectedRegistryServer = null;
      addWindow = flowDone();
      void loadAll();
    }}
    ontryprompt={tryAddedServer}
  />
{/if}

<MailboxDialog
  open={mailboxOpen}
  onclose={() => {
    mailboxOpen = false;
    addWindow = flowCancelled(addWindow);
  }}
  onadded={(connectionId) => {
    addWindow = flowDone();
    select({ kind: "connection", id: connectionId });
    void loadAll();
  }}
/>

<FolderConnectionDialog
  open={folderDialogOpen}
  outcome={folderOutcome}
  onclose={closeFolderDialog}
  ontry={(connection) => {
    closeFolderDialog();
    openTry(connection.id, connection.label);
  }}
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
  oncompleted={() => {
    void refreshNativeStatus();
    void loadAll();
  }}
  ontry={(provider, accountId) => {
    oauthOpen = false;
    openTry(`native:${provider}:${accountId}`, nativeCard(provider).name);
  }}
  onopenclient={(provider) => openClientSection(provider)}
/>

<AddConnectionWindow
  open={addWindow.open}
  bind:query={addWindow.query}
  chosen={addWindow.chosen}
  selection={registry}
  {publicEntries}
  failures={catalogueFailures}
  {catalogueTier}
  loading={registryLoading}
  {agents}
  {installedNames}
  {installedByPackage}
  onclose={() => (addWindow = ADD_WINDOW_CLOSED)}
  onchoose={(key) => (addWindow = { ...addWindow, chosen: key })}
  onback={() => (addWindow = backToSearch(addWindow))}
  onexploreall={handleExploreAll}
  onbacktoselection={showSelection}
  onadd={addEntry}
  onmanage={handleManage}
  oninstalled={() => {
    addWindow = flowDone();
    void loadAll();
  }}
/>

<TryConnectionDialog
  open={tryTarget !== null}
  name={tryTarget?.name ?? ""}
  questions={tryQuestionList}
  launching={tryLaunching}
  onclose={() => (tryTarget = null)}
  onlaunch={(prompt) => {
    if (tryTarget) void launchTry(tryTarget.connectionId, prompt);
  }}
/>
