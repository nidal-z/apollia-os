<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus } from "$lib/stores/sse";
  import { currentRoute } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Bot } from "lucide-svelte";
  import AgentCard from "../components/agents/AgentCard.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";
  import type { ChatSessionSummary, CreateSessionRequest } from "$lib/types";

  const SKELETON_COUNT = 3;

  let installingAgent = $state(false);
  let installError = $state<string | null>(null);

  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  let detailAgent = $state<AgentListItem | null>(null);
  let detailOpen = $state(false);

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
      const message = err instanceof Error ? err.message : String(err);
      installError = message;
    } finally {
      installingAgent = false;
    }
  }

  function openLogs(agentId: string) {
    logsAgentId = agentId;
    logsOpen = true;
  }

  function closeLogs() {
    logsOpen = false;
  }

  function openDetail(agent: AgentListItem) {
    detailAgent = agent;
    detailOpen = true;
  }

  function closeDetail() {
    detailOpen = false;
  }

  function openLogsFromDetail(agentId: string) {
    closeDetail();
    openLogs(agentId);
  }

  async function startChatWithAgent(agentName: string): Promise<void> {
    try {
      const request: CreateSessionRequest = { mode: "agent", agent_name: agentName };
      const session = await invoke<ChatSessionSummary>("create_chat_session", { request });
      pendingChatSessionId.set(session.id);
      currentRoute.set("chat");
    } catch {
      currentRoute.set("chat");
    }
  }
</script>

<div class="space-y-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h1 class="text-2xl font-bold" data-testid="agents-header">{$t('agents.title')}</h1>
      <p class="text-sm text-muted-foreground" data-testid="agents-subtitle">{$t('agents.subtitle')}</p>
    </div>
    <Button onclick={pickAndInstallAgent} disabled={installingAgent} data-testid="install-agent-button">
      {installingAgent ? $t('agents.installing') : $t('agents.install')}
    </Button>
  </div>

  <MacSandboxBanner />

  {#if installError}
    <div class="rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-4 py-2 text-sm text-[hsl(var(--destructive))]">
      {installError}
    </div>
  {/if}

  <!-- Agent list, skeleton loaders, or empty state -->
  {#if $connectionStatus === "connecting"}
    <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-skeleton">
      {#each { length: SKELETON_COUNT } as _}
        <div class="rounded-xl glass-card glass-border">
          <div class="px-4 pt-4 pb-3">
            <div class="flex items-start gap-3">
              <Skeleton class="h-10 w-10 shrink-0 rounded-full" />
              <div class="flex-1 space-y-2">
                <div class="flex items-center justify-between">
                  <Skeleton class="h-5 w-[50%]" />
                  <Skeleton class="h-5 w-16 rounded-full" />
                </div>
                <Skeleton class="h-3 w-[80%]" />
              </div>
            </div>
          </div>
          <div class="border-t px-4 py-2.5">
            <Skeleton class="h-3 w-[40%]" />
          </div>
          <div class="border-t px-4 py-2">
            <div class="flex gap-2">
              <Skeleton class="h-8 w-16 rounded-md" />
              <Skeleton class="h-8 w-14 rounded-md" />
            </div>
          </div>
        </div>
      {/each}
    </div>
  {:else if $agents.length === 0}
    <EmptyState
      icon={Bot}
      title={$t('agents.empty_title_install')}
      subtitle={$t('agents.empty_subtitle_install')}
      ctaLabel={$t('agents.install')}
      ctaAction={pickAndInstallAgent}
      page="agents"
    />
  {:else}
    <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-grid">
      {#each $agents as agent (agent.name)}
        <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
          <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} onchat={startChatWithAgent} />
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- Logs drawer -->
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}

<!-- Agent detail sheet -->
{#if detailAgent}
  <AgentDetail agent={detailAgent} open={detailOpen} onclose={closeDetail} onlogs={openLogsFromDetail} />
{/if}

