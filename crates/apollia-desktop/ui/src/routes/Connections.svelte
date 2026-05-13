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
  let registryLoading = $state(false);
  let loadError = $state<string | null>(null);

  // Catalog pagination — show 48 cards initially, expand 48 at a time.
  const CATALOG_PAGE = 48;
  let catalogLimit = $state(CATALOG_PAGE);

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
  let customDialogOpen = $state(false);

  // ── Native Apollia connectors (Google Workspace, Microsoft 365) ─────────────
  type ProviderId = "google" | "microsoft";

  interface NativeConnectorCard {
    id: ProviderId;
    name: string;
    description: string;
  }

  interface OauthAccountInfo {
    provider: ProviderId;
    account_id: string;
  }

  const NATIVE_CONNECTORS: NativeConnectorCard[] = [
    {
      id: "google",
      name: "Google Workspace",
      description: "Gmail (envoi + brouillons), Calendar, Drive Workspace",
    },
    {
      id: "microsoft",
      name: "Microsoft 365",
      description: "Outlook Mail, Outlook Calendar, OneDrive",
    },
  ];

  let nativeAccounts = $state<OauthAccountInfo[]>([]);
  let nativeLoading = $state(false);
  let nativeError = $state<string | null>(null);
  let oauthDialogOpen = $state(false);
  let oauthDialogProvider = $state<ProviderId | null>(null);
  let oauthDialogAuthUrl = $state<string | null>(null);
  let oauthDialogState = $state<string | null>(null);
  let oauthDialogPastedCode = $state("");
  let oauthDialogError = $state<string | null>(null);
  let oauthDialogBusy = $state(false);

  async function refreshNativeStatus() {
    nativeLoading = true;
    nativeError = null;
    try {
      nativeAccounts = await invoke<OauthAccountInfo[]>("oauth_get_status");
    } catch (e) {
      nativeError = formatTauriError(e);
    } finally {
      nativeLoading = false;
    }
  }

  function accountsForProvider(provider: ProviderId): OauthAccountInfo[] {
    return nativeAccounts.filter((a) => a.provider === provider);
  }

  function formatTauriError(e: unknown): string {
    if (typeof e === "string") return e;
    const anyE = e as { kind?: string; detail?: string; message?: string };
    if (anyE.kind === "sovereignty_blocked") {
      return "Profil souveraineté « local-only » : connecteurs cloud désactivés.";
    }
    if (anyE.kind === "oauth_client_not_configured") {
      return "Client OAuth non configuré dans ce build. Voir docs/internal/release/OAUTH-CLIENT-IDS.md.";
    }
    if (anyE.kind && anyE.detail) {
      return `${anyE.kind}: ${anyE.detail}`;
    }
    if (anyE.message) return anyE.message;
    if (anyE.kind) return anyE.kind;
    return String(e);
  }

  async function startNativeConnect(provider: ProviderId) {
    oauthDialogProvider = provider;
    oauthDialogAuthUrl = null;
    oauthDialogState = null;
    oauthDialogPastedCode = "";
    oauthDialogError = null;
    oauthDialogBusy = true;
    oauthDialogOpen = true;
    try {
      // Default scopes mirror connector_providers.rs defaults — let the
      // user trim later via "Gérer le compte" once we expose a scope picker.
      const defaultScopes =
        provider === "google"
          ? [
              "mail.send",
              "mail.compose",
              "calendar.read",
              "calendar.write",
              "drive.workspace",
            ]
          : [
              "mail.read",
              "mail.send",
              "calendar.read",
              "calendar.write",
              "files.read",
            ];
      const start = await invoke<{
        auth_url: string;
        state: string;
        callback_port: number;
      }>("oauth_start_flow", {
        provider,
        scopes: defaultScopes,
        sovereignty: "cloud_allowed",
      });
      oauthDialogAuthUrl = start.auth_url;
      oauthDialogState = start.state;
      // Open the system browser on the auth URL.
      window.open(start.auth_url, "_blank");
    } catch (e) {
      oauthDialogError = formatTauriError(e);
    } finally {
      oauthDialogBusy = false;
    }
  }

  async function completeNativeFlow() {
    if (!oauthDialogState || !oauthDialogPastedCode) return;
    oauthDialogBusy = true;
    oauthDialogError = null;
    try {
      await invoke("oauth_complete_flow", {
        state: oauthDialogState,
        code: oauthDialogPastedCode.trim(),
      });
      oauthDialogOpen = false;
      await refreshNativeStatus();
    } catch (e) {
      oauthDialogError = formatTauriError(e);
    } finally {
      oauthDialogBusy = false;
    }
  }

  async function disconnectNative(account: OauthAccountInfo) {
    if (
      !confirm(
        `Déconnecter ${account.account_id} (${account.provider}) ?\nLe token sera révoqué localement.`,
      )
    ) {
      return;
    }
    try {
      await invoke("oauth_disconnect", {
        provider: account.provider,
        accountId: account.account_id,
      });
      await refreshNativeStatus();
    } catch (e) {
      nativeError = formatTauriError(e);
    }
  }

  // ── Loaders ────────────────────────────────────────────────────────────────

  /** Phase 1: load servers + enrichments immediately (fast, no network call).
   *  Phase 2: fetch the full registry catalogue in background. */
  async function loadAll(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [serverList, enrichmentEntries, agentList] = await Promise.all([
        invoke<McpServerStatusView[]>("list_mcp_servers"),
        invoke<Array<{ package_identifier: string; enrichment: ConnectorEnrichmentView }>>(
          "list_mcp_enrichments",
        ),
        invoke<AgentListItem[]>("list_agents").catch(() => [] as AgentListItem[]),
      ]);
      servers = serverList;
      enrichmentMap = new Map(
        enrichmentEntries.map((e) => [e.package_identifier, e.enrichment]),
      );
      agents = agentList;
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
    // Phase 2: fetch registry in background — page renders immediately above.
    void loadRegistry();
  }

  async function loadRegistry(): Promise<void> {
    registryLoading = true;
    try {
      registry = await invoke<RegistryServerView[]>("fetch_mcp_registry").catch(
        () => [] as RegistryServerView[],
      );
      // Reset pagination so curated enrichments aren't pushed off the first page.
      catalogLimit = CATALOG_PAGE;
    } finally {
      registryLoading = false;
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

  /**
   * Registry entries are keyed by the package identifier (e.g.
   * `@modelcontextprotocol/server-filesystem`) but the installed-server
   * `name` is the sanitized operator label (e.g. `local-files`). To bridge
   * the two, we index the installed servers by their `package` field so we
   * can map a registry entry → its actual installed server name.
   */
  const installedByPackage = $derived.by(() => {
    const map = new Map<string, string>();
    for (const s of servers) {
      if (s.package) map.set(s.package, s.name);
    }
    return map;
  });

  /** Resolve the installed server name (if any) for a registry catalogue entry. */
  function installedNameFor(entry: RegistryServerView): string | null {
    if (installedNames.has(entry.name)) return entry.name;
    const byPkg = installedByPackage.get(entry.name);
    if (byPkg) return byPkg;
    const pkgId = entry.packages?.[0]?.identifier;
    if (pkgId) {
      const match = installedByPackage.get(pkgId);
      if (match) return match;
    }
    return null;
  }

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
    mcpEntries.filter((e) => e.is_installed || installedNameFor(e) !== null).length,
  );

  const filteredMcp = $derived(
    mcpEntries.filter((e) => {
      const installed = e.is_installed || installedNameFor(e) !== null;
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

  // Visible slice — reset catalogLimit when filter changes to avoid showing 0 results.
  $effect(() => {
    void mcpFilter;
    catalogLimit = CATALOG_PAGE;
  });

  const visibleMcp = $derived(filteredMcp.slice(0, catalogLimit));
  const hasMoreMcp = $derived(filteredMcp.length > catalogLimit);

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

  /**
   * Open the custom MCP server dialog — a simpler form than the catalogue
   * wizard, used to register a server identified only by command/URL +
   * headers (no enrichment, no preset transport).
   */
  function handleAddCustomMcp() {
    customDialogOpen = true;
  }

  // Custom MCP server form state.
  let customForm = $state({
    name: "",
    transport: "stdio" as "stdio" | "streamable-http" | "sse",
    command: "",
    args: "",
    url: "",
    headers: "" as string, // free-form: "Header-Name=value" per line
    requires_approval: true,
  });
  let customBusy = $state(false);
  let customError = $state<string | null>(null);
  let customTestResult = $state<string | null>(null);

  function resetCustomForm() {
    customForm = {
      name: "",
      transport: "stdio",
      command: "",
      args: "",
      url: "",
      headers: "",
      requires_approval: true,
    };
    customError = null;
    customTestResult = null;
  }

  function sanitizeServerName(raw: string): string {
    const cleaned = raw
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "");
    return cleaned.length > 0 ? cleaned : "custom-mcp";
  }

  function buildCustomConfig(): Record<string, unknown> | null {
    const env: Record<string, string> = {};
    for (const line of customForm.headers.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      const eq = trimmed.indexOf("=");
      if (eq <= 0) continue;
      env[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
    }
    const base: Record<string, unknown> = {
      name: sanitizeServerName(customForm.name || "custom-mcp"),
      env,
      transport: customForm.transport,
      requires_approval: customForm.requires_approval,
      tags: [],
    };
    if (customForm.transport === "stdio") {
      if (!customForm.command.trim()) {
        customError = "Indiquez la commande à lancer (ex. npx, uvx, /usr/local/bin/myserver).";
        return null;
      }
      base.command = customForm.command.trim();
      base.args = customForm.args
        .split(/\s+/)
        .map((a) => a.trim())
        .filter((a) => a.length > 0);
    } else {
      if (!customForm.url.trim()) {
        customError = "Indiquez l'URL du serveur MCP (HTTP ou SSE).";
        return null;
      }
      base.url = customForm.url.trim();
    }
    return base;
  }

  async function testCustomServer() {
    customError = null;
    customTestResult = null;
    const config = buildCustomConfig();
    if (!config) return;
    customBusy = true;
    try {
      const result = await invoke<{ tools: { local_name: string }[] }>(
        "test_mcp_connection",
        { config },
      );
      customTestResult = `${result.tools.length} outil(s) détecté(s).`;
    } catch (e) {
      customError = formatTauriError(e);
    } finally {
      customBusy = false;
    }
  }

  async function installCustomServer() {
    customError = null;
    const config = buildCustomConfig();
    if (!config) return;
    customBusy = true;
    try {
      await invoke("add_mcp_server", { config });
      customDialogOpen = false;
      resetCustomForm();
      await loadAll();
    } catch (e) {
      customError = formatTauriError(e);
    } finally {
      customBusy = false;
    }
  }

  $effect(() => {
    loadAll();
    void refreshNativeStatus();
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
    subtitle={$t("connections.hero.subtitle")}
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

  <!-- ============ SECTION CONNECTEURS NATIFS APOLLIA (OAuth) ============ -->
  <SectionTitle
    count={$t(
      nativeAccounts.length === 1
        ? "connections.native_accounts_connected_one"
        : "connections.native_accounts_connected_other",
      { values: { count: nativeAccounts.length } },
    )}
  >
    {$t("connections.native_section_title")}
    {#snippet action()}
      <Chip tone="primary" size="sm">{$t("connections.native_section_tag")}</Chip>
    {/snippet}
  </SectionTitle>

  {#if nativeError}
    <p class="px-8 pb-2 text-xs text-destructive">{nativeError}</p>
  {/if}

  <div class="px-8 pb-4 grid grid-cols-2 gap-3" data-testid="connections-native-grid">
    {#each NATIVE_CONNECTORS as connector (connector.id)}
      {@const accounts = accountsForProvider(connector.id)}
      <div class="rounded-xl border border-border bg-surface-1 p-4 flex flex-col gap-2">
        <div class="flex items-center justify-between">
          <div>
            <div class="font-medium text-sm">{connector.name}</div>
            <div class="text-xs text-muted-foreground">{connector.description}</div>
          </div>
          <Chip tone={accounts.length > 0 ? "success" : "neutral"} size="sm" outline={accounts.length === 0}>
            {accounts.length > 0 ? `${accounts.length} actif` : "Non connecté"}
          </Chip>
        </div>

        {#if accounts.length > 0}
          <ul class="text-xs space-y-1 mt-1">
            {#each accounts as account (account.account_id)}
              <li class="flex items-center justify-between gap-2">
                <span class="truncate">{account.account_id}</span>
                <button
                  type="button"
                  class="text-destructive hover:underline text-[11px]"
                  onclick={() => disconnectNative(account)}
                  disabled={nativeLoading}
                >
                  Déconnecter
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        <div class="mt-1">
          <BtnPrimary onclick={() => startNativeConnect(connector.id)} disabled={nativeLoading}>
            {#snippet icon()}<Plus size={12} />{/snippet}
            {accounts.length > 0 ? "Ajouter un compte" : "Connecter un compte"}
          </BtnPrimary>
        </div>
      </div>
    {/each}
  </div>

  <!-- ============ SECTION SERVEURS MCP INSTALLÉS ============ -->
  <SectionTitle count={`${activeCount} actif${activeCount > 1 ? "s" : ""} · ${servers.length} total`}>
    Serveurs MCP installés
    {#snippet action()}
      <Chip tone="neutral" size="sm">protocole ouvert</Chip>
    {/snippet}
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
          Tous{mcpEntries.length > 0 ? ` · ${mcpEntries.length}` : registryLoading ? " · …" : ""}
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
          Officiels{officialCount > 0 ? ` · ${officialCount}` : ""}
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
          Communauté{communityCount > 0 ? ` · ${communityCount}` : ""}
        </button>
      </Chip>
    </div>

    <div class="px-8 pb-8">
      {#if loading || (registryLoading && mcpEntries.length === 0)}
        <div class="space-y-4">
          <div class="flex items-center gap-2 text-xs text-muted-foreground">
            <span class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true"></span>
            Chargement du catalogue MCP…
          </div>
          <div class="grid grid-cols-4 gap-3" data-testid="connections-mcp-loading">
            {#each Array(12) as _, i (i)}
              <div class="h-20 rounded-xl bg-surface-1 border border-border animate-pulse"></div>
            {/each}
          </div>
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
          {#each visibleMcp as entry (entry.name)}
            {@const installedName = installedNameFor(entry)}
            {@const installed = entry.is_installed || installedName !== null}
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
                  installed
                    ? handleManage(installedName ?? entry.name)
                    : handleConnect(entry)}
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
        {#if hasMoreMcp}
          <div class="mt-4 flex items-center justify-center gap-3">
            <BtnSecondary onclick={() => (catalogLimit += CATALOG_PAGE)}>
              Voir plus ({filteredMcp.length - catalogLimit} restants)
            </BtnSecondary>
            {#if registryLoading}
              <span class="text-xs text-muted-foreground">Chargement du catalogue…</span>
            {/if}
          </div>
        {:else if registryLoading}
          <div class="mt-4 flex justify-center">
            <span class="text-xs text-muted-foreground">Chargement du catalogue…</span>
          </div>
        {/if}
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

<!-- ============ NATIVE OAUTH DIALOG ============ -->
{#if oauthDialogOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" role="dialog" aria-modal="true">
    <div class="w-full max-w-md rounded-xl bg-surface-1 border border-border p-5 space-y-3">
      <h2 class="text-lg font-semibold">
        Connecter {oauthDialogProvider === "google" ? "Google Workspace" : "Microsoft 365"}
      </h2>

      {#if oauthDialogBusy && !oauthDialogAuthUrl}
        <p class="text-sm text-muted-foreground">Préparation du flow OAuth…</p>
      {:else if oauthDialogError && !oauthDialogAuthUrl}
        <p class="text-sm text-destructive">{oauthDialogError}</p>
      {:else if oauthDialogAuthUrl}
        <p class="text-sm text-muted-foreground">
          Une fenêtre navigateur s'est ouverte sur le consentement
          {oauthDialogProvider === "google" ? "Google" : "Microsoft"}. Validez
          les permissions, puis collez ici le code d'autorisation retourné.
        </p>
        <p class="text-xs text-muted-foreground">
          Si la fenêtre ne s'est pas ouverte : <a
            class="underline"
            href={oauthDialogAuthUrl}
            target="_blank"
            rel="noopener noreferrer">ouvrir l'URL manuellement</a
          >.
        </p>
        <input
          type="text"
          class="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
          placeholder={$t("connections.custom_mcp.code_placeholder")}
          bind:value={oauthDialogPastedCode}
          disabled={oauthDialogBusy}
        />
        {#if oauthDialogError}
          <p class="text-xs text-destructive">{oauthDialogError}</p>
        {/if}
      {/if}

      <div class="flex justify-end gap-2 pt-1">
        <BtnSecondary onclick={() => (oauthDialogOpen = false)}>{$t("common.cancel")}</BtnSecondary>
        {#if oauthDialogAuthUrl}
          <BtnPrimary
            onclick={completeNativeFlow}
            disabled={oauthDialogBusy || oauthDialogPastedCode.trim().length === 0}
          >
            {oauthDialogBusy ? $t("common.finalizing") : $t("common.finalize")}
          </BtnPrimary>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- ============ CUSTOM MCP DIALOG ============ -->
{#if customDialogOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" role="dialog" aria-modal="true">
    <div class="w-full max-w-xl rounded-xl bg-surface-1 border border-border p-5 space-y-3 overflow-auto max-h-[90vh]">
      <h2 class="text-lg font-semibold">Ajouter un serveur MCP personnalisé</h2>
      <p class="text-xs text-muted-foreground">
        Pour brancher un MCP server interne, communautaire, ou en cours de
        développement. Pour un service curé (Notion, GitHub, Local Files…),
        utilisez plutôt une carte du catalogue ci-dessous.
      </p>

      <div class="space-y-2">
        <label class="block text-xs font-medium">
          Nom (interne, unique)
          <input
            type="text"
            class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
            placeholder="ex. acme-internal"
            bind:value={customForm.name}
            disabled={customBusy}
          />
        </label>

        <label class="block text-xs font-medium">
          Transport
          <select
            class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
            bind:value={customForm.transport}
            disabled={customBusy}
          >
            <option value="stdio">stdio (subprocess local)</option>
            <option value="streamable-http">Streamable HTTP (MCP 2025-11-25)</option>
            <option value="sse">SSE (legacy 2024-11-05)</option>
          </select>
        </label>

        {#if customForm.transport === "stdio"}
          <label class="block text-xs font-medium">
            Commande
            <input
              type="text"
              class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1 text-sm font-mono"
              placeholder="npx, uvx, /path/to/binary…"
              bind:value={customForm.command}
              disabled={customBusy}
            />
          </label>
          <label class="block text-xs font-medium">
            Arguments (espace-séparé)
            <input
              type="text"
              class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1 text-sm font-mono"
              placeholder="@modelcontextprotocol/server-filesystem /tmp"
              bind:value={customForm.args}
              disabled={customBusy}
            />
          </label>
        {:else}
          <label class="block text-xs font-medium">
            URL du serveur
            <input
              type="url"
              class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1 text-sm font-mono"
              placeholder="https://mcp.example.com/v1"
              bind:value={customForm.url}
              disabled={customBusy}
            />
          </label>
        {/if}

        <label class="block text-xs font-medium">
          {customForm.transport === "stdio" ? "Variables d'environnement" : "Headers HTTP"} (une par ligne, format <code>NOM=valeur</code>)
          <textarea
            class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1 text-sm font-mono"
            rows="3"
            placeholder={customForm.transport === "stdio"
              ? "NOTION_TOKEN=ntn_xxxxx\nDEBUG=1"
              : "Authorization=Bearer xxx\nX-Custom-Header=value"}
            bind:value={customForm.headers}
            disabled={customBusy}
          ></textarea>
        </label>

        <label class="block text-xs font-medium flex items-center gap-2 pt-1">
          <input type="checkbox" bind:checked={customForm.requires_approval} disabled={customBusy} />
          Demander une approbation HITL pour chaque appel d'outil
        </label>
      </div>

      {#if customTestResult}
        <div class="rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300">
          ✓ Test OK — {customTestResult}
        </div>
      {/if}
      {#if customError}
        <div class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {customError}
        </div>
      {/if}

      <div class="flex justify-end gap-2 pt-1">
        <BtnSecondary
          onclick={() => {
            customDialogOpen = false;
            resetCustomForm();
          }}
          disabled={customBusy}>Annuler</BtnSecondary
        >
        <BtnSecondary onclick={testCustomServer} disabled={customBusy}>
          {customBusy ? "Test…" : "Tester"}
        </BtnSecondary>
        <BtnPrimary onclick={installCustomServer} disabled={customBusy}>
          {customBusy ? "Installation…" : "Installer"}
        </BtnPrimary>
      </div>
    </div>
  </div>
{/if}
