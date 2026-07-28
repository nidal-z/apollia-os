<script lang="ts">
  /**
   * McpServerDetail - self-contained detail pane for an installed MCP server.
   *
   * Owns the lazy detail fetch and the test / reconnect / disconnect / approval
   * flows (via `$lib/ipc/connections`), keeping the route orchestrator thin. The
   * header status reflects live health via the `server` prop plus a local test
   * verdict override, cleared on the next `McpServerHealthChanged` event.
   */
  import { t } from "svelte-i18n";
  import { Spinner } from "$lib/components/ui/progress";
  import { TabBar } from "$lib/components/ui/tabs";
  import { formatRelativeTime } from "$lib/utils";
  import McpDetailHeader from "./McpDetailHeader.svelte";
  import McpOverviewTab from "./McpOverviewTab.svelte";
  import McpToolsTab from "./McpToolsTab.svelte";
  import McpSettingsTab from "./McpSettingsTab.svelte";
  import { statusFromHealth, syncDescriptor, testOutcomeFromHealth } from "$lib/connections/status";
  import { messageOf } from "$lib/connections/errors";
  import {
    getMcpServerDetail,
    setMcpServerApproval,
    testMcpLiveServer,
    restartMcpServer,
    deleteMcpSecret,
    removeMcpServer,
  } from "$lib/ipc/connections";
  import type {
    McpServerStatusView,
    McpServerDetailView,
    McpHealth,
    ConnectorEnrichmentView,
  } from "$lib/types";
  import type { ApprovalLevel, McpTab, TestState } from "../shared/types";

  interface Props {
    server: McpServerStatusView;
    enrichment: ConnectorEnrichmentView | null;
    tab: McpTab;
    ontabchange: (tab: McpTab) => void;
    /** Refresh the sidebar list (post reconnect). */
    onchanged: () => void;
    /** Clear selection + reload (post disconnect). */
    ondisconnected: () => void;
  }

  let { server, enrichment, tab, ontabchange, onchanged, ondisconnected }: Props = $props();

  let mcpDetail = $state<McpServerDetailView | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let approvalLevel = $state<ApprovalLevel>("ask");
  let approvalPending = $state(false);
  let approvalError = $state<string | null>(null);
  let testState = $state<TestState>("idle");
  let testToolCount = $state(0);
  let testDetail = $state<string | null>(null);
  let testError = $state<string | null>(null);
  let reconnecting = $state(false);
  let reconnectError = $state<string | null>(null);
  let confirmDisconnect = $state(false);
  let disconnecting = $state(false);
  let disconnectError = $state<string | null>(null);
  let localHealth = $state<McpHealth | null>(null);

  const label = $derived(enrichment?.operator_label ?? server.name);
  const displayHealth = $derived(localHealth ?? server.health);
  const displayStatus = $derived(statusFromHealth(displayHealth));
  const sync = $derived(syncDescriptor(server.last_call_at));
  const lastActivityLabel = $derived(
    mcpDetail?.status.last_call_at ? formatRelativeTime(mcpDetail.status.last_call_at) : null,
  );

  function deriveApprovalLevel(requiresApproval: boolean): ApprovalLevel {
    return requiresApproval ? "ask" : "auto";
  }

  async function fetchDetail(name: string): Promise<void> {
    loading = true;
    error = null;
    try {
      mcpDetail = await getMcpServerDetail(name);
      approvalLevel = deriveApprovalLevel(mcpDetail.config.requires_approval);
    } catch (err: unknown) {
      error = messageOf(err);
      mcpDetail = null;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    const name = server.name;
    mcpDetail = null; testState = "idle"; testDetail = null; testError = null;
    reconnectError = null; confirmDisconnect = false; disconnectError = null;
    approvalError = null; localHealth = null;
    void fetchDetail(name);
  });

  $effect(() => {
    void server.health;
    localHealth = null;
  });

  async function handleApprovalChange(newLevel: ApprovalLevel): Promise<void> {
    if (!mcpDetail) return;
    approvalLevel = newLevel;
    approvalPending = true;
    approvalError = null;
    const requiresApproval = newLevel === "ask";
    try {
      await setMcpServerApproval(server.name, requiresApproval);
      mcpDetail = {
        ...mcpDetail,
        config: { ...mcpDetail.config, requires_approval: requiresApproval },
        status: { ...mcpDetail.status, requires_approval: requiresApproval },
      };
    } catch (err: unknown) {
      approvalError = messageOf(err);
      approvalLevel = deriveApprovalLevel(mcpDetail.config.requires_approval);
    } finally {
      approvalPending = false;
    }
  }

  async function handleTest(): Promise<void> {
    testState = "testing";
    testError = null;
    testDetail = null;
    try {
      const res = await testMcpLiveServer(server.name);
      if (res.kind === "oauth_required") {
        testState = "needs_reauth";
        return;
      }
      testToolCount = res.tools.length;
      const health = res.live_health ?? null;
      if (health) localHealth = health;
      const outcome = testOutcomeFromHealth(health);
      testState = outcome.verdict;
      testDetail = outcome.detail ?? null;
      testError = outcome.error ?? null;
    } catch (err: unknown) {
      testState = "error";
      testError = messageOf(err);
    }
  }

  async function handleReconnect(): Promise<void> {
    reconnecting = true;
    reconnectError = null;
    testState = "idle";
    testDetail = null;
    try {
      const updated = await restartMcpServer(server.name);
      if (mcpDetail) mcpDetail = { ...mcpDetail, status: updated };
      localHealth = updated.health;
      approvalLevel = deriveApprovalLevel(updated.requires_approval);
      onchanged();
    } catch (err: unknown) {
      reconnectError = messageOf(err);
    } finally {
      reconnecting = false;
    }
  }

  async function handleDisconnect(): Promise<void> {
    if (!mcpDetail) return;
    disconnecting = true;
    disconnectError = null;
    try {
      for (const envKey of mcpDetail.config.env_keys) {
        await deleteMcpSecret(server.name, envKey);
      }
      await removeMcpServer(server.name);
      ondisconnected();
    } catch (err: unknown) {
      disconnectError = messageOf(err);
    } finally {
      disconnecting = false;
    }
  }
</script>

{#if loading && !mcpDetail}
  <div class="flex flex-1 items-center justify-center text-body-sm text-muted-foreground">
    <Spinner size={14} class="mr-2" />
    {$t("connections.loading")}
  </div>
{:else if error}
  <div class="flex flex-1 items-center justify-center px-8 text-body-sm text-destructive">{error}</div>
{:else if mcpDetail}
  <McpDetailHeader
    {label}
    meta={enrichment?.category ?? server.server_info ?? $t("connections.mcp_installed_fallback")}
    status={displayStatus}
    {sync}
    testing={testState === "testing"}
    {reconnecting}
    ontest={handleTest}
    onreconnect={handleReconnect}
  />

  <div class="px-8 pt-3.5">
    <TabBar
      variant="underline"
      testidPrefix="mcp-detail"
      activeTab={tab}
      items={[
        { key: "overview", label: $t("connections.mcp_tab_overview") },
        { key: "tools", label: $t("connections.mcp_tab_tools"), count: mcpDetail.tools.length },
        { key: "settings", label: $t("connections.mcp_tab_settings") },
      ]}
      ontabchange={(k) => ontabchange(k as McpTab)}
    />
  </div>

  <div class="flex-1 overflow-auto px-8 py-5">
    {#if tab === "overview"}
      <McpOverviewTab
        health={displayHealth}
        toolsCount={mcpDetail.status.tools_count}
        statusError={mcpDetail.status.error}
        {lastActivityLabel}
        {testState}
        {testToolCount}
        {testDetail}
        {testError}
        {reconnectError}
      />
    {:else if tab === "tools"}
      <McpToolsTab tools={mcpDetail.tools} />
    {:else}
      <McpSettingsTab
        serverName={server.name}
        {approvalLevel}
        {approvalPending}
        {approvalError}
        {confirmDisconnect}
        {disconnecting}
        {disconnectError}
        onapprovalchange={handleApprovalChange}
        onsaved={() => void fetchDetail(server.name)}
        onrequestdisconnect={() => (confirmDisconnect = true)}
        oncanceldisconnect={() => {
          confirmDisconnect = false;
          disconnectError = null;
        }}
        onconfirmdisconnect={handleDisconnect}
      />
    {/if}
  </div>
{/if}
