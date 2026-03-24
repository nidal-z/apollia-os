<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Bot, Download, Sparkles } from "lucide-svelte";
  import AgentCard from "../components/agents/AgentCard.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import CreateFromTemplateDialog from "../components/agents/CreateFromTemplateDialog.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";
  import type { ChatSessionSummary, CreateSessionRequest } from "$lib/types";

  const SKELETON_COUNT = 4;

  let installingAgent = $state(false);
  let installError = $state<string | null>(null);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);
  let detailAgent = $state<AgentListItem | null>(null);
  let detailOpen = $state(false);
  let showCreateDialog = $state(false);

  const activeAgents = $derived($agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"));
  const inactiveAgents = $derived($agents.filter((a) => a.runtime_status !== "active" && a.runtime_status !== "degraded"));

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
</script>

<div class="max-w-6xl" data-testid="agents-page">
  <!-- Header -->
  <div class="flex items-end justify-between">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight" data-testid="agents-header">{$t('agents.title')}</h1>
      <p class="mt-1 text-sm text-muted-foreground" data-testid="agents-subtitle">{$t('agents.subtitle')}</p>
    </div>
    <div class="flex items-center gap-2">
      <Button size="sm" variant="outline" onclick={() => showCreateDialog = true} data-testid="btn-new-agent" class="gap-1.5">
        <Sparkles size={13} />
        {$t('agents.new_agent')}
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
            <Skeleton class="h-8 w-8 rounded-lg" />
            <div class="flex-1 space-y-1.5">
              <Skeleton class="h-4 w-[60%]" />
              <Skeleton class="h-3 w-[40%]" />
            </div>
          </div>
          <Skeleton class="mt-2.5 h-8 w-full" />
          <Skeleton class="mt-2 h-6 w-[50%]" />
        </div>
      {/each}
    </div>
  {:else if $agents.length === 0}
    <div class="mt-6">
      <EmptyState
        icon={Bot}
        title={$t('agents.empty_title_install')}
        subtitle={$t('agents.empty_subtitle_install')}
        ctaLabel={$t('agents.install')}
        ctaAction={pickAndInstallAgent}
        page="agents"
      />
    </div>
  {:else}
    <!-- Active agents section -->
    {#if activeAgents.length > 0}
      <div class="mt-5">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('agents.section_active')}</h2>
          <span class="text-xs text-muted-foreground/50">{activeAgents.length}</span>
        </div>
        <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-grid-active">
          {#each activeAgents as agent (agent.name)}
            <div class="h-full" animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200 }}>
              <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} onchat={startChatWithAgent} />
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Inactive agents section -->
    {#if inactiveAgents.length > 0}
      <div class="mt-5">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('agents.section_inactive')}</h2>
          <span class="text-xs text-muted-foreground/50">{inactiveAgents.length}</span>
        </div>
        <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-grid-inactive">
          {#each inactiveAgents as agent (agent.name)}
            <div class="h-full" animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200 }}>
              <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} onchat={startChatWithAgent} />
            </div>
          {/each}
        </div>
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

<CreateFromTemplateDialog
  open={showCreateDialog}
  onclose={() => showCreateDialog = false}
  oncreated={() => showCreateDialog = false}
/>
