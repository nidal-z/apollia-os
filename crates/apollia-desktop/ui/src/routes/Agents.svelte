<script lang="ts">
  /**
   * Agents route - thin orchestrator.
   *
   * Owns selection + effects and composes the decomposed pieces: the sidebar
   * (AgentListPanel), the detail panes (ApolliaChatDetail / PackageDetail /
   * AgentDetailHeader + AgentDetailTabs), and the install dialog / logs sheet.
   * All rendering lives in `components/agents/*`; all IPC goes through
   * `$lib/ipc/agents`.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import {
    agentPackages,
    getPackageDetail,
    refreshPackages,
    uninstallPackage,
  } from "$lib/stores/agentPackages";
  import { SplitLayout, ErrorBanner } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { createChatSession, installAgent } from "$lib/ipc/agents";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import InstallPackageDialog from "../components/agents/InstallPackageDialog.svelte";
  import AgentListPanel from "../components/agents/AgentListPanel.svelte";
  import AgentDetailPane from "../components/agents/AgentDetailPane.svelte";
  import { createAgentActions } from "../components/agents/useAgentActions.svelte";
  import type {
    AgentListItem,
    AgentPackageDetailView,
    AgentPackageListItem,
    ChatSessionSummary,
    CreateSessionRequest,
    InstallPackageResponse,
  } from "$lib/types";

  type AgentTab = "overview" | "tools" | "memory" | "activity" | "settings";

  const actions = createAgentActions();

  // ── Selection + local UI state ───────────────────────────────────────
  let query = $state("");
  let selectedName = $state<string | null>(null);
  let selectedPackageName = $state<string | null>(null);
  let apolliaChatSelected = $state(false);
  let agentDetailTab = $state<AgentTab>("overview");
  let pkgDetail = $state<AgentPackageDetailView | null>(null);

  let installingAgent = $state(false);
  // The humanized error, not just its sentence. Installing an agent fails for
  // exactly one interesting reason, a Python module the loader refuses, and the
  // Python cause is the only actionable part. Keeping the string alone reduced
  // "@agent() missing a required argument: 'version'" to "an unexpected error
  // stopped this action", which tells the operator nothing to act on.
  let installError = $state<HumanizedError | null>(null);
  let installPackageOpen = $state(false);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  $effect(() => {
    refreshPackages();
  });

  // Assistants only; workers stay accessible via the package section.
  const allAssistants = $derived($agents.filter((a) => a.agent_type !== "worker"));

  const filteredAssistants = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q.length === 0) return allAssistants;
    return allAssistants.filter(
      (a) =>
        a.name.toLowerCase().includes(q) ||
        (a.description ?? "").toLowerCase().includes(q),
    );
  });

  const filteredPackages = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q.length === 0) return $agentPackages;
    return $agentPackages.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        (p.description ?? "").toLowerCase().includes(q),
    );
  });

  const selected = $derived(
    selectedName === null
      ? null
      : ($agents.find((a) => a.name === selectedName) ?? null),
  );
  const selectedPackage = $derived(
    selectedPackageName === null
      ? null
      : ($agentPackages.find((p) => p.name === selectedPackageName) ?? null),
  );

  // Auto-select the first assistant when nothing else is selected.
  $effect(() => {
    if (selectedPackageName !== null || apolliaChatSelected) return;
    if (
      filteredAssistants.length > 0 &&
      (selectedName === null ||
        !filteredAssistants.some((a) => a.name === selectedName))
    ) {
      selectedName = filteredAssistants[0].name;
    }
    if (filteredAssistants.length === 0) selectedName = null;
  });

  // Reset the agent detail tab when the agent selection changes.
  $effect(() => {
    void selectedName;
    agentDetailTab = "overview";
  });

  // Clear a package selection if it gets uninstalled.
  $effect(() => {
    if (selectedPackageName !== null && selectedPackage === null) {
      selectedPackageName = null;
      pkgDetail = null;
    }
  });

  // ── Selection handlers ───────────────────────────────────────────────
  function selectAgent(name: string): void {
    selectedName = name;
    apolliaChatSelected = false;
    selectedPackageName = null;
  }
  function selectApolliaChat(): void {
    apolliaChatSelected = true;
    selectedName = null;
    selectedPackageName = null;
  }
  async function selectPackage(pkg: AgentPackageListItem): Promise<void> {
    selectedPackageName = pkg.name;
    selectedName = null;
    apolliaChatSelected = false;
    pkgDetail = null;
    try {
      pkgDetail = await getPackageDetail(pkg.name);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  // ── Actions ──────────────────────────────────────────────────────────
  async function pickAndInstallAgent(): Promise<void> {
    installError = null;
    try {
      const path = await openDialog({
        filters: [{ name: "Python Agent", extensions: ["py"] }],
        multiple: false,
      });
      if (!path) return;
      installingAgent = true;
      await installAgent(path);
    } catch (err) {
      installError = reportError(err, { surface: "inline" });
    } finally {
      installingAgent = false;
    }
  }

  async function handleUninstallPkg(name: string): Promise<void> {
    try {
      await uninstallPackage(name);
      if (selectedPackageName === name) {
        selectedPackageName = null;
        pkgDetail = null;
      }
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  async function startChatWithAgent(agentName: string): Promise<void> {
    try {
      const request: CreateSessionRequest = { mode: "agent", agent_name: agentName };
      const session: ChatSessionSummary = await createChatSession(request);
      pendingChatSessionId.set(session.id);
    } catch {
      // Fall through to the chat route even if session creation failed.
    }
    navigateTo("chat");
  }

  function openLogs(agentId: string): void {
    logsAgentId = agentId;
    logsOpen = true;
  }
  function openSettings(agent: AgentListItem): void {
    selectAgent(agent.name);
    agentDetailTab = "settings";
  }
  async function handlePkgInstalled(_result: InstallPackageResponse): Promise<void> {
    await refreshPackages();
  }
</script>

<div class="flex h-full min-h-0 w-full flex-col" data-testid="agents-page">
  <div class="px-8 pt-3">
    <MacSandboxBanner />
    {#if installError}
      <ErrorBanner class="mt-3" data-testid="agent-install-error">
        <p class="font-medium">{installError.friendly_message}</p>
        <p class="text-caption opacity-90">{installError.suggested_action}</p>
        {#if installError.detail}
          <details class="mt-2">
            <summary
              class="cursor-pointer text-caption opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              data-testid="agent-install-error-details"
            >
              {$t("errors.show_details")}
            </summary>
            <p class="mt-1.5 break-all font-mono text-code-sm opacity-80">
              {installError.detail}
            </p>
          </details>
        {/if}
      </ErrorBanner>
    {/if}
  </div>

  <SplitLayout
    sidebarWidth={320}
    sidebarTestid="agents-list"
    sidebarLabel={$t("agents.page_title")}
  >
    {#snippet sidebar()}
      <AgentListPanel
        assistants={filteredAssistants}
        packages={filteredPackages}
        bind:query
        {selectedName}
        {selectedPackageName}
        {apolliaChatSelected}
        connecting={$connectionStatus === "connecting"}
        {installingAgent}
        agentActions={actions}
        onSelectAgent={selectAgent}
        onSelectPackage={selectPackage}
        onSelectApolliaChat={selectApolliaChat}
        onInstallAgent={pickAndInstallAgent}
        onInstallPackage={() => (installPackageOpen = true)}
        onOpenLogs={openLogs}
        onOpenSettings={openSettings}
        onUninstallPackage={handleUninstallPkg}
      />
    {/snippet}

    <div class="min-h-0 flex-1 overflow-y-auto">
      <AgentDetailPane
        {selected}
        {selectedPackage}
        {apolliaChatSelected}
        {pkgDetail}
        connecting={$connectionStatus === "connecting"}
        agentActions={actions}
        bind:agentDetailTab
        onStartChat={startChatWithAgent}
        onOpenLogs={openLogs}
        onNavigateMemory={() => navigateTo("memory")}
        onNavigateTasks={() => navigateTo("tasks")}
        onSelectAgent={selectAgent}
        onInstallAgent={pickAndInstallAgent}
        onUninstallPackage={handleUninstallPkg}
      />
    </div>
  </SplitLayout>
</div>

{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={() => (logsOpen = false)} />
{/if}

<InstallPackageDialog
  open={installPackageOpen}
  onclose={() => (installPackageOpen = false)}
  oninstalled={handlePkgInstalled}
/>
