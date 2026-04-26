<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { get } from "svelte/store";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { tourOpenAgentDetail } from "$lib/stores/tour";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Bot, Download, Zap, Package } from "lucide-svelte";
  import AgentCard from "../components/agents/AgentCard.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentPackageCard from "../components/agents/AgentPackageCard.svelte";
  import AgentPackageDetail from "../components/agents/AgentPackageDetail.svelte";
  import InstallPackageDialog from "../components/agents/InstallPackageDialog.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import { EmptyState } from "$lib/components/layout";
  import { EMPTY_STATES } from "$lib/i18n/strings/empty-states";
  import type { AgentPackageListItem, AgentPackageDetailView, ChatSessionSummary, CreateSessionRequest, InstallPackageResponse } from "$lib/types";
  import { agentPackages, refreshPackages, uninstallPackage, getPackageDetail } from "$lib/stores/agentPackages";
  import { addToast } from "$lib/components/ui/toast/store";

  const SKELETON_COUNT = 4;

  let installingAgent = $state(false);
  let installError = $state<string | null>(null);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);
  let detailAgent = $state<AgentListItem | null>(null);
  let detailOpen = $state(false);

  // Package state
  let installPackageOpen = $state(false);
  let pkgDetail = $state<AgentPackageDetailView | null>(null);
  let pkgDetailOpen = $state(false);

  $effect(() => { refreshPackages(); });

  // Split workers from assistants using agent_type (canonical field).
  // supports_a2a is true for both populations — it is not a valid discriminant.
  const allWorkers = $derived($agents.filter((a) => a.agent_type === "worker"));
  const allAssistants = $derived($agents.filter((a) => a.agent_type !== "worker"));

  const activeAssistants = $derived(allAssistants.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"));
  const inactiveAssistants = $derived(allAssistants.filter((a) => a.runtime_status !== "active" && a.runtime_status !== "degraded"));
  const activeWorkers = $derived(allWorkers.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"));
  const inactiveWorkers = $derived(allWorkers.filter((a) => a.runtime_status !== "active" && a.runtime_status !== "degraded"));

  async function pickAndInstallAgent() {
    installError = null;
    try {
      const path = await openDialog({
        filters: [{ name: "Python Agent", extensions: ["py"] }],
        multiple: false,
      });
      if (!path) return;
      installingAgent = true;
      await invoke("install_agent", { path });
    } catch (err: unknown) {
      installError = err instanceof Error ? err.message : String(err);
    } finally {
      installingAgent = false;
    }
  }

  function openLogs(agentId: string) { logsAgentId = agentId; logsOpen = true; }
  function closeLogs() { logsOpen = false; }
  function openDetail(agent: AgentListItem) { detailAgent = agent; detailOpen = true; }
  function closeDetail() { detailOpen = false; }
  function openLogsFromDetail(agentId: string) { closeDetail(); openLogs(agentId); }

  async function startChatWithAgent(agentName: string): Promise<void> {
    try {
      const request: CreateSessionRequest = { mode: "agent", agent_name: agentName };
      const session = await invoke<ChatSessionSummary>("create_chat_session", { request });
      pendingChatSessionId.set(session.id);
      navigateTo("chat");
    } catch {
      navigateTo("chat");
    }
  }

  async function openPkgDetail(pkg: AgentPackageListItem) {
    try { pkgDetail = await getPackageDetail(pkg.name); pkgDetailOpen = true; }
    catch { pkgDetailOpen = false; }
  }

  async function handleUninstallPkg(pkg: AgentPackageListItem) {
    try {
      await uninstallPackage(pkg.name);
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function handlePkgInstalled(_result: InstallPackageResponse) {
    await refreshPackages();
  }

  // Open the agent detail panel when the tour requests it programmatically.
  $effect(() => {
    return tourOpenAgentDetail.subscribe((agentName) => {
      if (agentName === null) return;
      const found = get(agents).find((a) => a.name === agentName);
      if (found !== undefined) {
        openDetail(found);
      }
    });
  });
</script>

<div class="mx-auto w-full max-w-6xl" data-testid="agents-page">
  <!-- Header -->
  <div class="flex items-end justify-between">
    <div>
      <h1 class="text-display-lg text-foreground" data-testid="agents-header">{$t('agents.title')}</h1>
      <p class="mt-2 text-sm text-muted-foreground md:text-base" data-testid="agents-subtitle">{$t('agents.subtitle')}</p>
    </div>
    <div class="flex items-center gap-2">
      <Button size="sm" variant="outline" onclick={() => (installPackageOpen = true)} class="gap-1.5">
        <Package size={13} />
        Installer un package
      </Button>
      <Button size="sm" onclick={pickAndInstallAgent} disabled={installingAgent} data-testid="install-agent-button" class="gap-1.5">
        <Download size={13} />
        {installingAgent ? $t('agents.installing') : $t('agents.install')}
      </Button>
    </div>
  </div>

  <!-- Sandbox banner -->
  <div class="mt-4">
    <MacSandboxBanner />
  </div>

  {#if installError}
    <div class="mt-3 rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
      {installError}
    </div>
  {/if}

  <!-- Content -->
  {#if $connectionStatus === "connecting"}
    <div class="mt-5 grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-skeleton">
      {#each { length: SKELETON_COUNT } as _}
        <div class="glass-card glass-border rounded-lg p-3.5">
          <div class="flex items-center gap-2.5">
            <Skeleton variant="avatar" class="h-8 w-8 rounded-lg" />
            <div class="flex-1 space-y-1.5">
              <Skeleton variant="text" class="w-[60%]" />
              <Skeleton variant="text" class="h-3 w-[40%]" />
            </div>
          </div>
          <Skeleton variant="card" class="mt-2.5 h-8" />
          <Skeleton variant="text" class="mt-2 h-6 w-[50%]" />
        </div>
      {/each}
    </div>
  {:else}
    <!-- ── Assistants ──────────────────────────────────────────────────── -->
    {#if allAssistants.length > 0}
      <div class="mt-6">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="flex items-center gap-1.5 text-sm font-medium uppercase tracking-wider text-muted-foreground">
            <Bot size={14} />{$t('agents.section_assistants')}
          </h2>
          <span class="text-xs text-muted-foreground/50">{allAssistants.length}</span>
        </div>

        {#if activeAssistants.length > 0}
          <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-grid-active">
            {#each activeAssistants as agent (agent.name)}
              <div class="h-full" animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200 }}>
                <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} onchat={startChatWithAgent} />
              </div>
            {/each}
          </div>
        {/if}

        {#if inactiveAssistants.length > 0}
          <div class="mt-3 grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3 opacity-60" data-testid="agents-grid-inactive">
            {#each inactiveAssistants as agent (agent.name)}
              <div class="h-full" animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200 }}>
                <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} onchat={startChatWithAgent} />
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- ── Workers A2A ─────────────────────────────────────────────────── -->
    {#if allWorkers.length > 0}
      <div class="mt-6">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="flex items-center gap-1.5 text-sm font-medium uppercase tracking-wider text-muted-foreground">
            <Zap size={13} class="text-secondary/70" />{$t('agents.section_workers')}
          </h2>
          <span class="text-xs text-muted-foreground/50">
            {activeWorkers.length}/{allWorkers.length} {$t('agents.workers_active_suffix')}
          </span>
        </div>
        <p class="mb-3 text-xs text-muted-foreground/60">{$t('agents.workers_hint')}</p>

        {#if activeWorkers.length > 0}
          <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-grid-workers-active">
            {#each activeWorkers as agent (agent.name)}
              <div class="h-full" animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200 }}>
                <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} />
              </div>
            {/each}
          </div>
        {/if}

        {#if inactiveWorkers.length > 0}
          <div class="mt-3 grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3 opacity-60" data-testid="agents-grid-workers-inactive">
            {#each inactiveWorkers as agent (agent.name)}
              <div class="h-full" animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200 }}>
                <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} />
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- ── Packages ──────────────────────────────────────────────────────── -->
    {#if $agentPackages.length > 0}
      <div class="mt-6">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="flex items-center gap-1.5 text-sm font-medium uppercase tracking-wider text-muted-foreground">
            <Package size={13} class="text-primary/70" />Packages
          </h2>
          <span class="text-xs text-muted-foreground/50">{$agentPackages.length}</span>
        </div>
        <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {#each $agentPackages as pkg (pkg.name)}
            <div class="h-full" in:fly={{ y: 8, duration: 200 }}>
              <AgentPackageCard
                {pkg}
                ondetail={openPkgDetail}
                onuninstall={handleUninstallPkg}
              />
            </div>
          {/each}
        </div>
      </div>
    {/if}

    {#if allAssistants.length === 0 && allWorkers.length === 0}
      <div class="mt-6">
        <EmptyState
          icon={EMPTY_STATES.agents.icon}
          title={$t(EMPTY_STATES.agents.titleKey)}
          description={$t(EMPTY_STATES.agents.descriptionKey)}
          primaryLabel={$t(EMPTY_STATES.agents.primaryCtaKey ?? '')}
          primaryAction={pickAndInstallAgent}
          page="agents"
        />
      </div>
    {/if}
  {/if}
</div>

{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}
{#if detailAgent}
  <AgentDetail agent={detailAgent} open={detailOpen} onclose={closeDetail} onlogs={openLogsFromDetail} />
{/if}

<InstallPackageDialog
  open={installPackageOpen}
  onclose={() => (installPackageOpen = false)}
  oninstalled={handlePkgInstalled}
/>

<AgentPackageDetail
  pkg={pkgDetail}
  open={pkgDetailOpen}
  onclose={() => (pkgDetailOpen = false)}
  onuninstall={async (name) => {
    try { await uninstallPackage(name); }
    catch (err) { addToast(err instanceof Error ? err.message : String(err), "error"); }
  }}
/>

